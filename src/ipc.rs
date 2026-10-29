//! How the settings window, a second launch and `Nab.exe --nab` talk to the
//! running resident: window messages to its hidden window, found by class name.

use crate::win::{some, spawn_self, wide};
use std::ptr::null;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, FindWindowW, PostMessageW, WM_APP};

pub const RESIDENT_CLASS: &str = "Nab.Resident";

/// Re-read settings.toml.
pub const WM_NAB_RELOAD: u32 = WM_APP + 1;
/// Take a nab. wParam: a nab's index in the settings, or `KIND_FLAG | kind` for
/// the first nab of that kind. lParam: a delay in milliseconds (the settings
/// window's "Try it" gives you time to click into another window).
pub const WM_NAB_TAKE: u32 = WM_APP + 2;
/// Open the settings window.
pub const WM_NAB_SETTINGS: u32 = WM_APP + 3;
/// Quit.
pub const WM_NAB_QUIT: u32 = WM_APP + 4;

pub const KIND_FLAG: usize = 0x100;

pub fn resident() -> Option<HWND> {
    let class = wide(RESIDENT_CLASS);
    some(unsafe { FindWindowW(class.as_ptr(), null()) })
}

pub fn send(msg: u32, wparam: usize, lparam: isize) -> bool {
    resident().is_some_and(|h| unsafe { PostMessageW(h, msg, wparam, lparam) } != 0)
}

/// `what`: "1", "2"... (a nab from the settings, counted from 1) or a kind:
/// window, area, screen, all. Starts Nab first if it isn't running.
pub fn request_nab(what: &str) {
    let wparam = match what.trim().to_ascii_lowercase().as_str() {
        "window" => KIND_FLAG,
        "area" | "region" => KIND_FLAG | 1,
        "screen" => KIND_FLAG | 2,
        "all" | "all-screens" | "desktop" => KIND_FLAG | 3,
        n => match n.parse::<usize>() {
            Ok(k) if k >= 1 => k - 1,
            _ => return,
        },
    };
    if resident().is_none() {
        spawn_self(&["--autostart", "--no-hello"]);
        let until = Instant::now() + Duration::from_secs(4);
        while resident().is_none() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(40));
        }
    }
    // The area picker wants the keyboard (Esc); let the resident take the front.
    unsafe { AllowSetForegroundWindow(u32::MAX) };
    send(WM_NAB_TAKE, wparam, 0);
}
