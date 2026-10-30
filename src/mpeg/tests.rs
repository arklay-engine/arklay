//! The decoders against an independent decode of a small fixture.
//!
//! `testdata/tiny.mpg` is a 48x32, 9-picture MPEG-1 program stream
//! (I/P/B pictures, two B pictures per reference, 29.97 fps) with four
//! frames of 192 kbit/s 44.1 kHz stereo Layer II audio, encoded by ffmpeg
//! from its `testsrc` and `sine` sources. `tiny.yuv` and `tiny.pcm` are
//! ffmpeg's own decode of it (`-f rawvideo -pix_fmt yuv420p` and
//! `-f s16le`). The IDCT and the synthesis filter may differ from ffmpeg's
//! in rounding, so the comparison is by signal-to-noise ratio.

use super::*;

const TINY: &[u8] = include_bytes!("testdata/tiny.mpg");
const TINY_YUV: &[u8] = include_bytes!("testdata/tiny.yuv");
const TINY_PCM: &[u8] = include_bytes!("testdata/tiny.pcm");

/// Peak signal-to-noise ratio of `got` against `expected`, in dB.
fn psnr(got: &[u8], expected: &[u8]) -> f64 {
    assert_eq!(got.len(), expected.len());
    let error: f64 = got
        .iter()
        .zip(expected)
        .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum::<f64>()
        / got.len() as f64;
    if error == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / error).log10()
}

/// The cropped planes of a decoded picture, in `yuv420p` order.
fn planes(frame: &video::Frame, width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for row in frame.y.chunks(frame.luma_stride).take(height) {
        out.extend_from_slice(&row[..width]);
    }
    for plane in [&frame.cb, &frame.cr] {
        for row in plane.chunks(frame.chroma_stride).take(height / 2) {
            out.extend_from_slice(&row[..width / 2]);
        }
    }
    out
}

#[test]
fn video_matches_an_independent_decode() {
    let program = ps::demux(TINY).unwrap();
    let (mut stream, header) = VideoStream::new(program.video).unwrap();
    assert_eq!((header.width, header.height), (48, 32));
    assert_eq!(header.frame_rate, (30_000, 1001));
    assert_eq!(stream.pictures.len(), 9);
    let size = 48 * 32 * 3 / 2;
    let mut shown = 0;
    loop {
        match stream.next() {
            Next::Picture(frame) => {
                let expected = &TINY_YUV[shown * size..(shown + 1) * size];
                let quality = psnr(&planes(frame, 48, 32), expected);
                assert!(quality > 40.0, "picture {shown}: {quality:.1} dB");
                shown += 1;
            }
            Next::Repeat => panic!("picture {shown} failed to decode"),
            Next::End => break,
        }
    }
    assert_eq!(shown, 9);
    assert_eq!(stream.decoder.damaged_slices(), 0);
}

#[test]
fn audio_matches_an_independent_decode() {
    let program = ps::demux(TINY).unwrap();
    let mut decoder = mp2::Decoder::new();
    let mut samples = Vec::new();
    let mut count = 0;
    for (offset, header) in mp2::frames(&program.audio) {
        assert_eq!(header.sample_rate, 44_100);
        decoder
            .decode(&program.audio[offset..], &mut samples)
            .unwrap();
        count += 1;
    }
    assert_eq!(count, 4);
    let expected: Vec<f64> = TINY_PCM
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f64::from(i16::from_le_bytes(*pair)) / 32768.0)
        .collect();
    assert_eq!(samples.len(), expected.len());
    let signal: f64 = expected.iter().map(|value| value * value).sum();
    let noise: f64 = samples
        .iter()
        .zip(&expected)
        .map(|(&got, want)| (f64::from(got) - want).powi(2))
        .sum();
    let snr = 10.0 * (signal / noise).log10();
    assert!(snr > 40.0, "{snr:.1} dB");
}

#[test]
fn film_presents_in_order_and_lines_up_the_audio() {
    let mut film = Film::open(TINY).unwrap();
    assert_eq!(film.frame_count(), 9);
    assert_eq!(film.size(), (48, 32));
    assert_eq!(film.frame_index(), Some(0));
    assert_eq!(film.frame().rgba.len(), 48 * 32 * 4);
    assert_eq!(film.frame().canvas.len(), 320 * 240 * 4);
    // Skipping ahead decodes the pictures in between without showing them.
    let serial = film.frame().serial;
    film.present(5).unwrap();
    assert_eq!(film.frame_index(), Some(5));
    assert_ne!(film.frame().serial, serial);
    // Past the end keeps the last picture.
    film.present(20).unwrap();
    assert_eq!(film.frame_index(), Some(8));
    assert_eq!(film.audio_frames(), Some((4 * 1152, 44_100)));

    // The audio starts 0.0109 s (982 ticks) before the video, so the
    // first 481 source frames are dropped and the stream halves to the
    // mixer rate.
    let program = ps::demux(TINY).unwrap();
    let offset = program.audio_pts.unwrap() as i64 - program.video_pts.unwrap() as i64;
    assert_eq!(offset, -982);
    let mut out = Vec::new();
    film.audio(0, 10_000, &mut out);
    assert_eq!(out.len(), 20_000);
    let kept = (4 * 1152 - 481) / 2;
    assert!(out[..kept * 2].iter().any(|&sample| sample != 0));
    assert!(out[(kept + 2) * 2..].iter().all(|&sample| sample == 0));
}

#[test]
fn damaged_streams_fail_cleanly() {
    assert!(Film::open(b"RIFF....").is_err());
    // Truncations at every point either open or fail; none panic.
    for len in (0..TINY.len()).step_by(97) {
        if let Ok(mut film) = Film::open(&TINY[..len]) {
            film.present(100).unwrap();
            let mut out = Vec::new();
            film.audio(0, 5000, &mut out);
        }
    }
    // Corrupt bytes inside the slices are survived.
    let mut corrupt = TINY.to_vec();
    for at in (600..corrupt.len()).step_by(41) {
        corrupt[at] ^= 0x5A;
    }
    if let Ok(mut film) = Film::open(&corrupt) {
        film.present(100).unwrap();
    }
}
