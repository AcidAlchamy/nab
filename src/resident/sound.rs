//! The optional shutter: two soft clicks, synthesised once into an in-memory
//! WAV and played with PlaySound. No audio files ship with Nab.

use std::sync::OnceLock;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

const RATE: u32 = 44_100;

fn shutter() -> Vec<u8> {
    // Two short bursts of filtered noise (the blade opening, then closing),
    // each with a quick exponential tail, plus a faint low thump.
    let len = (RATE as f32 * 0.16) as usize;
    let mut samples = vec![0f32; len];
    let mut seed = 0x2545_F491u32;
    let mut noise = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    for (at, level, decay) in [(0.0f32, 0.55f32, 140.0f32), (0.055, 0.42, 110.0)] {
        let start = (at * RATE as f32) as usize;
        let mut low = 0.0f32;
        for (i, s) in samples.iter_mut().enumerate().skip(start) {
            let t = (i - start) as f32 / RATE as f32;
            let env = (t / 0.0008).min(1.0) * (-t * decay).exp();
            low += (noise() - low) * 0.35; // a gentle low-pass: softer, less hiss
            let thump = (std::f32::consts::TAU * 180.0 * t).sin() * (-t * 60.0).exp() * 0.25;
            *s += (low + thump) * env * level;
        }
    }

    let data: Vec<u8> =
        samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32 * 0.8) as i16).to_le_bytes()).collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    wav
}

pub fn play() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    let wav = WAV.get_or_init(shutter);
    unsafe { PlaySoundW(wav.as_ptr().cast(), std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
}
