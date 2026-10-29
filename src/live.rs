//! A few bytes of shared memory between the resident and the settings window:
//! how many nabs so far, and what became of each nab's hotkey. Plain atomics,
//! no locks, no messages.

use crate::config::MAX_NABS;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE};

#[repr(C)]
pub struct Live {
    nabs: AtomicU32,
    hotkeys: [AtomicU32; MAX_NABS],
}

/// What became of the hotkey a nab asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyState {
    Off,
    Ready,
    /// Another program already owns that key combination.
    Taken,
    /// The text couldn't be understood.
    Invalid,
    /// Hotkeys are paused from the tray.
    Paused,
}

impl Live {
    pub fn add_nab(&self) {
        self.nabs.fetch_add(1, Relaxed);
    }

    pub fn nabs(&self) -> u32 {
        self.nabs.load(Relaxed)
    }

    pub fn set_hotkey(&self, i: usize, s: HotkeyState) {
        if let Some(slot) = self.hotkeys.get(i) {
            slot.store(s as u32, Relaxed);
        }
    }

    pub fn hotkey(&self, i: usize) -> HotkeyState {
        match self.hotkeys.get(i).map_or(0, |s| s.load(Relaxed)) {
            1 => HotkeyState::Ready,
            2 => HotkeyState::Taken,
            3 => HotkeyState::Invalid,
            4 => HotkeyState::Paused,
            _ => HotkeyState::Off,
        }
    }
}

/// Opens (or creates) the shared block. Lives for the rest of the process.
pub fn open() -> Option<&'static Live> {
    let name = crate::win::wide("Local\\Nab.Live.v1");
    unsafe {
        let map = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            std::ptr::null(),
            PAGE_READWRITE,
            0,
            size_of::<Live>() as u32,
            name.as_ptr(),
        );
        if map.is_null() {
            return None;
        }
        let view = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, size_of::<Live>());
        // A fresh mapping is zeroed, which is a valid `Live`.
        (view.Value as *const Live).as_ref()
    }
}
