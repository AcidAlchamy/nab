//! Nab: one key grabs the window you're in. It lands on the clipboard and in
//! the folder you picked, with a name that says what it is.
//!
//! One small exe with four faces:
//!   Nab.exe               the resident: tray icon, hotkeys, captures
//!                         (plain Win32, asleep until you press a hotkey)
//!   Nab.exe --settings    the settings window (only alive while it's open)
//!   Nab.exe --hello       the donationware hello card shown at start
//!   Nab.exe --nab <what>  asks the running Nab for a nab, for Stream Deck
//!                         buttons, shortcuts and scripts: a nab's number from
//!                         the settings, or window / area / screen / all
//!
//! Nab is donationware by Acid Alchamy: free to use, and if it helps, please
//! consider a donation at acidalchamy.com/donationware.

#![windows_subsystem = "windows"]

mod autostart;
mod capture;
mod config;
mod hotkey;
mod ipc;
mod live;
mod paint;
mod resident;
mod ui;
mod win;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DONATE_URL: &str = "https://acidalchamy.com/donationware/nab/?from=app#donate";
pub const HOME_URL: &str = "https://acidalchamy.com/donationware/nab/";
pub const REPO_URL: &str = "https://github.com/AcidAlchamy/nab";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if has("--settings") {
        ui::run_settings(has("--welcome"));
    } else if has("--hello") {
        ui::run_hello();
    } else if let Some(what) = value("--nab") {
        ipc::request_nab(&what);
    } else {
        resident::run(has("--autostart"), has("--no-hello"));
    }
}
