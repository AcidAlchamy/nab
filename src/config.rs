//! Settings, stored as TOML in %APPDATA%\Nab\settings.toml.
//!
//! The resident reads this file at start and whenever the settings window says
//! it changed; the settings window writes it. Unknown or missing keys fall back
//! to the defaults, so old files keep working.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The most nabs (hotkey + folder combinations) you can set up.
pub const MAX_NABS: usize = 12;

/// What a nab captures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The window you're in, edge to edge (no desktop around it).
    Window,
    /// An area you drag out, or a window you click.
    Area,
    /// The whole screen the pointer is on.
    Screen,
    /// Every screen at once.
    AllScreens,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Window, Kind::Area, Kind::Screen, Kind::AllScreens];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Window => "Window",
            Kind::Area => "Area",
            Kind::Screen => "Screen",
            Kind::AllScreens => "All screens",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpg,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpg => "jpg",
        }
    }
}

/// One hotkey and where its pictures go.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Nab {
    pub name: String,
    pub kind: Kind,
    /// Like "Ctrl+Shift+Q"; empty = no hotkey (the tray menu still has it).
    pub hotkey: String,
    /// Where files go. Empty = Pictures\Nab.
    pub folder: String,
    /// File name, see `capture::name`. Slashes make subfolders.
    pub pattern: String,
    pub format: Format,
    /// Save a file.
    pub save: bool,
    /// Put it on the clipboard.
    pub copy: bool,
}

impl Default for Nab {
    fn default() -> Self {
        Self {
            name: "Window".into(),
            kind: Kind::Window,
            hotkey: String::new(),
            folder: String::new(),
            pattern: crate::capture::name::DEFAULT_PATTERN.into(),
            format: Format::Png,
            save: true,
            copy: true,
        }
    }
}

impl Nab {
    pub fn new(kind: Kind, hotkey: &str) -> Self {
        Self { name: kind.label().into(), kind, hotkey: hotkey.into(), ..Default::default() }
    }

    pub fn folder_path(&self) -> PathBuf {
        let f = self.folder.trim();
        if f.is_empty() { default_folder() } else { PathBuf::from(f) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Hotkeys do nothing while paused (the tray menu still works).
    pub paused: bool,
    /// The preview card in the corner after each nab.
    pub card: bool,
    /// A quick white flash over what was captured.
    pub flash: bool,
    /// A soft shutter click.
    pub sound: bool,
    /// Draw the mouse pointer into window and screen nabs.
    pub pointer: bool,
    /// On Windows 11, make the corners outside a window's rounded edge
    /// transparent instead of keeping whatever was behind them.
    pub corners: bool,
    /// JPEG quality, 50 to 100.
    pub jpg_quality: u8,
    /// Show the donationware hello card when Nab starts.
    pub hello_card: bool,
    /// The nabs, in the order they're listed (and numbered for `--nab N`).
    pub nabs: Vec<Nab>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            paused: false,
            card: true,
            flash: true,
            sound: false,
            pointer: false,
            corners: true,
            jpg_quality: 90,
            hello_card: true,
            nabs: vec![
                Nab::new(Kind::Window, "Ctrl+Shift+Q"),
                Nab::new(Kind::Area, "Ctrl+Shift+A"),
                Nab::new(Kind::Screen, ""),
            ],
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg: Config =
            std::fs::read_to_string(path()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
        cfg.nabs.truncate(MAX_NABS);
        if cfg.nabs.is_empty() {
            cfg.nabs = Config::default().nabs;
        }
        cfg.jpg_quality = cfg.jpg_quality.clamp(50, 100);
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(
            &tmp,
            format!("# Nab settings. Edit in the app: right-click the tray icon, Settings.\n\n{text}"),
        )?;
        std::fs::rename(tmp, path)
    }

    /// The first nab of a kind, or a stand-in with that kind if there's none.
    pub fn nab_of_kind(&self, kind: Kind) -> Nab {
        self.nabs.iter().find(|n| n.kind == kind).cloned().unwrap_or_else(|| {
            let mut n = self.nabs.first().cloned().unwrap_or_default();
            n.kind = kind;
            n.name = kind.label().into();
            n
        })
    }
}

/// Pictures\Nab (wherever Pictures really is, OneDrive included).
pub fn default_folder() -> PathBuf {
    crate::win::pictures_dir().join("Nab")
}

pub fn dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Nab")
}

pub fn path() -> PathBuf {
    dir().join("settings.toml")
}

pub fn exists() -> bool {
    path().exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip_and_partial_files() {
        let mut c = Config::default();
        c.nabs[1].folder = r"D:\Shots\Work".into();
        c.nabs[1].format = Format::Jpg;
        let text = toml::to_string_pretty(&c).unwrap();
        assert!(text.contains("[[nabs]]"));
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, c);

        let partial: Config =
            toml::from_str("sound = true\n[[nabs]]\nkind = \"area\"\nhotkey = \"Ctrl+Alt+S\"").unwrap();
        assert!(partial.sound);
        assert_eq!(partial.nabs.len(), 1);
        assert_eq!(partial.nabs[0].kind, Kind::Area);
        assert_eq!(partial.nabs[0].pattern, crate::capture::name::DEFAULT_PATTERN);
        assert!(partial.nabs[0].copy && partial.nabs[0].save);
    }

    #[test]
    fn kind_lookup_falls_back() {
        let mut c = Config::default();
        c.nabs.retain(|n| n.kind != Kind::AllScreens);
        let n = c.nab_of_kind(Kind::AllScreens);
        assert_eq!(n.kind, Kind::AllScreens);
        assert_eq!(c.nab_of_kind(Kind::Area).hotkey, "Ctrl+Shift+A");
    }
}
