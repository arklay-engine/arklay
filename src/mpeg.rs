//! The shipped RE2 films: MPEG-1 program streams (`movie/<name>.mpg`, the
//! install's `Zmovie/*.bin`) carrying 640x480 MPEG-1 video at 29.97 fps
//! and 44.1 kHz stereo MPEG-1 Layer II audio.
//!
//! Like the RE1 Cinepak films, the shipped format is decoded by the port's
//! own code rather than a codec crate, so every build plays it: [`ps`]
//! demuxes the program stream, [`video`] decodes the pictures and [`mp2`]
//! the soundtrack. The decoders follow pl_mpeg (Dominic Szablewski, MIT,
//! <https://github.com/phoboslab/pl_mpeg>); each module notes where it
//! departs from it.
//!
//! A [`Film`] is the playback source behind [`crate::movie::MovieSession`]:
//! it indexes the pictures once, decodes forward in display order on the
//! caller's thread (a 640x480 picture costs a few milliseconds) and converts
//! only the picture that is presented. The soundtrack is decoded on demand
//! and resampled to the mixer rate as one continuous stream, lined up with
//! the picture by the two streams' first time stamps.

pub mod bits;
pub mod mp2;
pub mod ps;
mod tables;
pub mod video;

use anyhow::{Context, Result, bail};

use crate::audio::SAMPLE_RATE;
use crate::budget;
use crate::media::{self, Colour, Matrix, Planes, StereoResampler, VideoFrame};
use bits::{Bits, find_start_code};
use video::{Decoder, SequenceHeader};

/// The software canvas the picture is box-filtered onto.
const CANVAS_WIDTH: usize = 320;
const CANVAS_HEIGHT: usize = 240;

/// MPEG-1 video is BT.601 with studio-range luma.
const COLOUR: Colour = Colour {
    matrix: Matrix::Bt601,
    full_range: false,
};

/// The program stream clock.
const PTS_RATE: i64 = 90_000;

/// What [`VideoStream::next`] produced.
enum Next<'a> {
    /// The next picture in display order.
    Picture(&'a video::Frame),
    /// A picture that failed to decode: the previous one stays on screen.
    Repeat,
    /// The stream has no more pictures.
    End,
}

/// The video elementary stream, its picture index and the decoder.
struct VideoStream {
    data: Vec<u8>,
    /// The byte offset of every picture start code, in decode order.
    pictures: Vec<usize>,
    decoder: Decoder,
    /// The next picture to decode.
    next: usize,
    flushed: bool,
    warned: bool,
}

impl VideoStream {
    fn new(data: Vec<u8>) -> Result<(Self, SequenceHeader)> {
        let mut pictures = Vec::new();
        let mut header = None;
        let mut at = 0;
        while let Some(start) = find_start_code(&data, at) {
            match data[start + 3] {
                video::PICTURE => {
                    if pictures.len() >= budget::MAX_AVI_FRAMES {
                        bail!(
                            "MPEG video has more than {} pictures",
                            budget::MAX_AVI_FRAMES
                        );
                    }
                    pictures.push(start);
                }
                video::SEQUENCE_HEADER if header.is_none() => {
                    let mut bits = Bits::new(&data[start + 4..]);
                    header = Some(SequenceHeader::read(&mut bits)?);
                }
                _ => {}
            }
            at = start + 4;
        }
        let header = header.context("MPEG video has no sequence header")?;
        if header.width * header.height > budget::MAX_PIXELS {
            bail!(
                "MPEG picture {}x{} is too large",
                header.width,
                header.height
            );
        }
        if pictures.is_empty() {
            bail!("MPEG video has no pictures");
        }
        let decoder = Decoder::new(&header);
        let stream = VideoStream {
            data,
            pictures,
            decoder,
            next: 0,
            flushed: false,
            warned: false,
        };
        Ok((stream, header))
    }

    /// Decode forward to the next picture in display order.
    fn next(&mut self) -> Next<'_> {
        while self.next < self.pictures.len() {
            let index = self.next;
            self.next += 1;
            let start = self.pictures[index];
            let end = self
                .pictures
                .get(index + 1)
                .copied()
                .unwrap_or(self.data.len());
            match self.decoder.decode(&self.data[start..end]) {
                Ok(true) => {
                    return match self.decoder.last_shown() {
                        Some(frame) => Next::Picture(frame),
                        None => Next::Repeat,
                    };
                }
                Ok(false) => {}
                Err(err) => {
                    if !std::mem::replace(&mut self.warned, true) {
                        eprintln!("warning: film picture {index} failed to decode: {err:#}");
                    }
                    return Next::Repeat;
                }
            }
        }
        if !std::mem::replace(&mut self.flushed, true)
            && let Some(frame) = self.decoder.flush()
        {
            return Next::Picture(frame);
        }
        Next::End
    }
}

/// The soundtrack decoded on demand into a continuous mixer-rate stream.
struct AudioStream {
    data: Vec<u8>,
    /// Every frame's `(offset, header)`.
    frames: Vec<(usize, mp2::Header)>,
    next: usize,
    decoder: mp2::Decoder,
    resampler: Option<StereoResampler>,
    /// Source frames to insert (positive) or drop (negative) at the start,
    /// lining the soundtrack up with the first picture.
    lead: i64,
    /// Interleaved output from frame `buffer_start` on.
    buffer: Vec<i16>,
    buffer_start: u64,
    scratch: Vec<f32>,
    warned: bool,
}

impl AudioStream {
    fn new(data: Vec<u8>, offset_ticks: i64) -> Option<Self> {
        let frames: Vec<_> = mp2::frames(&data).collect();
        let rate = i64::from(frames.first()?.1.sample_rate);
        Some(AudioStream {
            data,
            frames,
            next: 0,
            decoder: mp2::Decoder::new(),
            resampler: None,
            lead: offset_ticks * rate / PTS_RATE,
            buffer: Vec::new(),
            buffer_start: 0,
            scratch: Vec::new(),
            warned: false,
        })
    }

    /// The source frames the track decodes to, before the line-up.
    fn source_frames(&self) -> u64 {
        (self.frames.len() * mp2::SAMPLES_PER_FRAME) as u64
    }

    /// Decode the next frame onto the buffer; `false` at the track's end.
    fn decode_next(&mut self) -> bool {
        let Some(&(offset, header)) = self.frames.get(self.next) else {
            return false;
        };
        self.next += 1;
        self.scratch.clear();
        if let Err(err) = self.decoder.decode(&self.data[offset..], &mut self.scratch) {
            if !std::mem::replace(&mut self.warned, true) {
                eprintln!("warning: film audio frame failed to decode: {err:#}");
            }
            // Silence keeps the rest of the track in sync.
            self.scratch.clear();
            self.scratch.resize(mp2::SAMPLES_PER_FRAME * 2, 0.0);
        }
        let resampler = self
            .resampler
            .get_or_insert_with(|| StereoResampler::new(header.sample_rate, SAMPLE_RATE));
        let mut skip = 0;
        if self.lead > 0 {
            let silence = vec![0.0; self.lead as usize * 2];
            resampler.push(&silence, &mut self.buffer);
        } else if self.lead < 0 {
            skip = (self.lead.unsigned_abs() as usize).min(self.scratch.len() / 2);
            self.lead += skip as i64;
        }
        if self.lead > 0 {
            self.lead = 0;
        }
        resampler.push(&self.scratch[skip * 2..], &mut self.buffer);
        true
    }

    fn buffered_end(&self) -> u64 {
        self.buffer_start + (self.buffer.len() / 2) as u64
    }

    /// Append output frames `start..end` to `out`, padding with silence
    /// past the track's end. Calls must not go backwards.
    fn read(&mut self, start: u64, end: u64, out: &mut Vec<i16>) {
        let end = end.max(start);
        while self.buffered_end() < end && self.decode_next() {}
        let buffered_end = self.buffered_end();
        let skip = start.clamp(self.buffer_start, buffered_end) - self.buffer_start;
        self.buffer.drain(..(skip * 2) as usize);
        self.buffer_start += skip;
        let gap = self.buffer_start.saturating_sub(start).min(end - start);
        out.resize(out.len() + (gap * 2) as usize, 0);
        let take = (end - start - gap).min((self.buffer.len() / 2) as u64);
        out.extend(self.buffer.drain(..(take * 2) as usize));
        self.buffer_start += take;
        let missing = end - start - gap - take;
        out.resize(out.len() + (missing * 2) as usize, 0);
    }
}

/// One MPEG-1 film in playback.
pub struct Film {
    video: VideoStream,
    audio: Option<AudioStream>,
    width: usize,
    height: usize,
    frame_rate: (u32, u32),
    /// The picture on screen and its display index.
    frame: VideoFrame,
    shown: Option<usize>,
    /// The display index the next decoded picture takes.
    next_display: usize,
    ended: bool,
}

impl Film {
    /// Demux `data` and decode its first picture.
    pub fn open(data: &[u8]) -> Result<Self> {
        let program = ps::demux(data)?;
        let offset = match (program.video_pts, program.audio_pts) {
            (Some(video), Some(audio)) => audio as i64 - video as i64,
            _ => 0,
        };
        let (video, header) = VideoStream::new(program.video)?;
        let audio = AudioStream::new(program.audio, offset);
        let mut film = Film {
            video,
            audio,
            width: header.width,
            height: header.height,
            frame_rate: header.frame_rate,
            frame: VideoFrame::default(),
            shown: None,
            next_display: 0,
            ended: false,
        };
        film.present(0)?;
        if film.shown.is_none() {
            bail!("film decoded no pictures");
        }
        Ok(film)
    }

    /// The frame rate as `(num, den)`.
    pub fn frame_rate(&self) -> (u32, u32) {
        self.frame_rate
    }

    /// The picture size in pixels.
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// The pictures the film shows: one per coded picture.
    pub fn frame_count(&self) -> usize {
        self.video.pictures.len()
    }

    /// The soundtrack's length in source frames at its own rate, and that
    /// rate; `None` for a silent film.
    pub fn audio_frames(&self) -> Option<(u64, u32)> {
        let audio = self.audio.as_ref()?;
        Some((audio.source_frames(), audio.frames.first()?.1.sample_rate))
    }

    /// Slices that failed to decode so far.
    pub fn damaged_slices(&self) -> usize {
        self.video.decoder.damaged_slices()
    }

    /// Make display frame `index` current, decoding forward as needed. A
    /// stream that ends early keeps its last picture on screen; going
    /// backwards is not supported and keeps the current picture.
    pub fn present(&mut self, index: usize) -> Result<()> {
        if self.shown.is_some_and(|shown| shown >= index) {
            return Ok(());
        }
        // A target past the end shows the last picture.
        let index = index.min(self.frame_count().saturating_sub(1));
        while !self.ended {
            let display = self.next_display;
            let due = display >= index;
            match self.video.next() {
                Next::Picture(frame) => {
                    self.next_display += 1;
                    if due {
                        convert(frame, self.width, self.height, &mut self.frame);
                        self.shown = Some(display);
                        return Ok(());
                    }
                }
                Next::Repeat => {
                    self.next_display += 1;
                    if due && self.shown.is_some() {
                        self.shown = Some(display);
                        return Ok(());
                    }
                }
                Next::End => self.ended = true,
            }
        }
        Ok(())
    }

    /// The picture on screen.
    pub fn frame(&self) -> &VideoFrame {
        &self.frame
    }

    /// The display index of the picture on screen.
    pub fn frame_index(&self) -> Option<usize> {
        self.shown
    }

    /// Append the soundtrack for output frames `start..end` (at
    /// [`SAMPLE_RATE`], stereo) to `out`, padding with silence past the
    /// track's end. Calls must not go backwards.
    pub fn audio(&mut self, start: u64, end: u64, out: &mut Vec<i16>) {
        match &mut self.audio {
            Some(stream) => stream.read(start, end, out),
            None => out.resize(out.len() + (end.saturating_sub(start) as usize) * 2, 0),
        }
    }
}

/// Convert a decoded picture to the frame's RGBA and canvas.
fn convert(picture: &video::Frame, width: usize, height: usize, out: &mut VideoFrame) {
    let planes = Planes {
        width,
        height,
        y: &picture.y,
        u: &picture.cb,
        v: &picture.cr,
        y_stride: picture.luma_stride,
        u_stride: picture.chroma_stride,
        v_stride: picture.chroma_stride,
    };
    media::yuv420_to_rgba(&planes, COLOUR, &mut out.rgba);
    media::downscale_letterboxed(
        &out.rgba,
        width,
        height,
        CANVAS_WIDTH,
        CANVAS_HEIGHT,
        &mut out.canvas,
    );
    out.width = width as u32;
    out.height = height as u32;
    out.serial = media::next_frame_serial();
}

#[cfg(test)]
mod tests;
