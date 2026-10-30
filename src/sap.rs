//! RE2 `.sap` sound containers.
//!
//! A `.sap` starts with two little-endian `u32` mask words; each set bit,
//! low word first, stands for one RIFF WAV that follows the 8-byte header
//! back to back (openre's `audio.cpp` documents the masks). Every voice and
//! BGM `.sap` in the retail data has mask `0x1`: exactly one WAV.
//!
//! A sound bank's bits are its slots: `bio2.exe`'s `SsLoadSap` creates the
//! buffer for slot `n` from the WAV of the `n`th bit (low word bits 0-31,
//! then high word bits 0-15 as slots 32-47), so a bank may leave slots
//! empty.

use anyhow::{Result, bail, ensure};

/// The header length before the first WAV.
pub const HEADER_LEN: usize = 8;

/// The WAVs a `.sap` holds, in order, as slices of `data`.
pub fn wavs(data: &[u8]) -> Result<Vec<&[u8]>> {
    Ok(slots(data)?.into_iter().map(|(_, wav)| wav).collect())
}

/// The WAVs a `.sap` holds with the slot each fills: the index of its mask
/// bit, the high word's bits counting from 32.
pub fn slots(data: &[u8]) -> Result<Vec<(u8, &[u8])>> {
    ensure!(data.len() >= HEADER_LEN, "SAP is shorter than its header");
    let mask = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let slots = (0..64u8).filter(|bit| mask >> bit & 1 != 0);
    let mut out = Vec::with_capacity(mask.count_ones() as usize);
    let mut at = HEADER_LEN;
    for (index, slot) in slots.enumerate() {
        let Some(header) = data.get(at..at + 8) else {
            bail!("SAP WAV {index} at byte {at} is truncated");
        };
        ensure!(
            &header[0..4] == b"RIFF",
            "SAP WAV {index} at byte {at} is not RIFF"
        );
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize + 8;
        let Some(wav) = data.get(at..at + len) else {
            bail!("SAP WAV {index} at byte {at} runs past the end");
        };
        out.push((slot, wav));
        at += len;
    }
    Ok(out)
}

/// The single WAV of a one-WAV `.sap` (every voice and BGM file).
pub fn single_wav(data: &[u8]) -> Result<&[u8]> {
    match wavs(data)?.as_slice() {
        [wav] => Ok(wav),
        wavs => bail!("SAP holds {} WAVs, expected one", wavs.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riff(body: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF".to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    fn sap(low: u32, high: u32, wavs: &[Vec<u8>]) -> Vec<u8> {
        let mut out = low.to_le_bytes().to_vec();
        out.extend(high.to_le_bytes());
        for wav in wavs {
            out.extend(wav);
        }
        out
    }

    #[test]
    fn one_set_bit_is_one_wav() {
        let wav = riff(b"WAVEdata");
        let data = sap(1, 0, std::slice::from_ref(&wav));
        assert_eq!(single_wav(&data).unwrap(), wav.as_slice());
    }

    #[test]
    fn every_set_bit_in_both_words_is_a_wav() {
        let first = riff(b"WAVEa");
        let second = riff(b"WAVEbb");
        let third = riff(b"WAVEccc");
        let data = sap(
            0b101,
            0x8000_0000,
            &[first.clone(), second.clone(), third.clone()],
        );
        assert_eq!(
            wavs(&data).unwrap(),
            vec![first.as_slice(), second.as_slice(), third.as_slice()]
        );
        assert!(single_wav(&data).is_err());
    }

    #[test]
    fn each_wav_fills_its_bits_slot() {
        let first = riff(b"WAVEa");
        let second = riff(b"WAVEbb");
        let data = sap(0b1000_0100, 0b10, &[first.clone(), second.clone()]);
        assert!(wavs(&data).is_err());
        let third = riff(b"WAVEccc");
        let data = sap(
            0b1000_0100,
            0b10,
            &[first.clone(), second.clone(), third.clone()],
        );
        assert_eq!(
            slots(&data).unwrap(),
            vec![
                (2, first.as_slice()),
                (7, second.as_slice()),
                (33, third.as_slice())
            ]
        );
        assert!(slots(&sap(0, 0, &[])).unwrap().is_empty());
    }

    #[test]
    fn malformed_containers_fail() {
        assert!(wavs(&[1, 0, 0]).is_err());
        assert!(wavs(&sap(1, 0, &[])).is_err());
        assert!(wavs(&sap(1, 0, &[b"JUNKxxxx".to_vec()])).is_err());
        let mut short = riff(b"WAVEdata");
        short.truncate(10);
        assert!(wavs(&sap(1, 0, &[short])).is_err());
    }
}
