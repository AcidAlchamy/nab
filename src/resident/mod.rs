//! The resident: a hidden window, a tray icon and one hotkey per nab.
//!
//! It sleeps in GetMessage until you press a hotkey. Nothing polls; a timer
//! runs only while the card or the flash is on screen. Files are encoded and
//! written on a short-lived thread, so the next nab is never kept waiting.

mod card;
mod flash;
mod sound;
mod tray;

use crate::capture::{self, Shot, area, clip, name, save};
use crate::config::{Config, Format, Kind, MAX_NABS, Nab};
use crate::ipc::{KIND_FLAG, RESIDENT_CLASS, WM_NAB_QUIT, WM_NAB_RELOAD, WM_NAB_SETTINGS, WM_NAB_TAKE};
use crate::live::{HotkeyState, Live};
use crate::win::{self, open_path, open_url, reveal, spawn_self, wide};
use crate::{autostart, config, hotkey, live};
use card::{Hit, Status};
use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray::{MenuInfo, Pick, Tray, WM_TRAY};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, POINT, RECT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    KillTimer, MSG, MSGFLT_ALLOW, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetTimer,
    TranslateMessage, WM_APP, WM_CONTEXTMENU, WM_DESTROY, WM_DESTROYCLIPBOARD, WM_HOTKEY, WM_RENDERALLFORMATS,
    WM_RENDERFORMAT, WM_TIMER, WM_USER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};

const TIMER_FRAME: usize = 1;
const TIMER_LATER: usize = 2;
/// A save thread finished.
const WM_NAB_SAVED: u32 = WM_APP + 30;
/// How long a nab from the tray menu waits for the menu to fade away.
const TRAY_DELAY_MS: u32 = 230;

const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;

#[derive(Clone, Copy, Debug)]
enum Target {
    Index(usize),
    Kind(Kind),
}

/// The nab on the clipboard. Its PNG is made on a thread right after the nab
/// (or taken from the saved file); the pixels are kept only until then, in
/// case someone pastes before it's ready.
struct Clip {
    id: u32,
    shot: Option<Arc<Shot>>,
    png: Option<Arc<Vec<u8>>>,
}

/// A finished save (and the clipboard's PNG), handed back from its thread.
struct Saved {
    id: u32,
    /// The file written, None when this nab doesn't save files.
    result: Result<Option<PathBuf>, String>,
    png: Option<Vec<u8>>,
}

struct App {
    hwnd: HWND,
    cfg: Config,
    live: Option<&'static Live>,
    tray: Tray,
    card: card::Card,
    flash: flash::Flash,
    clip: Option<Clip>,
    last: Option<PathBuf>,
    saved: Arc<Mutex<Vec<Saved>>>,
    next_id: u32,
    later: Option<(Target, bool)>,
    frame_ms: u32,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app unless it's already borrowed further up the stack
/// (a modal loop like the tray menu or the area picker re-enters the window
/// procedure; hotkeys pressed then are simply ignored).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

pub(crate) fn card_mouse(msg: u32, x: i32, y: i32) {
    with_app(|app| {
        if let Some(hit) = app.card.mouse(msg, x, y) {
            app.on_card(hit);
        }
    });
}

pub(crate) fn card_wants_hand() -> bool {
    with_app(|app| app.card.wants_hand()).unwrap_or(false)
}

pub fn run(autostarted: bool, no_hello: bool) {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let name = wide("Local\\Nab.Resident");
    let _mutex = unsafe { CreateMutexW(null(), 1, name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Already running: a second launch opens the settings instead.
        if !autostarted {
            crate::ipc::send(WM_NAB_SETTINGS, 0, 0);
        }
        return;
    }

    let first_run = !config::exists();
    let cfg = Config::load();
    if first_run {
        let _ = cfg.save();
    }
    autostart::follow_moves();

    let hwnd = create_window();
    if hwnd.is_null() {
        return;
    }
    for msg in [WM_NAB_RELOAD, WM_NAB_TAKE, WM_NAB_SETTINGS, WM_NAB_QUIT] {
        unsafe { ChangeWindowMessageFilterEx(hwnd, msg, MSGFLT_ALLOW, null_mut()) };
    }

    let tray = Tray::new(hwnd, &tip(&cfg));
    let mut app = App {
        hwnd,
        cfg,
        live: live::open(),
        tray,
        card: card::Card::new(),
        flash: flash::Flash::new(),
        clip: None,
        last: None,
        saved: Arc::new(Mutex::new(Vec::new())),
        next_id: 1,
        later: None,
        frame_ms: 0,
        taskbar_created: unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) },
    };
    app.apply();
    let hello = app.cfg.hello_card && !no_hello;
    APP.with(|cell| *cell.borrow_mut() = Some(app));

    if first_run {
        spawn_self(&["--settings", "--welcome"]);
    } else if hello {
        spawn_self(&["--hello"]);
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    APP.with(|cell| cell.borrow_mut().take());
}

/// The tray icon's tooltip: what the main hotkey does.
fn tip(cfg: &Config) -> String {
    match cfg.nabs.iter().find(|n| !n.hotkey.trim().is_empty()) {
        Some(n) => format!("Nab: {} nabs {}", n.hotkey, describe(n.kind)),
        None => "Nab: right-click for a nab".into(),
    }
}

fn describe(kind: Kind) -> &'static str {
    match kind {
        Kind::Window => "the window you're in",
        Kind::Area => "an area you pick",
        Kind::Screen => "the screen you're on",
        Kind::AllScreens => "every screen",
    }
}

fn create_window() -> HWND {
    let hinst = unsafe { GetModuleHandleW(null()) };
    let class = wide(RESIDENT_CLASS);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&wc);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("Nab resident").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        )
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_HOTKEY if (1..=MAX_NABS).contains(&wparam) => {
            with_app(|app| app.take(Target::Index(wparam - 1), false));
        }
        WM_TIMER if wparam == TIMER_FRAME => {
            with_app(|app| app.on_frame());
        }
        WM_TIMER if wparam == TIMER_LATER => {
            unsafe { KillTimer(hwnd, TIMER_LATER) };
            with_app(|app| {
                if let Some((target, from_tray)) = app.later.take() {
                    app.take(target, from_tray);
                }
            });
        }
        WM_TRAY => {
            let event = (lparam & 0xFFFF) as u32;
            match event {
                WM_CONTEXTMENU => {
                    let at = POINT { x: (wparam & 0xFFFF) as i16 as i32, y: ((wparam >> 16) & 0xFFFF) as i16 as i32 };
                    // Read what the menu needs, then let go of the app: the menu
                    // runs its own message loop.
                    let state = with_app(|app| (app.tray, app.menu_info()));
                    if let Some((tray, info)) = state {
                        if let Some(pick) = tray.menu(at, &info) {
                            with_app(|app| app.on_pick(pick));
                        }
                    }
                }
                NIN_SELECT | NIN_KEYSELECT => {
                    with_app(|app| app.open_settings());
                }
                _ => {}
            }
        }
        WM_NAB_TAKE => {
            let target = if wparam & KIND_FLAG != 0 {
                Target::Kind(Kind::ALL.get(wparam & 0xFF).copied().unwrap_or(Kind::Window))
            } else {
                Target::Index(wparam)
            };
            with_app(|app| app.later(target, false, lparam.clamp(0, 10_000) as u32));
        }
        WM_NAB_SAVED => {
            with_app(|app| app.on_saved());
        }
        WM_NAB_RELOAD => {
            with_app(|app| app.reload());
        }
        WM_NAB_SETTINGS => {
            with_app(|app| app.open_settings());
        }
        WM_NAB_QUIT => {
            with_app(|app| app.quit());
        }
        WM_RENDERFORMAT if wparam as u32 == clip::png_format() => {
            if let Some(Some(png)) = with_app(|app| app.clip_png()) {
                clip::render_png(&png);
            }
            return 0;
        }
        WM_RENDERALLFORMATS => {
            if let Some(Some(png)) = with_app(|app| app.clip_png()) {
                clip::render_all(hwnd, &png);
            }
            return 0;
        }
        WM_DESTROYCLIPBOARD => {
            with_app(|app| app.clip = None);
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
        }
        _ => {
            if with_app(|app| msg == app.taskbar_created && msg != 0).unwrap_or(false) {
                with_app(|app| app.tray.add(app.cfg.paused, &tip(&app.cfg)));
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl App {
    fn apply(&mut self) {
        for i in 0..MAX_NABS {
            unsafe { UnregisterHotKey(self.hwnd, (i + 1) as i32) };
        }
        for i in 0..MAX_NABS {
            let state = match self.cfg.nabs.get(i) {
                None => HotkeyState::Off,
                Some(n) if n.hotkey.trim().is_empty() => HotkeyState::Off,
                Some(_) if self.cfg.paused => HotkeyState::Paused,
                Some(n) => match hotkey::parse(&n.hotkey) {
                    Some((mods, vk)) if unsafe { RegisterHotKey(self.hwnd, (i + 1) as i32, mods, vk) } != 0 => {
                        HotkeyState::Ready
                    }
                    Some(_) => HotkeyState::Taken,
                    None => HotkeyState::Invalid,
                },
            };
            if let Some(live) = self.live {
                live.set_hotkey(i, state);
            }
        }
        self.tray.update(self.cfg.paused, &tip(&self.cfg));
    }

    fn reload(&mut self) {
        self.cfg = Config::load();
        self.apply();
    }

    fn menu_info(&self) -> MenuInfo {
        MenuInfo {
            nabs: self.cfg.nabs.iter().map(|n| (n.name.clone(), n.hotkey.clone())).collect(),
            paused: self.cfg.paused,
            has_last: self.last.as_ref().is_some_and(|p| p.exists()),
        }
    }

    fn on_pick(&mut self, pick: Pick) {
        match pick {
            Pick::Nab(i) => self.later(Target::Index(i), true, TRAY_DELAY_MS),
            Pick::OpenLast => {
                if let Some(p) = &self.last {
                    open_path(p);
                }
            }
            Pick::OpenFolder => {
                let dir = self.cfg.nabs.first().map_or_else(config::default_folder, Nab::folder_path);
                let _ = std::fs::create_dir_all(&dir);
                open_path(&dir);
            }
            Pick::Pause => {
                self.cfg.paused = !self.cfg.paused;
                let _ = self.cfg.save();
                self.apply();
            }
            Pick::Settings => self.open_settings(),
            Pick::Donate => open_url(crate::DONATE_URL),
            Pick::Website => open_url(crate::HOME_URL),
            Pick::Quit => self.quit(),
        }
    }

    /// Opening the settings also picks up hand edits to settings.toml.
    fn open_settings(&mut self) {
        self.reload();
        spawn_self(&["--settings"]);
    }

    fn quit(&mut self) {
        // Leave a full copy on the clipboard: once Nab is gone it can't make the PNG on request.
        if let Some(png) = self.clip_png() {
            clip::render_all(self.hwnd, &png);
        }
        self.clip = None;
        self.card.hide();
        self.flash.stop();
        self.tray.remove();
        unsafe { DestroyWindow(self.hwnd) };
    }

    fn later(&mut self, target: Target, from_tray: bool, ms: u32) {
        if ms == 0 {
            self.take(target, from_tray);
        } else {
            self.later = Some((target, from_tray));
            unsafe { SetTimer(self.hwnd, TIMER_LATER, ms, None) };
        }
    }

    fn take(&mut self, target: Target, from_tray: bool) {
        let nab = match target {
            Target::Index(i) => self.cfg.nabs.get(i).cloned(),
            Target::Kind(k) => Some(self.cfg.nab_of_kind(k)),
        };
        let Some(nab) = nab else { return };
        // Out of the way first: the card and the flash never end up in a picture.
        let card_was_up = self.card.hide();
        self.flash.stop();
        if card_was_up {
            std::thread::sleep(Duration::from_millis(60)); // one composition pass without them
        }
        if let Some((shot, rect, app, title)) = self.capture(nab.kind, from_tray) {
            self.deliver(&nab, shot, rect, &app, &title);
        }
    }

    /// Grabs the pixels for a kind of nab: (picture, where on screen, app, window title).
    fn capture(&self, kind: Kind, from_tray: bool) -> Option<(Shot, RECT, String, String)> {
        let (pointer, corners) = (self.cfg.pointer, self.cfg.corners);
        match kind {
            Kind::Window => {
                let w = if from_tray { capture::front_app_window() } else { capture::foreground() };
                let Some(w) = w else { return self.capture(Kind::Screen, from_tray) }; // desktop or taskbar in front
                let mut shot = capture::grab(&w.rect, pointer)?;
                if corners && w.round > 0.0 {
                    shot.round_corners(w.round);
                }
                Some((shot, w.rect, w.app(), w.title()))
            }
            Kind::Screen => {
                let m = win::monitor_at(win::cursor())?;
                Some((capture::grab(&m.rect, pointer)?, m.rect, "Screen".into(), String::new()))
            }
            Kind::AllScreens => {
                let r = win::virtual_screen();
                Some((capture::grab(&r, pointer)?, r, "Desktop".into(), String::new()))
            }
            Kind::Area => {
                let vs = win::virtual_screen();
                let frozen = capture::grab_canvas(&vs)?;
                let windows = capture::windows_on_screen();
                let pick = area::pick(frozen, POINT { x: vs.left, y: vs.top }, windows)?;
                let mut shot = pick.shot;
                let (app, title) = match &pick.window {
                    Some(w) => {
                        if corners && w.round > 0.0 {
                            shot.round_corners(w.round);
                        }
                        (w.app(), w.title())
                    }
                    None => ("Area".into(), String::new()),
                };
                Some((shot, pick.rect, app, title))
            }
        }
    }

    fn deliver(&mut self, nab: &Nab, shot: Shot, rect: RECT, app: &str, title: &str) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        if let Some(live) = self.live {
            live.add_nab();
        }
        if self.cfg.flash {
            self.flash.play(&rect);
        }
        if self.cfg.sound {
            sound::play();
        }

        let shot = Arc::new(shot);
        let copied = nab.copy && clip::put(self.hwnd, &shot);
        if copied {
            self.clip = Some(Clip { id, shot: Some(shot.clone()), png: None });
        }

        let file = nab.save.then(|| {
            let facts = name::Facts { app, title, nab: &nab.name, w: shot.w, h: shot.h, when: name::Stamp::now() };
            name::path_for(&nab.folder_path(), &nab.pattern, &facts, nab.format.ext())
        });
        if let Some(path) = &file {
            self.last = Some(path.clone());
        }
        if file.is_some() || copied {
            let (shot, path, format, quality) = (shot.clone(), file.clone(), nab.format, self.cfg.jpg_quality);
            let (saved, hwnd) = (self.saved.clone(), self.hwnd as isize);
            std::thread::spawn(move || {
                let mut png = None;
                let result = match &path {
                    Some(path) => match save::encode(&shot, format, quality) {
                        Ok(bytes) => {
                            let written = save::write(path, &bytes);
                            // The file's own PNG doubles as the clipboard's.
                            if format == Format::Png {
                                png = Some(bytes);
                            }
                            written.map(|()| Some(path.clone())).map_err(|e| friendly(&e))
                        }
                        Err(e) => Err(friendly(&e)),
                    },
                    None => Ok(None),
                };
                if copied && png.is_none() {
                    png = save::png(&shot, true).ok();
                }
                if let Ok(mut list) = saved.lock() {
                    list.push(Saved { id, result, png });
                }
                unsafe { PostMessageW(hwnd as HWND, WM_NAB_SAVED, 0, 0) };
            });
        }

        if self.cfg.card {
            let status = if file.is_some() { Status::Saving } else { Status::CopiedOnly };
            let info = card::Info { id, app: app.to_owned(), file, copied, size: (shot.w, shot.h) };
            self.card.show(&shot, info, status, &rect);
        }
        self.ensure_timer();
    }

    fn on_saved(&mut self) {
        let items = self.saved.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
        for item in items {
            match item.result {
                Ok(Some(_)) => self.card.set_status(item.id, Status::Saved),
                Ok(None) => {}
                Err(why) => self.card.set_status(item.id, Status::Failed(why)),
            }
            if let (Some(png), Some(c)) = (item.png, self.clip.as_mut()) {
                if c.id == item.id {
                    c.png.get_or_insert_with(|| Arc::new(png));
                    c.shot = None; // the PNG is all the clipboard needs now
                }
            }
        }
    }

    /// The clipboard's PNG, made now if no save made it already.
    fn clip_png(&mut self) -> Option<Arc<Vec<u8>>> {
        let c = self.clip.as_mut()?;
        if c.png.is_none() {
            c.png = save::png(c.shot.as_ref()?, true).ok().map(Arc::new);
            c.shot = None;
        }
        c.png.clone()
    }

    fn on_card(&mut self, hit: Hit) {
        let file = self.card.file().filter(|f| f.exists());
        match (hit, file) {
            (Hit::Picture | Hit::Open, Some(f)) => {
                open_path(&f);
                self.card.dismiss();
            }
            (Hit::Folder, Some(f)) => {
                reveal(&f);
                self.card.dismiss();
            }
            (Hit::Close, _) => self.card.dismiss(),
            _ => {}
        }
        self.ensure_timer();
    }

    fn ensure_timer(&mut self) {
        let want = if self.flash.active() || self.card.animating() {
            16
        } else if self.card.active() {
            125
        } else {
            0
        };
        if want != self.frame_ms {
            unsafe {
                if want == 0 {
                    KillTimer(self.hwnd, TIMER_FRAME);
                } else {
                    SetTimer(self.hwnd, TIMER_FRAME, want, None);
                }
            }
            self.frame_ms = want;
        }
    }

    fn on_frame(&mut self) {
        self.flash.tick();
        self.card.tick();
        self.ensure_timer();
    }
}

/// A save error in a few plain words.
fn friendly(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        PermissionDenied => "no permission to write in that folder".into(),
        NotFound => "that folder can't be reached".into(),
        StorageFull => "the drive is full".into(),
        _ => {
            let s = e.to_string();
            if s.chars().count() > 60 { format!("{}\u{2026}", s.chars().take(59).collect::<String>()) } else { s }
        }
    }
}
