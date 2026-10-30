//! The MPEG program stream demuxer (ISO/IEC 11172-1, with the MPEG-2 pack
//! and PES header forms of 13818-1 accepted too).
//!
//! A film is demuxed whole at open: the first video stream (`0xE0`-`0xEF`)
//! and the first audio stream (`0xC0`-`0xDF`) are each concatenated into one
//! elementary stream, and the first presentation time stamp of each is kept
//! so the two tracks can be lined up. Padding, private and system streams are
//! skipped.

use anyhow::{Result, bail};

use super::bits::find_start_code;

/// The pack header start code.
const PACK: u8 = 0xBA;
/// The program end code.
const END: u8 = 0xB9;
/// The system header start code.
const SYSTEM: u8 = 0xBB;
/// The program stream map: an MPEG-2 PSM, skipped like the padding stream.
const STREAM_MAP: u8 = 0xBC;

/// The demuxed elementary streams of one program stream.
#[derive(Debug, Default)]
pub struct Program {
    /// The first video stream's bytes.
    pub video: Vec<u8>,
    /// The first audio stream's bytes.
    pub audio: Vec<u8>,
    /// The first video PTS in 90 kHz ticks.
    pub video_pts: Option<u64>,
    /// The first audio PTS in 90 kHz ticks.
    pub audio_pts: Option<u64>,
}

/// Demux `data`, a program stream from its first pack header.
pub fn demux(data: &[u8]) -> Result<Program> {
    if !crate::format::is_mpeg_program(data) {
        bail!("not an MPEG program stream");
    }
    let mut program = Program::default();
    let mut video_id = None;
    let mut audio_id = None;
    let mut at = 0;
    while let Some(start) = find_start_code(data, at) {
        let code = data[start + 3];
        let body = start + 4;
        at = match code {
            PACK => body + pack_header_len(&data[body..])?,
            END => break,
            SYSTEM | STREAM_MAP | 0xBD..=0xFF => {
                let length = read_u16(data, body)? as usize;
                let payload = body + 2;
                let end = payload + length;
                if end > data.len() {
                    // A truncated last packet keeps what it has.
                    if is_elementary(code) {
                        let packet = &data[payload..];
                        take_packet(&mut program, &mut video_id, &mut audio_id, code, packet)?;
                    }
                    break;
                }
                if is_elementary(code) {
                    let packet = &data[payload..end];
                    take_packet(&mut program, &mut video_id, &mut audio_id, code, packet)?;
                }
                end
            }
            // A stray start code between packets: resynchronise past it.
            _ => body,
        };
    }
    if program.video.is_empty() {
        bail!("program stream has no video");
    }
    Ok(program)
}

/// Whether stream id `code` is an audio or video elementary stream.
fn is_elementary(code: u8) -> bool {
    matches!(code, 0xC0..=0xEF)
}

/// The length of a pack header after its start code: 8 bytes for MPEG-1,
/// 10 plus stuffing for MPEG-2.
fn pack_header_len(body: &[u8]) -> Result<usize> {
    let Some(&first) = body.first() else {
        bail!("truncated pack header");
    };
    if first >> 6 == 0b01 {
        let Some(&stuffing) = body.get(9) else {
            bail!("truncated MPEG-2 pack header");
        };
        Ok(10 + usize::from(stuffing & 7))
    } else {
        Ok(8)
    }
}

fn read_u16(data: &[u8], at: usize) -> Result<u16> {
    match data.get(at..at + 2) {
        Some(bytes) => Ok(u16::from_be_bytes([bytes[0], bytes[1]])),
        None => bail!("truncated packet length at byte {at}"),
    }
}

/// Strip a PES header and append the payload to its stream.
fn take_packet(
    program: &mut Program,
    video_id: &mut Option<u8>,
    audio_id: &mut Option<u8>,
    code: u8,
    packet: &[u8],
) -> Result<()> {
    let (selected, out, first_pts) = if code >= 0xE0 {
        let id = *video_id.get_or_insert(code);
        (id == code, &mut program.video, &mut program.video_pts)
    } else {
        let id = *audio_id.get_or_insert(code);
        (id == code, &mut program.audio, &mut program.audio_pts)
    };
    if !selected {
        return Ok(());
    }
    let (payload, pts) = pes_payload(packet)?;
    if first_pts.is_none() {
        *first_pts = pts;
    }
    out.extend_from_slice(payload);
    Ok(())
}

/// The payload of a PES packet body and its PTS, in either header form.
fn pes_payload(packet: &[u8]) -> Result<(&[u8], Option<u64>)> {
    if packet.first().is_some_and(|byte| byte >> 6 == 0b10) {
        // MPEG-2: two flag bytes and the header data length.
        let (Some(&flags), Some(&length)) = (packet.get(1), packet.get(2)) else {
            bail!("truncated MPEG-2 PES header");
        };
        let start = 3 + usize::from(length);
        if start > packet.len() {
            bail!("MPEG-2 PES header runs past its packet");
        }
        let pts = (flags & 0x80 != 0)
            .then(|| packet.get(3..8).map(timestamp))
            .flatten();
        return Ok((&packet[start..], pts));
    }
    // MPEG-1: stuffing, an optional STD buffer field, then the time stamps.
    let mut at = 0;
    while packet.get(at) == Some(&0xFF) {
        at += 1;
    }
    if packet.get(at).is_some_and(|byte| byte >> 6 == 0b01) {
        at += 2;
    }
    let Some(&marker) = packet.get(at) else {
        bail!("truncated MPEG-1 PES header");
    };
    let (skip, pts) = match marker >> 4 {
        0b0010 => (5, packet.get(at..at + 5).map(timestamp)),
        0b0011 => (10, packet.get(at..at + 5).map(timestamp)),
        _ => (1, None),
    };
    at += skip;
    if at > packet.len() {
        bail!("MPEG-1 PES header runs past its packet");
    }
    Ok((&packet[at..], pts))
}

/// A 33-bit time stamp from its five marker-separated bytes.
fn timestamp(bytes: &[u8]) -> u64 {
    (u64::from(bytes[0] >> 1 & 0x07) << 30)
        | (u64::from(bytes[1]) << 22)
        | (u64::from(bytes[2] >> 1) << 15)
        | (u64::from(bytes[3]) << 7)
        | u64::from(bytes[4] >> 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An MPEG-1 time stamp field with prefix `prefix` (`0b0010` PTS only).
    fn stamp(prefix: u8, value: u64) -> [u8; 5] {
        [
            (prefix << 4) | ((value >> 29) as u8 & 0x0E) | 1,
            (value >> 22) as u8,
            ((value >> 14) as u8 & 0xFE) | 1,
            (value >> 7) as u8,
            ((value << 1) as u8) | 1,
        ]
    }

    fn packet(id: u8, header: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 1, id];
        out.extend_from_slice(&((header.len() + payload.len()) as u16).to_be_bytes());
        out.extend_from_slice(header);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn demuxes_mpeg1_packets_into_their_streams() {
        let mut data = vec![0, 0, 1, 0xBA, 0x21, 0, 1, 0, 1, 0x80, 0, 1];
        data.extend(packet(0xBB, &[0; 6], &[]));
        let mut header = vec![0xFF, 0xFF, 0x40, 0x20];
        header.extend_from_slice(&stamp(0b0010, 36_280));
        data.extend(packet(0xE0, &header, &[1, 2, 3]));
        data.extend(packet(0xBE, &[], &[0xFF; 9]));
        data.extend(packet(0xC0, &stamp(0b0010, 36_281), &[7, 8]));
        // A second audio stream is ignored.
        data.extend(packet(0xC1, &[0x0F], &[9]));
        data.extend(packet(0xE0, &[0x0F], &[4]));
        data.extend([0, 0, 1, 0xB9]);
        let program = demux(&data).unwrap();
        assert_eq!(program.video, [1, 2, 3, 4]);
        assert_eq!(program.audio, [7, 8]);
        assert_eq!(program.video_pts, Some(36_280));
        assert_eq!(program.audio_pts, Some(36_281));
    }

    #[test]
    fn reads_mpeg2_pack_and_pes_headers() {
        let mut data = vec![0, 0, 1, 0xBA, 0x44, 0, 4, 0, 4, 1, 1, 0x89, 0xC3, 0xF9];
        data.extend([0xFF; 1]);
        let mut header = vec![0x81, 0x80, 5];
        header.extend_from_slice(&stamp(0b0010, 90_000));
        data.extend(packet(0xE0, &header, &[5, 6]));
        let program = demux(&data).unwrap();
        assert_eq!(program.video, [5, 6]);
        assert_eq!(program.video_pts, Some(90_000));
        assert!(program.audio.is_empty());
    }

    #[test]
    fn rejects_streams_without_video() {
        assert!(demux(b"RIFF").is_err());
        let mut data = vec![0, 0, 1, 0xBA, 0x21, 0, 1, 0, 1, 0x80, 0, 1];
        data.extend(packet(0xC0, &[0x0F], &[1]));
        assert!(demux(&data).is_err());
    }
}
