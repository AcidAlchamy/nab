//! The clipboard. A nab goes on as a classic bitmap (every program can paste
//! that) and as PNG (browsers, Discord, Slack and image editors prefer it; it
//! keeps the transparent corners). The PNG is made only when a program asks
//! for it, so copying never waits for an encoder.

use super::Shot;
use crate::win::wide;
use std::sync::OnceLock;
use std::time::Duration;
use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardOwner, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_DIB;

/// The registered "PNG" clipboard format.
pub fn png_format() -> u32 {
    static FMT: OnceLock<u32> = OnceLock::new();
    *FMT.get_or_init(|| unsafe { RegisterClipboardFormatW(wide("PNG").as_ptr()) })
}

/// Another program may have the clipboard open for a moment; try a few times.
fn open(owner: HWND) -> bool {
    for _ in 0..12 {
        if unsafe { OpenClipboard(owner) } != 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    false
}

fn global(bytes: &[u8]) -> Option<HANDLE> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if h.is_null() {
            return None;
        }
        let p = GlobalLock(h);
        if p.is_null() {
            GlobalFree(h);
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast(), bytes.len());
        GlobalUnlock(h);
        Some(h)
    }
}

/// A 24-bit bottom-up DIB (header + pixels), the most widely understood shape.
pub fn dib(shot: &Shot) -> Vec<u8> {
    let (w, h) = (shot.w as usize, shot.h as usize);
    let stride = (w * 3 + 3) & !3;
    let mut out = Vec::with_capacity(40 + stride * h);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((stride * h) as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes()); // 72 dpi
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    let flat = shot.flattened(0xFFFFFF);
    for y in (0..h).rev() {
        let row_start = out.len();
        for &p in &flat[y * w..(y + 1) * w] {
            out.extend_from_slice(&[p as u8, (p >> 8) as u8, (p >> 16) as u8]);
        }
        out.resize(row_start + stride, 0);
    }
    out
}

/// Puts a shot on the clipboard: the bitmap now, the PNG on request (the
/// owner window then gets WM_RENDERFORMAT and calls `render_png`).
pub fn put(owner: HWND, shot: &Shot) -> bool {
    if !open(owner) {
        return false;
    }
    let ok = unsafe {
        EmptyClipboard();
        let bitmap = global(&dib(shot)).map(|h| SetClipboardData(CF_DIB as u32, h)).is_some_and(|r| !r.is_null());
        SetClipboardData(png_format(), std::ptr::null_mut());
        bitmap
    };
    unsafe { CloseClipboard() };
    ok
}

/// Answers WM_RENDERFORMAT for the PNG (the clipboard is already open then).
pub fn render_png(bytes: &[u8]) {
    if let Some(h) = global(bytes) {
        unsafe { SetClipboardData(png_format(), h) };
    }
}

/// Answers WM_RENDERALLFORMATS (Nab is quitting but its nab is still on the clipboard).
pub fn render_all(owner: HWND, bytes: &[u8]) {
    if open(owner) {
        if unsafe { GetClipboardOwner() } == owner {
            render_png(bytes);
        }
        unsafe { CloseClipboard() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_rows_are_bottom_up_and_padded() {
        let shot =
            Shot { w: 3, h: 2, px: vec![0xFF010203, 0xFF040506, 0xFF070809, 0xFF0A0B0C, 0xFF0D0E0F, 0xFF101112] };
        let d = dib(&shot);
        assert_eq!(d.len(), 40 + 12 * 2);
        // First stored row is the bottom one, BGR order, padded to 12 bytes.
        assert_eq!(&d[40..49], &[0x0C, 0x0B, 0x0A, 0x0F, 0x0E, 0x0D, 0x12, 0x11, 0x10]);
        assert_eq!(&d[49..52], &[0, 0, 0]);
        assert_eq!(&d[52..55], &[0x03, 0x02, 0x01]);
    }
}
