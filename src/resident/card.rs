//! The nab card: a small preview in the corner of the screen after each nab.
//! The picture, the app it came from, the file it went to, and two buttons
//! (Open, Show in folder). It fades away after a few seconds unless you're
//! pointing at it, never takes the keyboard from the app you're in, and is
//! hidden before every nab so it never ends up in one.
//!
//! Drawn by hand into a per-pixel-alpha window: no UI toolkit in the resident.

use crate::capture::Shot;
use crate::paint::{self, Canvas, Font, apply_mask, cover, rrect};
use crate::win::{self, wide};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, DT_CENTER, DT_LEFT, DT_VCENTER};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_TOPMOST, IDC_ARROW, IDC_HAND, LoadCursorW, RegisterClassExW, SW_HIDE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetCursor, SetWindowPos, ShowWindow, ULW_ALPHA,
    UpdateLayeredWindow, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const W: f32 = 384.0;
const H: f32 = 106.0;
const RADIUS: f32 = 12.0;
const SHOW_MS: f32 = 5_000.0;
const FADE_IN_MS: f32 = 160.0;
const FADE_OUT_MS: f32 = 220.0;

/// Not in windows-sys's WindowsAndMessaging; it lives with the common controls.
const WM_MOUSELEAVE: u32 = 0x02A3;

const BG: u32 = 0x161320;
const WELL: u32 = 0x0A0810;
const HI: u32 = 0x1F1A2C;
const LINE: u32 = 0x2A243B;
const TEXT: u32 = 0xEEECF4;
const MUTED: u32 = 0xA39DB6;
const FAINT: u32 = 0x6E6884;
const ACID: u32 = 0xC6FF3A;
const VIOLET: u32 = 0x8B3DFF;
const PINK: u32 = 0xFF2E7E;
const BAD: u32 = 0xFF5A5A;

#[derive(Clone, Debug)]
pub enum Status {
    Saving,
    Saved,
    Failed(String),
    /// Only copied: this nab doesn't save files.
    CopiedOnly,
}

/// What a click on the card asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Nothing,
    Picture,
    Open,
    Folder,
    Close,
}

pub struct Info {
    pub id: u32,
    pub app: String,
    pub file: Option<PathBuf>,
    pub copied: bool,
    pub size: (i32, i32),
}

struct Fonts {
    scale: f32,
    title: Font,
    name: Font,
    small: Font,
}

pub struct Card {
    hwnd: HWND,
    canvas: Option<Canvas>,
    mask: Vec<u8>,
    thumb: Vec<u32>,
    thumb_size: (i32, i32),
    fonts: Option<Fonts>,
    info: Option<Info>,
    status: Status,
    scale: f32,
    pos: POINT,
    began: Instant,
    last_tick: Instant,
    last_draw: Instant,
    shown_ms: f32,
    leaving: Option<Instant>,
    visible: bool,
    hover: Hit,
    pointer_in: bool,
    hits: Vec<(Hit, RECT)>,
}

impl Card {
    pub fn new() -> Self {
        let hinst = unsafe { GetModuleHandleW(null()) };
        let class = wide("Nab.Card");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(card_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        let hwnd = unsafe {
            RegisterClassExW(&wc);
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                wide("Nab card").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                hinst,
                null(),
            )
        };
        let now = Instant::now();
        Self {
            hwnd,
            canvas: None,
            mask: Vec::new(),
            thumb: Vec::new(),
            thumb_size: (0, 0),
            fonts: None,
            info: None,
            status: Status::Saving,
            scale: 1.0,
            pos: POINT { x: 0, y: 0 },
            began: now,
            last_tick: now,
            last_draw: now,
            shown_ms: 0.0,
            leaving: None,
            visible: false,
            hover: Hit::Nothing,
            pointer_in: false,
            hits: Vec::new(),
        }
    }

    pub fn file(&self) -> Option<PathBuf> {
        self.info.as_ref().and_then(|i| i.file.clone())
    }

    pub fn active(&self) -> bool {
        self.visible
    }

    /// Shows the card for a fresh nab, in the corner of the screen it came from.
    pub fn show(&mut self, shot: &Shot, info: Info, status: Status, near: &RECT) {
        let Some(mon) = win::monitor_of_rect(near) else { return };
        self.prepare(shot, info, status, mon.scale());
        let (w, h) = ((W * self.scale).round() as i32, (H * self.scale).round() as i32);
        let margin = (16.0 * self.scale) as i32;
        self.pos = POINT { x: mon.work.right - w - margin, y: mon.work.bottom - h - margin };
        self.render();
        if !self.visible {
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            };
            self.visible = true;
        }
    }

    /// Everything for a new nab except putting it on screen.
    fn prepare(&mut self, shot: &Shot, info: Info, status: Status, scale: f32) {
        self.scale = scale;
        let s = self.scale;
        let (w, h) = ((W * s).round() as i32, (H * s).round() as i32);
        if self.canvas.as_ref().is_none_or(|c| c.w != w || c.h != h) {
            self.canvas = Canvas::new(w, h);
            let r = RADIUS * s;
            self.mask = (0..w * h)
                .map(|i| (cover(i % w, i / w, 0.0, 0.0, w as f32, h as f32, r) * 255.0 + 0.5) as u8)
                .collect();
        }
        if self.fonts.as_ref().is_none_or(|f| f.scale != s) {
            self.fonts = Some(Fonts {
                scale: s,
                title: Font::new((15.5 * s) as i32, 700),
                name: Font::new((13.5 * s) as i32, 600),
                small: Font::new((12.0 * s) as i32, 400),
            });
        }
        let (bw, bh) = (((124.0 - 8.0) * s) as i32, ((H - 24.0 - 8.0) * s) as i32);
        let k = (bw as f32 / shot.w as f32).min(bh as f32 / shot.h as f32).min(1.0);
        let (tw, th) = (((shot.w as f32 * k).round() as i32).max(1), ((shot.h as f32 * k).round() as i32).max(1));
        self.thumb = shrink(&shot.px, shot.w, shot.h, tw, th);
        self.thumb_size = (tw, th);
        self.info = Some(info);
        self.status = status;
        let now = Instant::now();
        self.began = now;
        self.last_tick = now;
        self.shown_ms = 0.0;
        self.leaving = None;
        self.hover = Hit::Nothing;
        self.pointer_in = false;
    }

    pub fn set_status(&mut self, id: u32, status: Status) {
        if self.info.as_ref().is_some_and(|i| i.id == id) {
            self.status = status;
            if self.visible {
                self.render();
            }
        }
    }

    /// Hides straight away (before a nab). Returns whether it was on screen.
    pub fn hide(&mut self) -> bool {
        let was = self.visible;
        if was {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
        self.visible = false;
        self.leaving = None;
        self.pointer_in = false;
        self.hover = Hit::Nothing;
        was
    }

    /// Fades out.
    pub fn dismiss(&mut self) {
        if self.visible && self.leaving.is_none() {
            self.leaving = Some(Instant::now());
        }
    }

    /// Fading in or out (wants frames); otherwise it only needs a redraw now and then.
    pub fn animating(&self) -> bool {
        self.visible && (self.began.elapsed().as_secs_f32() * 1000.0 < FADE_IN_MS || self.leaving.is_some())
    }

    pub fn tick(&mut self) {
        if !self.visible {
            return;
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32() * 1000.0;
        self.last_tick = now;
        // Ask where the pointer is rather than trusting enter/leave messages:
        // a click that hides the card never gets its "leave".
        if let Some(c) = &self.canvas {
            let p = win::cursor();
            let r = RECT { left: self.pos.x, top: self.pos.y, right: self.pos.x + c.w, bottom: self.pos.y + c.h };
            self.pointer_in = win::contains(&r, p);
        }
        if !self.pointer_in {
            self.shown_ms += dt;
        }
        if self.leaving.is_none() && self.shown_ms >= SHOW_MS {
            self.leaving = Some(now);
        }
        if self.leaving.is_some_and(|t| t.elapsed().as_secs_f32() * 1000.0 >= FADE_OUT_MS) {
            self.hide();
            self.canvas = None; // give the memory back until the next nab
            self.mask.clear();
            self.thumb.clear();
            return;
        }
        // The time-left line moves slowly: a full redraw 8 times a second is
        // plenty. Fades only change the window's opacity and position.
        if now.duration_since(self.last_draw).as_millis() >= 125 {
            self.render();
        } else if self.animating() {
            self.present();
        }
    }

    /// Opacity and slide for this moment of the fade in or out.
    fn motion(&self) -> (f32, i32) {
        let ease = |t: f32| 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
        let t_in = ease(self.began.elapsed().as_secs_f32() * 1000.0 / FADE_IN_MS);
        let t_out = self.leaving.map_or(0.0, |t| ease(t.elapsed().as_secs_f32() * 1000.0 / FADE_OUT_MS));
        let alpha = t_in * (1.0 - t_out);
        let slide = ((1.0 - t_in) * 12.0 + t_out * 6.0) * self.scale;
        (alpha, slide.round() as i32)
    }

    /// Puts the drawn card on screen at this moment's opacity and position.
    fn present(&self) {
        let Some(canvas) = &self.canvas else { return };
        let (alpha, slide) = self.motion();
        let pos = POINT { x: self.pos.x, y: self.pos.y + slide };
        let size = SIZE { cx: canvas.w, cy: canvas.h };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (alpha * 255.0) as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe { UpdateLayeredWindow(self.hwnd, null_mut(), &pos, &size, canvas.dc, &src, 0, &blend, ULW_ALPHA) };
    }

    fn render(&mut self) {
        self.last_draw = Instant::now();
        let (Some(info), Some(fonts)) = (&self.info, &self.fonts) else { return };
        let Some(canvas) = self.canvas.as_mut() else { return };
        let s = self.scale;
        let (w, h) = (canvas.w, canvas.h);
        let saved = matches!(self.status, Status::Saved) && info.file.is_some();

        // Layout, in logical pixels.
        let well = (12.0, 12.0, 124.0, H - 24.0);
        let x0 = 150.0;
        let close = (W - 32.0, 8.0, 24.0, 24.0);
        let open = (x0, H - 34.0, 64.0, 24.0);
        let folder = (x0 + 70.0, H - 34.0, 120.0, 24.0);
        let mut hits = vec![(Hit::Close, close), (Hit::Picture, well)];
        if saved {
            hits.push((Hit::Open, open));
            hits.push((Hit::Folder, folder));
        }
        let hover = self.hover;
        let sc = |v: f32| v * s;
        let at = |x: f32, y: f32, bw: f32, bh: f32| RECT {
            left: (x * s) as i32,
            top: (y * s) as i32,
            right: ((x + bw) * s) as i32,
            bottom: ((y + bh) * s) as i32,
        };
        self.hits = hits.iter().map(|&(k, (x, y, bw, bh))| (k, at(x, y, bw, bh))).collect();

        {
            let px = canvas.px();
            px.fill(0xFF00_0000 | BG);
            // The brand line along the top.
            let rule = (3.0 * s).round() as i32;
            for y in 0..rule.min(h) {
                for x in 0..w {
                    let t = x as f32 / (w - 1).max(1) as f32;
                    let c = if t < 0.6 {
                        paint::mix(VIOLET, PINK, t / 0.6)
                    } else {
                        paint::mix(PINK, ACID, (t - 0.6) / 0.4)
                    };
                    px[(y * w + x) as usize] = 0xFF00_0000 | c;
                }
            }
            rrect(px, w, 0.0, 0.0, w as f32, h as f32, RADIUS * s, LINE, 1.0);

            // The picture in its well.
            let (wx, wy, ww, wh) = (sc(well.0), sc(well.1), sc(well.2), sc(well.3));
            rrect(px, w, wx, wy, ww, wh, sc(8.0), WELL, 0.0);
            let (tw, th) = self.thumb_size;
            let (tx, ty) = ((wx + (ww - tw as f32) / 2.0).round() as i32, (wy + (wh - th as f32) / 2.0).round() as i32);
            for y in 0..th {
                for x in 0..tw {
                    let (dx, dy) = (tx + x, ty + y);
                    if dx < 0 || dy < 0 || dx >= w || dy >= h {
                        continue;
                    }
                    let p = self.thumb[(y * tw + x) as usize];
                    let a = (p >> 24) as f32 / 255.0;
                    let d = &mut px[(dy * w + dx) as usize];
                    *d = 0xFF00_0000 | paint::mix(*d & 0xFF_FFFF, p & 0xFF_FFFF, a);
                }
            }
            let ring = if hover == Hit::Picture { ACID } else { LINE };
            rrect(px, w, wx, wy, ww, wh, sc(8.0), ring, 1.0);

            if saved {
                for (kind, (x, y, bw, bh)) in [(Hit::Open, open), (Hit::Folder, folder)] {
                    rrect(px, w, sc(x), sc(y), sc(bw), sc(bh), sc(8.0), HI, 0.0);
                    rrect(px, w, sc(x), sc(y), sc(bw), sc(bh), sc(8.0), if hover == kind { ACID } else { LINE }, 1.0);
                }
            }
            if hover == Hit::Close {
                rrect(px, w, sc(close.0), sc(close.1), sc(close.2), sc(close.3), sc(7.0), HI, 0.0);
            }

            // Time left, a thin lime line along the bottom.
            let left = (1.0 - self.shown_ms / SHOW_MS).clamp(0.0, 1.0);
            let (bx, by, bw) = (sc(RADIUS), h as f32 - sc(3.0), (w as f32 - 2.0 * sc(RADIUS)) * left);
            if bw > 0.5 {
                rrect(
                    px,
                    w,
                    bx,
                    by,
                    bw,
                    sc(2.0),
                    sc(1.0),
                    paint::mix(BG, ACID, if self.pointer_in { 0.35 } else { 0.8 }),
                    0.0,
                );
            }
        }

        // Words (GDI text on the opaque card).
        let dc = canvas.dc;
        let title_r = at(x0, 10.0, 70.0, 24.0);
        paint::text(dc, "Nabbed", title_r, &fonts.title, TEXT, DT_LEFT | DT_VCENTER);
        let tw = paint::text_width(dc, "Nabbed", &fonts.title);
        let app = format!("\u{00B7}  {}", info.app);
        let app_r = RECT {
            left: title_r.left + tw + (7.0 * s) as i32,
            top: title_r.top,
            right: (sc(W - 40.0)) as i32,
            bottom: title_r.bottom,
        };
        paint::text(dc, &app, app_r, &fonts.name, MUTED, DT_LEFT | DT_VCENTER);
        paint::text(
            dc,
            "\u{00D7}",
            at(close.0, close.1, close.2, close.3),
            &fonts.title,
            if hover == Hit::Close { TEXT } else { FAINT },
            DT_CENTER | DT_VCENTER,
        );

        let file_name = info.file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned());
        let line2 = file_name.unwrap_or_else(|| format!("{} \u{00D7} {}", info.size.0, info.size.1));
        let line2_r = at(x0, 35.0, W - x0 - 14.0, 20.0);
        let line2 = paint::fit_middle(dc, &line2, &fonts.name, line2_r.right - line2_r.left, 12);
        paint::text(dc, &line2, line2_r, &fonts.name, TEXT, DT_LEFT | DT_VCENTER);

        let copied = if info.copied { "Copied" } else { "" };
        let (line3, color) = match &self.status {
            Status::Saving => (join(copied, "Saving\u{2026}"), FAINT),
            Status::Saved => {
                let folder = info
                    .file
                    .as_ref()
                    .and_then(|f| f.parent())
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (join(copied, &format!("Saved in {folder}")), FAINT)
            }
            Status::Failed(why) => (format!("Couldn't save: {why}"), BAD),
            Status::CopiedOnly => (
                if info.copied { "Copied to the clipboard".into() } else { "Couldn't reach the clipboard".into() },
                FAINT,
            ),
        };
        let line3_y = if saved { 55.0 } else { 58.0 };
        paint::text(dc, &line3, at(x0, line3_y, W - x0 - 14.0, 18.0), &fonts.small, color, DT_LEFT | DT_VCENTER);
        if saved {
            for (kind, label, (x, y, bw, bh)) in [(Hit::Open, "Open", open), (Hit::Folder, "Show in folder", folder)] {
                paint::text(
                    dc,
                    label,
                    at(x, y, bw, bh),
                    &fonts.small,
                    if hover == kind { ACID } else { TEXT },
                    DT_CENTER | DT_VCENTER,
                );
            }
        }

        apply_mask(canvas.px(), &self.mask);
        self.present();
    }

    fn hit_at(&self, x: i32, y: i32) -> Hit {
        self.hits.iter().find(|(_, r)| win::contains(r, POINT { x, y })).map_or(Hit::Nothing, |(k, _)| *k)
    }

    /// Mouse over the card; returns a click's target on button-up.
    pub fn mouse(&mut self, msg: u32, x: i32, y: i32) -> Option<Hit> {
        match msg {
            WM_MOUSEMOVE => {
                if !self.pointer_in {
                    self.pointer_in = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    unsafe { TrackMouseEvent(&mut tme) };
                }
                let hit = self.hit_at(x, y);
                if hit != self.hover {
                    self.hover = hit;
                    self.render();
                }
                None
            }
            WM_MOUSELEAVE => {
                self.pointer_in = false;
                self.hover = Hit::Nothing;
                self.render();
                None
            }
            WM_LBUTTONUP => Some(self.hit_at(x, y)),
            _ => None,
        }
    }

    pub fn wants_hand(&self) -> bool {
        self.hover != Hit::Nothing
    }
}

fn join(a: &str, b: &str) -> String {
    if a.is_empty() { b.to_owned() } else { format!("{a}  \u{00B7}  {b}") }
}

/// Scales a picture down by averaging the pixels each new one covers
/// (premultiplied, so transparent corners don't darken the edges).
pub fn shrink(src: &[u32], sw: i32, sh: i32, dw: i32, dh: i32) -> Vec<u32> {
    if dw >= sw && dh >= sh {
        return src.to_vec();
    }
    let mut out = vec![0u32; (dw * dh) as usize];
    for dy in 0..dh {
        let y0 = dy * sh / dh;
        let y1 = ((dy + 1) * sh / dh).max(y0 + 1).min(sh);
        for dx in 0..dw {
            let x0 = dx * sw / dw;
            let x1 = ((dx + 1) * sw / dw).max(x0 + 1).min(sw);
            let (mut r, mut g, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for &p in &src[(y * sw + x0) as usize..(y * sw + x1) as usize] {
                    let pa = (p >> 24) as u64;
                    a += pa;
                    r += ((p >> 16) & 0xFF) as u64 * pa;
                    g += ((p >> 8) & 0xFF) as u64 * pa;
                    b += (p & 0xFF) as u64 * pa;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            out[(dy * dw + dx) as usize] = if a == 0 {
                0
            } else {
                let ch = |v: u64| ((v + a / 2) / a).min(255) as u32;
                (((a + n / 2) / n) as u32) << 24 | ch(r) << 16 | ch(g) << 8 | ch(b)
            };
        }
    }
    out
}

unsafe extern "system" fn card_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_MOUSEACTIVATE => return 3, // MA_NOACTIVATE: clicks never take focus from your app
        WM_SETCURSOR => {
            let hand = super::card_wants_hand();
            unsafe { SetCursor(LoadCursorW(null_mut(), if hand { IDC_HAND } else { IDC_ARROW })) };
            return 1;
        }
        WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONUP => {
            let (x, y) = ((lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32);
            super::card_mouse(msg, x, y);
            return 0;
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The card for the README, drawn by the real code (never shown on screen):
    /// `NAB_DEMO=art/build cargo test demo_card -- --ignored` writes card.png
    /// there, from mock-window.png (art/mock.py).
    #[test]
    #[ignore = "writes README art; needs NAB_DEMO"]
    fn demo_card() {
        let dir = std::path::PathBuf::from(std::env::var("NAB_DEMO").expect("NAB_DEMO=<folder with mock-window.png>"));
        let shot = crate::capture::save::load_png(&dir.join("mock-window.png"));
        let mut card = Card::new();
        let info = Info {
            id: 1,
            app: "Notes".into(),
            file: Some(PathBuf::from(r"C:\Users\You\Pictures\Nab\Notes 2026-10-29 21-14-03.png")),
            copied: true,
            size: (shot.w, shot.h),
        };
        card.prepare(&shot, info, Status::Saved, 2.0);
        card.shown_ms = SHOW_MS * 0.35;
        card.hover = Hit::Folder;
        card.render();
        let c = card.canvas.as_ref().unwrap();
        // Premultiplied back to straight alpha.
        let px = c
            .px_ref()
            .iter()
            .map(|&p| {
                let a = p >> 24;
                if a == 0 || a == 255 {
                    return p;
                }
                let ch = |s: u32| ((((p >> s) & 0xFF) * 255 + a / 2) / a).min(255) << s;
                (a << 24) | ch(16) | ch(8) | ch(0)
            })
            .collect();
        let out = Shot { w: c.w, h: c.h, px };
        std::fs::write(dir.join("card.png"), crate::capture::save::png(&out, false).unwrap()).unwrap();
    }

    #[test]
    fn shrinking_averages() {
        let src = vec![0xFF00_0000, 0xFFFF_FFFF, 0xFF00_0000, 0xFFFF_FFFF];
        let out = shrink(&src, 2, 2, 1, 1);
        assert_eq!(out[0] >> 24, 0xFF);
        assert_eq!((out[0] >> 16) & 0xFF, 128);
        let clear = shrink(&[0, 0xFFFF_0000], 2, 1, 1, 1);
        assert_eq!(clear[0] & 0xFF_FFFF, 0xFF_0000, "transparent pixels don't darken");
        assert_eq!(clear[0] >> 24, 128);
    }
}
