//! The area picker. The screens freeze and dim; drag out a rectangle, or click
//! a window to nab exactly that window (click the desktop for the whole
//! screen). A loupe beside the pointer magnifies the pixels so edges land
//! where you mean them. Arrow keys nudge the pointer one pixel. Esc or a right
//! click cancels.
//!
//! One borderless window per display shows its part of a single frozen
//! picture of the whole desktop, so a drag can cross screens and what you get
//! is exactly what you saw when you pressed the key.

use super::{Shot, Win, window_at};
use crate::paint::{self, Canvas, Font};
use crate::win::{self, Monitor, contains, intersect, wide};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::Once;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, COLORONCOLOR, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HDC, InvalidateRect,
    PAINTSTRUCT, SRCCOPY, SetStretchBltMode, StretchBlt, UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessagePos, GetMessageW, IDC_CROSS,
    LoadCursorW, MSG, PostQuitMessage, RegisterClassExW, SW_SHOWNA, SetCursor, SetCursorPos, SetForegroundWindow, ShowWindow,
    TranslateMessage, WM_ACTIVATEAPP, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
    WM_RBUTTONDOWN, WM_SETCURSOR, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: &str = "Nab.Area";
const ACID: u32 = 0xC6FF3A;
const INK: u32 = 0x140A2A;
const WHITE: u32 = 0xFFFFFF;
/// Pixels shown in the loupe, across.
const LOUPE_SRC: i32 = 15;

/// What was picked: the picture, where it was on screen, and the window if one was clicked.
pub struct Pick {
    pub shot: Shot,
    pub rect: RECT,
    pub window: Option<Win>,
}

struct Overlay {
    hwnd: HWND,
    mon: Monitor,
    scale: f32,
    back: Canvas,
    label: Font,
}

struct State {
    frozen: Canvas,
    dim: Canvas,
    /// The virtual desktop's top-left: frozen pixel (0, 0).
    origin: POINT,
    overlays: Vec<Overlay>,
    windows: Vec<Win>,
    names: Vec<Option<String>>,
    hover: Option<usize>,
    press: Option<POINT>,
    cursor: POINT,
    /// Set when the picker is finished: the rectangle and the window index, or None if cancelled.
    done: Option<Option<(RECT, Option<usize>)>>,
}

thread_local! {
    static AREA: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    AREA.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

/// Shows the picker over `frozen` (a picture of the whole virtual desktop,
/// whose top-left is `origin`) and returns what was picked, or None.
pub fn pick(frozen: Canvas, origin: POINT, windows: Vec<Win>) -> Option<Pick> {
    let dim = dimmed(&frozen)?;
    let hinst = unsafe { GetModuleHandleW(null()) };
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let class = wide(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(area_proc),
            hInstance: hinst,
            hCursor: unsafe { LoadCursorW(null_mut(), IDC_CROSS) },
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe { RegisterClassExW(&wc) };
    });

    let mut overlays = Vec::new();
    for mon in win::monitors() {
        let (w, h) = (mon.rect.right - mon.rect.left, mon.rect.bottom - mon.rect.top);
        let Some(back) = Canvas::new(w, h) else { continue };
        let class = wide(CLASS);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                wide("Nab area").as_ptr(),
                WS_POPUP,
                mon.rect.left,
                mon.rect.top,
                w,
                h,
                null_mut(),
                null_mut(),
                hinst,
                null(),
            )
        };
        if hwnd.is_null() {
            continue;
        }
        let scale = mon.scale();
        overlays.push(Overlay { hwnd, mon, scale, back, label: Font::new((13.5 * scale) as i32, 600) });
    }
    if overlays.is_empty() {
        return None;
    }

    let cursor = win::cursor();
    let names = vec![None; windows.len()];
    let hwnds: Vec<HWND> = overlays.iter().map(|o| o.hwnd).collect();
    let front = overlays.iter().find(|o| contains(&o.mon.rect, cursor)).map_or(hwnds[0], |o| o.hwnd);
    AREA.with(|a| {
        *a.borrow_mut() =
            Some(State { frozen, dim, origin, overlays, windows, names, hover: None, press: None, cursor, done: None })
    });
    with(|s| s.moved());
    for &h in &hwnds {
        unsafe {
            ShowWindow(h, SW_SHOWNA);
            UpdateWindow(h);
        }
    }
    unsafe {
        SetForegroundWindow(front);
        SetFocus(front);
    }

    let mut msg = MSG::default();
    while with(|s| s.done.is_none()).unwrap_or(false) {
        let got = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if got <= 0 {
            if got == 0 {
                unsafe { PostQuitMessage(msg.wParam as i32) }; // pass the quit on to the main loop
            }
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    let state = AREA.with(|a| a.borrow_mut().take())?;
    unsafe { ReleaseCapture() };
    for &h in &hwnds {
        unsafe { DestroyWindow(h) };
    }
    let (rect, index) = state.done.flatten()?;
    let local = RECT {
        left: rect.left - state.origin.x,
        top: rect.top - state.origin.y,
        right: rect.right - state.origin.x,
        bottom: rect.bottom - state.origin.y,
    };
    let shot = Shot::from_canvas(&state.frozen, &local)?;
    Some(Pick { shot, rect, window: index.map(|i| state.windows[i].clone()) })
}

/// The frozen picture, darkened and tinted towards Nab's ink.
fn dimmed(frozen: &Canvas) -> Option<Canvas> {
    let mut dim = Canvas::new(frozen.w, frozen.h)?;
    let (bg_r, bg_g, bg_b) = (0x0Du32, 0x0Bu32, 0x14u32);
    for (d, &p) in dim.px().iter_mut().zip(frozen.px_ref()) {
        let ch = |v: u32, bg: u32| (v * 108 + bg * 148) >> 8;
        *d = (ch((p >> 16) & 0xFF, bg_r) << 16) | (ch((p >> 8) & 0xFF, bg_g) << 8) | ch(p & 0xFF, bg_b);
    }
    Some(dim)
}

fn norm(a: POINT, b: POINT) -> RECT {
    RECT { left: a.x.min(b.x), top: a.y.min(b.y), right: a.x.max(b.x) + 1, bottom: a.y.max(b.y) + 1 }
}

impl State {
    fn scale_at(&self, p: POINT) -> f32 {
        self.overlays.iter().find(|o| contains(&o.mon.rect, p)).map_or(1.0, |o| o.scale)
    }

    fn monitor_at(&self, p: POINT) -> Option<RECT> {
        self.overlays.iter().find(|o| contains(&o.mon.rect, p)).map(|o| o.mon.rect)
    }

    fn dragging(&self) -> bool {
        self.press.is_some_and(|p| {
            let k = 4.0 * self.scale_at(p);
            ((p.x - self.cursor.x).abs() as f32) > k || ((p.y - self.cursor.y).abs() as f32) > k
        })
    }

    /// The lit rectangle right now: the drag, the window under the pointer, or its screen.
    fn current(&self) -> Option<RECT> {
        if let Some(p) = self.press.filter(|_| self.dragging()) {
            return Some(norm(p, self.cursor));
        }
        match self.hover {
            Some(i) => Some(self.windows[i].rect),
            None => self.monitor_at(self.cursor),
        }
    }

    fn label(&self) -> String {
        let Some(r) = self.current() else { return String::new() };
        let size = format!("{} \u{00D7} {}", r.right - r.left, r.bottom - r.top);
        if self.dragging() {
            return size;
        }
        match self.hover.and_then(|i| self.names[i].clone()) {
            Some(name) => format!("{name}  \u{00B7}  {size}"),
            None => format!("Whole screen  \u{00B7}  {size}"),
        }
    }

    fn moved(&mut self) {
        self.cursor = win::cursor();
        if self.press.is_none() {
            self.hover = window_at(&self.windows, self.cursor);
            if let Some(i) = self.hover {
                if self.names[i].is_none() {
                    self.names[i] = Some(self.windows[i].app());
                }
            }
        }
        for o in &self.overlays {
            unsafe { InvalidateRect(o.hwnd, null(), 0) };
        }
    }

    fn release(&mut self) {
        self.cursor = win::cursor();
        let dragged = self.dragging();
        let Some(p) = self.press.take() else { return };
        self.done = Some(if dragged {
            Some((norm(p, self.cursor), None))
        } else if let Some(i) = self.hover {
            Some((self.windows[i].rect, Some(i)))
        } else {
            self.monitor_at(self.cursor).map(|r| (r, None))
        });
    }

    fn draw(&mut self, hwnd: HWND, dc: HDC) {
        let Some(i) = self.overlays.iter().position(|o| o.hwnd == hwnd) else { return };
        let ov = &self.overlays[i];
        let look = Look { lit: self.current(), label: self.label(), cursor: self.cursor, hint: self.press.is_none() };
        let m = ov.mon.rect;
        compose(ov.back.dc, &self.frozen, &self.dim, self.origin, &m, ov.scale, &ov.label, &look);
        unsafe { BitBlt(dc, 0, 0, m.right - m.left, m.bottom - m.top, ov.back.dc, 0, 0, SRCCOPY) };
    }
}

/// What one frame of the picker shows.
struct Look {
    /// The lit rectangle, in screen coordinates.
    lit: Option<RECT>,
    label: String,
    cursor: POINT,
    /// The "Drag to nab an area" hint along the top.
    hint: bool,
}

/// Draws one display's picker frame into `b`, that display's back buffer.
#[allow(clippy::too_many_arguments)]
fn compose(b: HDC, frozen: &Canvas, dim: &Canvas, origin: POINT, m: &RECT, s: f32, font: &Font, look: &Look) {
    let (mw, mh) = (m.right - m.left, m.bottom - m.top);
    let cursor = look.cursor;
    unsafe { BitBlt(b, 0, 0, mw, mh, dim.dc, m.left - origin.x, m.top - origin.y, SRCCOPY) };
    {
        if let Some(r) = look.lit {
            if let Some(v) = intersect(&r, m) {
                unsafe {
                    BitBlt(
                        b,
                        v.left - m.left,
                        v.top - m.top,
                        v.right - v.left,
                        v.bottom - v.top,
                        frozen.dc,
                        v.left - origin.x,
                        v.top - origin.y,
                        SRCCOPY,
                    )
                };
            }
            let local =
                RECT { left: r.left - m.left, top: r.top - m.top, right: r.right - m.left, bottom: r.bottom - m.top };
            frame(b, &local, (2.0 * s).round().max(1.0) as i32, ACID);

            // The size (and app) under the rectangle, or inside it when there's no room below.
            let pad = (8.0 * s) as i32;
            let pill_h = (28.0 * s) as i32;
            let mut y = local.bottom + pad;
            if y + pill_h > mh {
                y = (local.bottom - pad - pill_h).max(local.top + pad);
            }
            let x = local.left.max(pad).min(mw - pad - 40);
            if contains(&RECT { left: 0, top: 0, right: mw, bottom: mh }, POINT { x: x.max(0), y: y.max(0) })
                && !look.label.is_empty()
            {
                paint::pill(b, x, y, &look.label, font, s, INK, WHITE, 0.92);
            }
        }

        if contains(m, cursor) {
            loupe(b, frozen, origin, m, cursor, s, font);
            if look.hint {
                let hint = "Drag to nab an area   \u{00B7}   Click a window   \u{00B7}   Esc cancels";
                let w = paint::text_width(b, hint, font) + (22.0 * s) as i32;
                paint::pill(b, (mw - w) / 2, (22.0 * s) as i32, hint, font, s, INK, 0xEEECF4, 0.9);
            }
        }
    }
}

/// A rectangle outline `t` pixels wide, drawn inside `r`.
fn frame(dc: HDC, r: &RECT, t: i32, rgb: u32) {
    unsafe {
        let brush = CreateSolidBrush(paint::colorref(rgb));
        for side in [
            RECT { left: r.left, top: r.top, right: r.right, bottom: r.top + t },
            RECT { left: r.left, top: r.bottom - t, right: r.right, bottom: r.bottom },
            RECT { left: r.left, top: r.top, right: r.left + t, bottom: r.bottom },
            RECT { left: r.right - t, top: r.top, right: r.right, bottom: r.bottom },
        ] {
            FillRect(dc, &side, brush);
        }
        DeleteObject(brush);
    }
}

/// The magnifier: the pixels around the pointer, big, with the one under it boxed.
fn loupe(b: HDC, frozen: &Canvas, origin: POINT, m: &RECT, cursor: POINT, s: f32, font: &Font) {
    let zoom = ((8.0 * s).round() as i32).max(6);
    let size = LOUPE_SRC * zoom;
    let off = (24.0 * s) as i32;
    let text_h = (34.0 * s) as i32;
    let (mw, mh) = (m.right - m.left, m.bottom - m.top);
    let (cx, cy) = (cursor.x - m.left, cursor.y - m.top);
    let x = if cx + off + size + 4 > mw { cx - off - size } else { cx + off };
    let y = if cy + off + size + text_h > mh { cy - off - size - text_h } else { cy + off };
    unsafe {
        SetStretchBltMode(b, COLORONCOLOR as _);
        StretchBlt(
            b,
            x,
            y,
            size,
            size,
            frozen.dc,
            cursor.x - origin.x - LOUPE_SRC / 2,
            cursor.y - origin.y - LOUPE_SRC / 2,
            LOUPE_SRC,
            LOUPE_SRC,
            SRCCOPY,
        );
    }
    let t = (1.0 * s).round().max(1.0) as i32;
    frame(b, &RECT { left: x - t, top: y - t, right: x + size + t, bottom: y + size + t }, t, WHITE);
    frame(b, &RECT { left: x - 3 * t, top: y - 3 * t, right: x + size + 3 * t, bottom: y + size + 3 * t }, 2 * t, INK);
    let c = (LOUPE_SRC / 2) * zoom;
    frame(b, &RECT { left: x + c - t, top: y + c - t, right: x + c + zoom + t, bottom: y + c + zoom + t }, 2 * t, ACID);
    paint::pill(
        b,
        x - 3 * t,
        y + size + (8.0 * s) as i32,
        &format!("{}, {}", cursor.x, cursor.y),
        font,
        s,
        INK,
        ACID,
        0.92,
    );
}

unsafe extern "system" fn area_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut ps) };
            with(|s| s.draw(hwnd, dc));
            unsafe { EndPaint(hwnd, &ps) };
            return 0;
        }
        WM_SETCURSOR => {
            unsafe { SetCursor(LoadCursorW(null_mut(), IDC_CROSS)) };
            return 1;
        }
        WM_MOUSEMOVE => {
            with(|s| s.moved());
        }
        WM_LBUTTONDOWN => {
            unsafe { SetCapture(hwnd) };
            // Where the press happened: on a slow PC the pointer has moved on by the time this runs.
            let at = unsafe { GetMessagePos() };
            let press = POINT { x: at as u16 as i16 as i32, y: (at >> 16) as u16 as i16 as i32 };
            with(|s| {
                s.cursor = press;
                s.press = Some(press);
            });
        }
        WM_LBUTTONUP => {
            unsafe { ReleaseCapture() };
            with(|s| s.release());
        }
        WM_RBUTTONDOWN => {
            with(|s| {
                if s.press.take().is_some() {
                    unsafe { ReleaseCapture() };
                    s.moved();
                } else {
                    s.done = Some(None);
                }
            });
        }
        WM_KEYDOWN => {
            let key = wparam as u16;
            let p = win::cursor();
            match key {
                VK_ESCAPE => {
                    with(|s| s.done = Some(None));
                }
                VK_RETURN | VK_SPACE => {
                    with(|s| {
                        s.press = Some(s.cursor);
                        s.release();
                    });
                }
                VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN => {
                    let (dx, dy) = match key {
                        VK_LEFT => (-1, 0),
                        VK_RIGHT => (1, 0),
                        VK_UP => (0, -1),
                        _ => (0, 1),
                    };
                    unsafe { SetCursorPos(p.x + dx, p.y + dy) };
                }
                _ => {}
            }
        }
        // Switching to another app cancels.
        WM_ACTIVATEAPP if wparam == 0 => {
            with(|s| {
                if s.done.is_none() {
                    s.done = Some(None);
                }
            });
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picker for the README, drawn by the real code over art/mock.py's
    /// made-up desktop: `NAB_DEMO=art/build cargo test demo_picker -- --ignored`.
    #[test]
    #[ignore = "writes README art; needs NAB_DEMO"]
    fn demo_picker() {
        let dir = std::path::PathBuf::from(std::env::var("NAB_DEMO").expect("NAB_DEMO=<folder with mock-desktop.png>"));
        let mock = crate::capture::save::load_png(&dir.join("mock-desktop.png"));
        let mut frozen = Canvas::new(mock.w, mock.h).unwrap();
        frozen.px().copy_from_slice(&mock.px);
        let dim = dimmed(&frozen).unwrap();
        let back = Canvas::new(mock.w, mock.h).unwrap();
        let m = RECT { left: 0, top: 0, right: mock.w, bottom: mock.h };
        let font = Font::new(14, 600);
        let shots = [
            (
                "picker-window.png",
                Look {
                    lit: Some(RECT { left: 640, top: 250, right: 1460, bottom: 750 }),
                    label: "Notes  \u{00B7}  820 \u{00D7} 500".into(),
                    cursor: POINT { x: 724, y: 321 },
                    hint: true,
                },
            ),
            (
                "picker-drag.png",
                Look {
                    lit: Some(norm(POINT { x: 118, y: 296 }, POINT { x: 492, y: 532 })),
                    label: "375 \u{00D7} 237".into(),
                    cursor: POINT { x: 492, y: 532 },
                    hint: false,
                },
            ),
        ];
        for (name, look) in shots {
            compose(back.dc, &frozen, &dim, POINT { x: 0, y: 0 }, &m, 1.0, &font, &look);
            let shot = Shot::from_canvas(&back, &m).unwrap();
            std::fs::write(dir.join(name), crate::capture::save::png(&shot, false).unwrap()).unwrap();
        }
    }

    #[test]
    fn drags_normalise_in_any_direction() {
        let a = POINT { x: 50, y: 40 };
        let b = POINT { x: 10, y: 90 };
        let r = norm(a, b);
        assert_eq!((r.left, r.top, r.right, r.bottom), (10, 40, 51, 91));
        assert_eq!(norm(b, a).left, r.left);
    }
}
