//! File names from a pattern like "{app} {date} {time}", so a folder of nabs
//! reads like a list: "Google Chrome 2026-10-29 21-14-03.png".
//!
//! Slashes in the pattern make subfolders ("{app}/{date} {time}" sorts nabs
//! into a folder per app). Anything Windows won't allow in a name is replaced.

use std::path::{Path, PathBuf};

pub const DEFAULT_PATTERN: &str = "{app} {date} {time}";

/// The tokens a pattern can use, with what each one turns into.
pub const TOKENS: [(&str, &str); 7] = [
    ("{app}", "the app's name"),
    ("{title}", "the window's title"),
    ("{date}", "2026-10-29"),
    ("{time}", "21-14-03"),
    ("{n}", "1, 2, 3\u{2026} the next free number"),
    ("{nab}", "this nab's name"),
    ("{size}", "1280x720"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

impl Stamp {
    pub fn now() -> Self {
        let mut t = windows_sys::Win32::Foundation::SYSTEMTIME::default();
        unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
        Self { year: t.wYear, month: t.wMonth, day: t.wDay, hour: t.wHour, minute: t.wMinute, second: t.wSecond }
    }
}

/// What the tokens are filled with.
pub struct Facts<'a> {
    pub app: &'a str,
    pub title: &'a str,
    pub nab: &'a str,
    pub w: i32,
    pub h: i32,
    pub when: Stamp,
}

/// Characters a Windows file name can't have, and control characters.
fn bad_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
}

/// A token's value, safe to sit inside a name.
fn clean_value(s: &str, max: usize) -> String {
    let s: String = s.chars().map(|c| if bad_char(c) { ' ' } else { c }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = s.chars().take(max).collect();
    if out.chars().count() < s.chars().count() {
        out = out.trim_end().to_owned();
    }
    out
}

/// One folder or file name: no trailing dots or spaces, no device names.
fn clean_segment(s: &str) -> String {
    let s: String = s.chars().map(|c| if bad_char(c) { ' ' } else { c }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_end_matches(['.', ' ']).trim_start().to_owned();
    let s: String = s.chars().take(150).collect();
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    const DEVICES: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
        "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if DEVICES.contains(&stem.as_str()) { format!("_{s}") } else { s }
}

/// The pattern filled in: relative folder segments, the last one the file stem.
fn fill(pattern: &str, f: &Facts, n: u32) -> Vec<String> {
    let t = f.when;
    let date = format!("{:04}-{:02}-{:02}", t.year, t.month, t.day);
    let time = format!("{:02}-{:02}-{:02}", t.hour, t.minute, t.second);
    let size = format!("{}x{}", f.w, f.h);
    let app = clean_value(f.app, 60);
    let title = clean_value(f.title, 80);
    let nab = clean_value(f.nab, 40);
    let segments: Vec<String> = pattern
        .split(['/', '\\'])
        .map(|seg| {
            let filled = seg
                .replace("{app}", &app)
                .replace("{title}", &title)
                .replace("{date}", &date)
                .replace("{time}", &time)
                .replace("{nab}", &nab)
                .replace("{size}", &size)
                .replace("{n}", &n.to_string());
            clean_segment(&filled)
        })
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() { vec![clean_segment(&format!("Nab {date} {time}"))] } else { segments }
}

fn join(dir: &Path, segments: &[String], suffix: &str, ext: &str) -> PathBuf {
    let mut p = dir.to_path_buf();
    let (file, folders) = segments.split_last().expect("fill never returns an empty list");
    for s in folders {
        p.push(s);
    }
    p.push(format!("{file}{suffix}.{ext}"));
    p
}

/// A free file name in `dir` for this nab. With {n} in the pattern, the
/// lowest number not taken yet; otherwise " (2)", " (3)"... if the name exists.
pub fn path_for(dir: &Path, pattern: &str, f: &Facts, ext: &str) -> PathBuf {
    if pattern.contains("{n}") {
        for n in 1..100_000 {
            let p = join(dir, &fill(pattern, f, n), "", ext);
            if !p.exists() {
                return p;
            }
        }
    }
    let segments = fill(pattern, f, 1);
    let first = join(dir, &segments, "", ext);
    if !first.exists() {
        return first;
    }
    (2..10_000).map(|i| join(dir, &segments, &format!(" ({i})"), ext)).find(|p| !p.exists()).unwrap_or(first)
}

/// What the pattern gives right now, for the settings window: "Google Chrome 2026-10-29 21-14-03.png".
pub fn preview(pattern: &str, f: &Facts, ext: &str) -> String {
    let segments = fill(pattern, f, 1);
    format!("{}.{ext}", segments.join("\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts<'static> {
        Facts {
            app: "Google Chrome",
            title: "Inbox (3) - mail: \"work\" | Chrome",
            nab: "Window",
            w: 1280,
            h: 720,
            when: Stamp { year: 2026, month: 10, day: 9, hour: 21, minute: 4, second: 3 },
        }
    }

    #[test]
    fn default_pattern_reads_like_a_list() {
        assert_eq!(preview(DEFAULT_PATTERN, &facts(), "png"), "Google Chrome 2026-10-09 21-04-03.png");
    }

    #[test]
    fn titles_lose_characters_windows_refuses() {
        assert_eq!(preview("{title}", &facts(), "png"), "Inbox (3) - mail work Chrome.png");
    }

    #[test]
    fn slashes_make_folders_and_empty_bits_vanish() {
        assert_eq!(
            preview("{app}/{date}/{time} {size}", &facts(), "jpg"),
            r"Google Chrome\2026-10-09\21-04-03 1280x720.jpg"
        );
        assert_eq!(preview("//{nab}//", &facts(), "png"), "Window.png");
        assert_eq!(preview("", &facts(), "png"), "Nab 2026-10-09 21-04-03.png");
        assert_eq!(preview("con", &facts(), "png"), "_con.png");
        assert_eq!(preview("shot...  ", &facts(), "png"), "shot.png");
    }

    #[test]
    fn names_never_collide() {
        let dir = std::env::temp_dir().join(format!("nab-name-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = facts();
        let a = path_for(&dir, "shot", &f, "png");
        std::fs::write(&a, b"x").unwrap();
        let b = path_for(&dir, "shot", &f, "png");
        assert_eq!(b.file_name().unwrap(), "shot (2).png");

        let n1 = path_for(&dir, "pic {n}", &f, "png");
        assert_eq!(n1.file_name().unwrap(), "pic 1.png");
        std::fs::write(&n1, b"x").unwrap();
        assert_eq!(path_for(&dir, "pic {n}", &f, "png").file_name().unwrap(), "pic 2.png");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
