//! The flash: a white window over what was just nabbed that fades out in a
//! fifth of a second. One alpha value for the whole window, so it costs
//! nothing even over a 4K screen.

use crate::win::wide;
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetStockObject, WHITE_BRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_TOPMOST, LWA_ALPHA, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const MS: f32 = 220.0;
const START_ALPHA: f32 = 120.0;

pub struct Flash {
    hwnd: HWND,
    start: Option<Instant>,
}

unsafe extern "system" fn quiet_proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    unsafe { DefWindowProcW(h, m, w, l) }
}

impl Flash {
    pub fn new() -> Self {
        let hinst = unsafe { GetModuleHandleW(null()) };
        let class = wide("Nab.Flash");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(quiet_proc),
            hInstance: hinst,
            hbrBackground: unsafe { GetStockObject(WHITE_BRUSH) },
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        let hwnd = unsafe {
            RegisterClassExW(&wc);
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                null(),
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
        Self { hwnd, start: None }
    }

    pub fn play(&mut self, r: &RECT) {
        unsafe {
            SetLayeredWindowAttributes(self.hwnd, 0, START_ALPHA as u8, LWA_ALPHA);
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        self.start = Some(Instant::now());
    }

    pub fn active(&self) -> bool {
        self.start.is_some()
    }

    pub fn tick(&mut self) {
        let Some(start) = self.start else { return };
        let t = start.elapsed().as_secs_f32() * 1000.0 / MS;
        if t >= 1.0 {
            self.stop();
            return;
        }
        let a = START_ALPHA * (1.0 - t) * (1.0 - t);
        unsafe { SetLayeredWindowAttributes(self.hwnd, 0, a as u8, LWA_ALPHA) };
    }

    pub fn stop(&mut self) {
        if self.start.take().is_some() {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
    }
}
