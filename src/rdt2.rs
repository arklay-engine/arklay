//! RE2 RDT room reader.
//!
//! Reads an RE2 RDT into the engine's [`RoomSource`], so the rest of the
//! engine builds and draws the room without caring which game it came from.
//! This covers what a room needs to be shown: the camera cuts (RID), the
//! camera switch zones (RVD) and the collision records (SCA). The scripts,
//! lights, masks (PRI), models and everything else stay unread for now.
//!
//! The header is eight `u8` counts (byte 1 is the camera count) followed by
//! 23 `u32` section offsets; the section order follows biohazard-utils'
//! `Rdt2` and openre's `RdtOffsetKind`.

use anyhow::{Context, Result, bail};

use crate::budget;
use crate::room::source::{Camera, RoomSource};
use crate::state::{Sca, ScaRect, Zone};

/// Offset of the section offset table.
const OFFSETS: usize = 0x08;
/// Number of section offsets.
const OFFSET_COUNT: usize = 23;
/// Section slot of the collision records (SCA).
const SCA_SLOT: usize = 6;
/// Section slot of the camera records (RID).
const RID_SLOT: usize = 7;
/// Section slot of the camera switch zones (RVD).
const RVD_SLOT: usize = 8;
/// Size of one camera record: flags, perspective, position, target and the
/// mask table offset.
const CAMERA_SIZE: usize = 0x20;
/// Size of one camera switch zone record (`VCut`).
const ZONE_SIZE: usize = 0x14;
/// Size of the collision header and of one collision record.
const SCA_RECORD_SIZE: usize = 0x10;
/// The camera's perspective word is the focal length in 1/128 units: the
/// executable sets the projection to `perspective >> 7` on every cut change
/// (`set_geom_screen`, called from 0x4C4989 among others).
const PERSPECTIVE_SHIFT: u32 = 7;

fn bytes(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .with_context(|| {
            format!(
                "RDT is truncated: need {len} bytes at 0x{offset:x}, the file is {} bytes",
                data.len()
            )
        })
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(data, offset, 2)?.try_into()?))
}

fn i16_at(data: &[u8], offset: usize) -> Result<i16> {
    u16_at(data, offset).map(|value| value as i16)
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(data, offset, 4)?.try_into()?))
}

fn i32_at(data: &[u8], offset: usize) -> Result<i32> {
    u32_at(data, offset).map(|value| value as i32)
}

/// The section offset table; a null offset is an absent section.
fn offsets(data: &[u8]) -> Result<[u32; OFFSET_COUNT]> {
    let mut offsets = [0; OFFSET_COUNT];
    for (slot, offset) in offsets.iter_mut().enumerate() {
        *offset = u32_at(data, OFFSETS + slot * 4)?;
        if *offset as usize > data.len() {
            bail!(
                "section {slot} offset 0x{offset:x} is past the end of the {}-byte RDT",
                data.len()
            );
        }
    }
    Ok(offsets)
}

/// Read an RE2 RDT into a room definition.
pub fn read(data: &[u8]) -> Result<RoomSource> {
    let Some(&camera_count) = data.get(1) else {
        bail!("RDT is too short for a camera count");
    };
    let offsets = offsets(data)?;
    let cameras = parse_cameras(data, offsets[RID_SLOT], camera_count)?;
    let zones = parse_zones(data, offsets[RVD_SLOT])?;
    let sca = parse_sca(data, offsets[SCA_SLOT])?;
    Ok(RoomSource {
        cameras,
        zones,
        sca,
        ..RoomSource::default()
    })
}

/// The camera records: `u16` flags, the `u16` perspective, the position and
/// the target. The record's mask table offset is not read yet.
fn parse_cameras(data: &[u8], offset: u32, count: u8) -> Result<Vec<Camera>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if offset == 0 {
        bail!("RDT declares {count} camera(s) but has no camera section");
    }
    (0..usize::from(count))
        .map(|index| {
            let at = offset as usize + index * CAMERA_SIZE;
            bytes(data, at, CAMERA_SIZE).with_context(|| format!("camera {index}"))?;
            let perspective = u16_at(data, at + 2)?;
            let field = |n: usize| i32_at(data, at + 4 + n * 4);
            Ok(Camera {
                pos: [field(0)?, field(1)?, field(2)?],
                look_at: [field(3)?, field(4)?, field(5)?],
                roll: 0,
                fov: i32::from(perspective >> PERSPECTIVE_SHIFT),
                mask_groups: Vec::new(),
            })
        })
        .collect()
}

/// The camera switch zones: `u8` enabled flag, `u8` floor (`0xFF` any),
/// `u8` from-camera, `u8` to-camera and four `(x, z)` corners, up to the
/// all-`0xFF` terminator. Like RE1's, each from-camera's first record is its
/// group's header.
fn parse_zones(data: &[u8], offset: u32) -> Result<Vec<Zone>> {
    if offset == 0 {
        return Ok(Vec::new());
    }
    let mut zones = Vec::new();
    let mut at = offset as usize;
    while let Ok(record) = bytes(data, at, ZONE_SIZE) {
        if record[..4] == [0xFF; 4] {
            break;
        }
        budget::check_len(
            zones.len() + 1,
            budget::MAX_RECORDS,
            "camera switch zone count",
        )?;
        let mut corners = [[0i16; 2]; 4];
        for (index, corner) in corners.iter_mut().enumerate() {
            *corner = [
                i16_at(data, at + 4 + index * 4)?,
                i16_at(data, at + 6 + index * 4)?,
            ];
        }
        zones.push(Zone {
            cam_from: i16::from(record[2]),
            cam_to: i16::from(record[3]),
            corners,
        });
        at += ZONE_SIZE;
    }
    Ok(zones)
}

/// The collision section: a 16-byte header (`i16` centre, `u32` count that
/// includes the header, `i32` ceiling, a tail word) then the records.
fn parse_sca(data: &[u8], offset: u32) -> Result<Option<Sca>> {
    if offset == 0 {
        return Ok(None);
    }
    let base = offset as usize;
    let count = u32_at(data, base + 4).context("collision header")? as usize;
    let records = count.saturating_sub(1);
    budget::check_len(records, budget::MAX_RECORDS, "collision record count")?;
    let start = base + SCA_RECORD_SIZE;
    bytes(data, start, records * SCA_RECORD_SIZE)
        .with_context(|| format!("collision table at 0x{base:x} with {records} record(s)"))?;
    let records = (0..records)
        .map(|index| {
            let at = start + index * SCA_RECORD_SIZE;
            Ok(ScaRect {
                x: i16_at(data, at)?,
                z: i16_at(data, at + 2)?,
                width: u16_at(data, at + 4)?,
                depth: u16_at(data, at + 6)?,
                id: u16_at(data, at + 8)?,
                kind: u16_at(data, at + 10)?,
                floor: u32_at(data, at + 12)?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(Some(Sca {
        cell_x: i16_at(data, base)?,
        cell_z: i16_at(data, base + 2)?,
        ceiling: i32_at(data, base + 8)?,
        tail: u32_at(data, base + 12)?,
        records,
    }))
}

#[cfg(test)]
mod tests;
