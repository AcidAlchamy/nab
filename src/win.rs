//! Small Win32 helpers shared by the resident and the settings window.

use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO, MONITORINFOEXW, MonitorFromPoint, MonitorFromRect,
};
use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Shell::{FOLDERID_Pictures, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, GetClassNameW, GetCursorPos, GetSystemMetrics, GetWindowTextW, GetWindowThreadProcessId,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWNORMAL,
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

pub fn open_url(url: &str) {
    let (verb, url) = (wide("open"), wide(url));
    unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), url.as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

/// Starts another copy of this exe with `args` (the settings window, the hello card).
pub fn spawn_self(args: &[&str]) {
    if let Ok(exe) = std::env::current_exe() {
        // Let the new window come to the front: we were the one the user clicked.
        unsafe { AllowSetForegroundWindow(u32::MAX) };
        let _ = std::process::Command::new(exe).args(args).spawn();
    }
}

pub fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    p
}

#[derive(Clone)]
pub struct Monitor {
    pub handle: HMONITOR,
    pub rect: RECT,
    /// The part not covered by the taskbar.
    pub work: RECT,
}

impl Monitor {
    /// Effective DPI scale (1.0 = 96 dpi).
    pub fn scale(&self) -> f32 {
        if let Some(scale) = test_scale() {
            return scale;
        }
        let (mut x, mut y) = (96u32, 96u32);
        unsafe { GetDpiForMonitor(self.handle, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
        x.max(96) as f32 / 96.0
    }
}

/// A scale-test build's stand-in for Windows' display scale.
pub fn test_scale() -> Option<f32> {
    if !cfg!(feature = "scale-test") {
        return None;
    }
    std::env::var("NAB_TEST_SCALE").ok()?.parse().ok()
}

pub fn monitor_info(handle: HMONITOR) -> Option<Monitor> {
    let mut mi: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(handle, &mut mi as *mut _ as *mut MONITORINFO) } == 0 {
        return None;
    }
    Some(Monitor { handle, rect: mi.monitorInfo.rcMonitor, work: mi.monitorInfo.rcWork })
}

pub fn monitor_at(p: POINT) -> Option<Monitor> {
    monitor_info(unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST) })
}

pub fn primary_monitor() -> Option<Monitor> {
    monitor_info(unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) })
}

/// Every display, left to right.
pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, data: isize) -> i32 {
        let list = unsafe { &mut *(data as *mut Vec<Monitor>) };
        if let Some(info) = monitor_info(m) {
            list.push(info);
        }
        1
    }
    let mut list: Vec<Monitor> = Vec::new();
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(each), &mut list as *mut _ as isize) };
    list.sort_by_key(|m| (m.rect.left, m.rect.top));
    list
}

/// The HWND of a window or null, as an Option.
pub fn some(h: HWND) -> Option<HWND> {
    (!h.is_null()).then_some(h)
}

pub fn monitor_of_rect(r: &RECT) -> Option<Monitor> {
    monitor_info(unsafe { MonitorFromRect(r, MONITOR_DEFAULTTONEAREST) })
}

/// The whole virtual desktop (every screen) in physical pixels.
pub fn virtual_screen() -> RECT {
    unsafe {
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        RECT {
            left: x,
            top: y,
            right: x + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: y + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

pub fn intersect(a: &RECT, b: &RECT) -> Option<RECT> {
    let r = RECT {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (r.right > r.left && r.bottom > r.top).then_some(r)
}

pub fn same(a: &RECT, b: &RECT) -> bool {
    (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
}

pub fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

/// The user's Pictures folder (OneDrive's when it's redirected there).
pub fn pictures_dir() -> PathBuf {
    unsafe {
        let mut p: *mut u16 = null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_DEFAULT as _, null_mut(), &mut p) >= 0 && !p.is_null() {
            let len = (0..).take_while(|&i| *p.add(i) != 0).count();
            let dir = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
            CoTaskMemFree(p as *const _);
            return dir;
        }
    }
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Pictures")
}

/// Windows' build number (22000 and up is Windows 11).
pub fn windows_build() -> u32 {
    let (key, value) = (wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"), wide("CurrentBuildNumber"));
    let mut buf = [0u16; 32];
    let mut bytes = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if rc != 0 {
        return 0;
    }
    from_wide(&buf).trim().parse().unwrap_or(0)
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The program behind a window.
pub fn exe_of(hwnd: HWND) -> Option<PathBuf> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// A program's own name for itself ("Google Chrome"), from its version info.
fn file_description(exe: &Path) -> Option<String> {
    let path = wide(&exe.display().to_string());
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut ptr: *mut core::ffi::c_void = null_mut();
        let mut len = 0u32;
        let translation = wide(r"\VarFileInfo\Translation");
        let lang = if VerQueryValueW(data.as_ptr().cast(), translation.as_ptr(), &mut ptr, &mut len) != 0 && len >= 4 {
            let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
            format!("{:04x}{:04x}", pair[0], pair[1])
        } else {
            "040904b0".to_owned()
        };
        let key = wide(&format!(r"\StringFileInfo\{lang}\FileDescription"));
        if VerQueryValueW(data.as_ptr().cast(), key.as_ptr(), &mut ptr, &mut len) == 0 || len == 0 {
            return None;
        }
        let text = from_wide(std::slice::from_raw_parts(ptr as *const u16, len as usize));
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }
}

/// A friendly name for the app behind a window: "Google Chrome", "Notepad".
pub fn app_name(hwnd: HWND) -> String {
    let Some(exe) = exe_of(hwnd) else { return "Window".into() };
    let stem = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    // Store apps all run inside one host; their title is the better name.
    if stem.eq_ignore_ascii_case("ApplicationFrameHost") {
        let title = window_title(hwnd);
        return if title.trim().is_empty() { "App".into() } else { title };
    }
    match file_description(&exe) {
        // Some programs describe themselves at length; a short name reads better.
        Some(d) if d.chars().count() <= 40 => d,
        _ => {
            let mut c = stem.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_else(|| "App".into())
        }
    }
}

pub fn open_path(path: &Path) {
    open_url(&path.display().to_string());
}

/// Opens Explorer with the file selected.
pub fn reveal(path: &Path) {
    let (verb, exe, args) = (wide("open"), wide("explorer.exe"), wide(&format!("/select,\"{}\"", path.display())));
    unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), exe.as_ptr(), args.as_ptr(), null(), SW_SHOWNORMAL) };
}
