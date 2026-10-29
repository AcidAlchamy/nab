//! The notification-area icon and its menu.

use crate::win::wide;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetSystemMetrics, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW,
    MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON, SM_CYSMICON, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_NULL,
};

pub const WM_TRAY: u32 = WM_APP + 20;
const ICON_ID: u32 = 1;
const NAB_BASE: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    Nab(usize),
    OpenLast,
    OpenFolder,
    Pause,
    Settings,
    Donate,
    Website,
    Quit,
}

const ITEMS: [(usize, Pick); 7] = [
    (1, Pick::OpenLast),
    (2, Pick::OpenFolder),
    (3, Pick::Pause),
    (4, Pick::Settings),
    (5, Pick::Donate),
    (6, Pick::Website),
    (7, Pick::Quit),
];

/// What the menu shows: each nab with its hotkey, and a few states.
pub struct MenuInfo {
    pub nabs: Vec<(String, String)>,
    pub paused: bool,
    pub has_last: bool,
}

/// Plain handles, so it can be copied out for the menu's modal loop.
#[derive(Clone, Copy)]
pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    icon_paused: HICON,
}

fn load_icon(id: u16) -> HICON {
    unsafe {
        LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            id as usize as *const u16,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )
    }
}

/// Let the tray menu follow the system's dark mode (uxtheme's unnamed
/// SetPreferredAppMode, ordinal 135; silently skipped where it doesn't exist).
fn dark_menus() {
    unsafe {
        let ux = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if ux.is_null() {
            return;
        }
        if let Some(f) = GetProcAddress(ux, 135usize as *const u8) {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set_mode(1); // AllowDark: follow the system setting
        }
        if let Some(f) = GetProcAddress(ux, 136usize as *const u8) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(f);
            flush();
        }
    }
}

impl Tray {
    pub fn new(hwnd: HWND, tip: &str) -> Self {
        dark_menus();
        let tray = Self { hwnd, icon: load_icon(1), icon_paused: load_icon(2) };
        tray.add(false, tip);
        tray
    }

    fn data(&self, paused: bool, tip: &str) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY,
            hIcon: if paused { self.icon_paused } else { self.icon },
            ..Default::default()
        };
        let tip = if paused { "Nab (hotkeys paused)" } else { tip };
        for (dst, src) in nid.szTip.iter_mut().take(127).zip(tip.encode_utf16()) {
            *dst = src;
        }
        nid
    }

    /// Adds the icon (again, after Explorer restarts).
    pub fn add(&self, paused: bool, tip: &str) {
        let mut nid = self.data(paused, tip);
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid);
            nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }
    }

    pub fn update(&self, paused: bool, tip: &str) {
        let nid = self.data(paused, tip);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }

    pub fn remove(&self) {
        let nid = self.data(false, "");
        unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    }

    /// Shows the menu at `at` and returns what was picked.
    pub fn menu(&self, at: POINT, info: &MenuInfo) -> Option<Pick> {
        unsafe {
            let menu = CreatePopupMenu();
            let add = |id: usize, text: &str, flags: u32| {
                AppendMenuW(menu, MF_STRING | flags, id, wide(text).as_ptr());
            };
            add(0, &format!("Nab {}  \u{2022}  donationware", crate::VERSION), MF_GRAYED);
            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
            for (i, (name, hotkey)) in info.nabs.iter().enumerate() {
                // Text after a tab is right-aligned: the hotkey.
                let text = if hotkey.is_empty() { format!("Nab: {name}") } else { format!("Nab: {name}\t{hotkey}") };
                add(NAB_BASE + i, &text, 0);
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
            add(1, "Open the last nab", if info.has_last { 0 } else { MF_GRAYED });
            add(2, "Open the Nab folder", 0);
            add(3, "Pause hotkeys", if info.paused { MF_CHECKED } else { 0 });
            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
            add(4, "Settings\u{2026}", 0);
            add(5, "Donate \u{2665}", 0);
            add(6, "Nab on the web", 0);
            AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
            add(7, "Quit Nab", 0);

            // Without this the menu won't close when you click elsewhere.
            SetForegroundWindow(self.hwnd);
            let id = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                at.x,
                at.y,
                0,
                self.hwnd,
                std::ptr::null(),
            ) as usize;
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            if (NAB_BASE..NAB_BASE + info.nabs.len()).contains(&id) {
                return Some(Pick::Nab(id - NAB_BASE));
            }
            ITEMS.iter().find(|(i, _)| *i == id).map(|(_, p)| *p)
        }
    }
}
