//! Grabbing pixels: what's on screen inside a rectangle, which window is
//! which, and the finishing touches (Windows 11's round corners, the pointer).
//!
//! Everything is in physical pixels: the resident is per-monitor DPI aware, so
//! a 150% screen gives a sharp 150% picture.

pub mod area;
pub mod clip;
pub mod name;
pub mod save;

use crate::paint::Canvas;
use crate::win::{self, intersect};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{BitBlt, CAPTUREBLT, DeleteObject, GetDC, ReleaseDC, SRCCOPY};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, DI_NORMAL, DrawIconEx, EnumWindows, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GetAncestor,
    GetCursorInfo, GetForegroundWindow, GetIconInfo, GetWindowLongW, GetWindowRect, ICONINFO, IsIconic,
    IsWindowVisible, IsZoomed, WS_CAPTION, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_THICKFRAME,
};

const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const DWMWA_CLOAKED: u32 = 14;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_DONOTROUND: i32 = 1;
const DWMWCP_ROUNDSMALL: i32 = 3;

/// A captured picture: 0xAARRGGBB pixels, straight (not premultiplied) alpha.
/// Alpha is 255 everywhere except corners made transparent.
#[derive(Clone)]
pub struct Shot {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Shot {
    /// Copies a canvas region (canvas coordinates) out as an opaque picture.
    pub fn from_canvas(c: &Canvas, r: &RECT) -> Option<Shot> {
        let r = intersect(r, &RECT { left: 0, top: 0, right: c.w, bottom: c.h })?;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let src = c.px_ref();
        let mut px = Vec::with_capacity((w * h) as usize);
        for y in r.top..r.bottom {
            let row = (y * c.w + r.left) as usize;
            px.extend(src[row..row + w as usize].iter().map(|p| p | 0xFF00_0000));
        }
        Some(Shot { w, h, px })
    }

    pub fn has_alpha(&self) -> bool {
        self.px.iter().any(|p| p >> 24 != 0xFF)
    }

    /// Makes the corners outside a rounded window transparent, anti-aliased.
    pub fn round_corners(&mut self, radius: f32) {
        let (w, h) = (self.w, self.h);
        let r = radius.min(w as f32 / 2.0).min(h as f32 / 2.0);
        let n = r.ceil() as i32 + 1;
        for y in 0..n.min(h) {
            for x in 0..n.min(w) {
                let k = crate::paint::cover(x, y, 0.0, 0.0, 2.0 * r + 2.0, 2.0 * r + 2.0, r);
                if k >= 1.0 {
                    continue;
                }
                for (cx, cy) in [(x, y), (w - 1 - x, y), (x, h - 1 - y), (w - 1 - x, h - 1 - y)] {
                    let p = &mut self.px[(cy * w + cx) as usize];
                    let a = ((*p >> 24) as f32 * k + 0.5) as u32;
                    *p = (*p & 0xFF_FFFF) | (a << 24);
                }
            }
        }
    }

    /// The picture over a solid background (for JPEG and the old-style clipboard picture).
    pub fn flattened(&self, bg: u32) -> Vec<u32> {
        if !self.has_alpha() {
            return self.px.clone();
        }
        self.px
            .iter()
            .map(|&p| {
                let a = (p >> 24) as f32 / 255.0;
                0xFF00_0000 | crate::paint::mix(bg, p & 0xFF_FFFF, a)
            })
            .collect()
    }
}

/// Copies what's on screen inside `r` into a canvas the size of `r`.
pub fn grab_canvas(r: &RECT) -> Option<Canvas> {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    let c = Canvas::new(w, h)?;
    unsafe {
        let screen = GetDC(null_mut());
        let ok = BitBlt(c.dc, 0, 0, w, h, screen, r.left, r.top, SRCCOPY | CAPTUREBLT);
        ReleaseDC(null_mut(), screen);
        (ok != 0).then_some(c)
    }
}

/// What's on screen inside `r`, with the pointer drawn in if asked.
pub fn grab(r: &RECT, pointer: bool) -> Option<Shot> {
    let c = grab_canvas(r)?;
    if pointer {
        draw_pointer(&c, r.left, r.top);
    }
    Shot::from_canvas(&c, &RECT { left: 0, top: 0, right: c.w, bottom: c.h })
}

/// Draws the current mouse pointer onto a canvas whose top-left is at (ox, oy) on screen.
fn draw_pointer(c: &Canvas, ox: i32, oy: i32) {
    unsafe {
        let mut ci = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
        if GetCursorInfo(&mut ci) == 0 || ci.flags & CURSOR_SHOWING == 0 || ci.hCursor.is_null() {
            return;
        }
        let mut ii = ICONINFO::default();
        let (hx, hy) = if GetIconInfo(ci.hCursor, &mut ii) != 0 {
            if !ii.hbmMask.is_null() {
                DeleteObject(ii.hbmMask);
            }
            if !ii.hbmColor.is_null() {
                DeleteObject(ii.hbmColor);
            }
            (ii.xHotspot as i32, ii.yHotspot as i32)
        } else {
            (0, 0)
        };
        let (x, y) = (ci.ptScreenPos.x - ox - hx, ci.ptScreenPos.y - oy - hy);
        DrawIconEx(c.dc, x, y, ci.hCursor, 0, 0, 0, null_mut(), DI_NORMAL);
    }
}

/// A window worth nabbing.
#[derive(Clone)]
pub struct Win {
    pub hwnd: HWND,
    /// Its visible edges (no invisible resize border), clipped to the screens.
    pub rect: RECT,
    /// Corner radius to make transparent, in pixels, or 0.
    pub round: f32,
}

impl Win {
    pub fn app(&self) -> String {
        win::app_name(self.hwnd)
    }

    pub fn title(&self) -> String {
        win::window_title(self.hwnd)
    }
}

fn dwm_get<T: Default>(hwnd: HWND, attr: u32) -> Option<T> {
    let mut v = T::default();
    let hr = unsafe { DwmGetWindowAttribute(hwnd, attr as _, &mut v as *mut T as *mut _, size_of::<T>() as u32) };
    (hr >= 0).then_some(v)
}

fn is_desktop(hwnd: HWND) -> bool {
    matches!(win::class_name(hwnd).as_str(), "Progman" | "WorkerW")
}

fn is_taskbar(hwnd: HWND) -> bool {
    matches!(win::class_name(hwnd).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// Is this our own (or another tool's) see-through overlay rather than a real window?
fn is_overlay(hwnd: HWND) -> bool {
    let ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    ex & WS_EX_TRANSPARENT != 0 || (ex & WS_EX_TOOLWINDOW != 0 && style & WS_CAPTION == 0)
}

/// The window's visible rectangle and whether Windows 11 rounds its corners.
pub fn describe(hwnd: HWND) -> Option<Win> {
    unsafe {
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return None;
        }
    }
    if dwm_get::<u32>(hwnd, DWMWA_CLOAKED).unwrap_or(0) != 0 {
        return None;
    }
    let rect = dwm_get::<RECT>(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS)
        .filter(|r| r.right > r.left && r.bottom > r.top)
        .or_else(|| {
            let mut r = RECT::default();
            (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then_some(r)
        })?;
    let rect = intersect(&rect, &win::virtual_screen())?;
    Some(Win { hwnd, rect, round: corner_radius(hwnd, &rect) })
}

fn corner_radius(hwnd: HWND, rect: &RECT) -> f32 {
    if win::windows_build() < 22000 || unsafe { IsZoomed(hwnd) } != 0 {
        return 0.0;
    }
    // Full screen (games, video, F11) has square corners.
    if win::monitor_of_rect(rect).is_some_and(|m| win::same(&m.rect, rect)) {
        return 0.0;
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let pref = dwm_get::<i32>(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE).unwrap_or(0);
    let framed = style & (WS_CAPTION | WS_THICKFRAME) != 0;
    if pref == DWMWCP_DONOTROUND || (!framed && pref == 0) {
        return 0.0;
    }
    let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0;
    (if pref == DWMWCP_ROUNDSMALL { 4.0 } else { 8.0 }) * scale
}

/// The window you're in, or None when that's the desktop, the taskbar, or
/// one of Nab's own windows.
pub fn foreground() -> Option<Win> {
    let hwnd = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) };
    if hwnd.is_null() || is_desktop(hwnd) || is_taskbar(hwnd) || win::class_name(hwnd).starts_with("Nab.") {
        return None;
    }
    describe(hwnd)
}

/// Every window on screen, front to back (for the area picker, and for a
/// window nab started from the tray, where the tray itself is in front).
pub fn windows_on_screen() -> Vec<Win> {
    unsafe extern "system" fn each(hwnd: HWND, data: isize) -> i32 {
        let list = unsafe { &mut *(data as *mut Vec<Win>) };
        if !is_desktop(hwnd) && !is_overlay(hwnd) && !win::class_name(hwnd).starts_with("Nab.") {
            if let Some(w) = describe(hwnd) {
                list.push(w);
            }
        }
        1
    }
    let mut list: Vec<Win> = Vec::new();
    unsafe { EnumWindows(Some(each), &mut list as *mut _ as isize) };
    list
}

/// The front-most real app window (skips the taskbar).
pub fn front_app_window() -> Option<Win> {
    windows_on_screen().into_iter().find(|w| !is_taskbar(w.hwnd))
}

/// The window under a point, front-most first.
pub fn window_at(list: &[Win], p: POINT) -> Option<usize> {
    list.iter().position(|w| win::contains(&w.rect, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: i32, h: i32) -> Shot {
        Shot { w, h, px: vec![0xFF11_2233; (w * h) as usize] }
    }

    #[test]
    fn rounding_clears_only_the_corners() {
        let mut s = solid(40, 30);
        s.round_corners(8.0);
        let a = |x: i32, y: i32| s.px[(y * 40 + x) as usize] >> 24;
        assert_eq!(a(0, 0), 0);
        assert_eq!(a(39, 0), 0);
        assert_eq!(a(0, 29), 0);
        assert_eq!(a(39, 29), 0);
        assert_eq!(a(20, 0), 255);
        assert_eq!(a(0, 15), 255);
        assert_eq!(a(8, 8), 255);
        assert!(a(2, 2) < 255 && a(4, 4) > 0, "soft edge: {} {}", a(2, 2), a(4, 4));
        assert!(s.has_alpha());
        let flat = s.flattened(0xFFFFFF);
        assert_eq!(flat[0], 0xFFFF_FFFF);
        assert_eq!(flat[20], 0xFF11_2233);
    }
}
