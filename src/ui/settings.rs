//! The settings window. Every change saves on its own (a quarter second after
//! you stop typing) and the resident picks it up straight away.

use super::theme::{self, ACID, BAD, BG, CARD_HI, FAINT, LINE, MUTED, TEXT, VIOLET};
use super::widgets::{self, card, row, toggle};
use crate::capture::name::{self, Facts, Stamp, TOKENS};
use crate::config::{self, Config, Format, Kind, MAX_NABS, Nab};
use crate::ipc::{self, WM_NAB_QUIT, WM_NAB_RELOAD, WM_NAB_TAKE};
use crate::live::{HotkeyState, Live};
use crate::win;
use crate::{autostart, hotkey};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2,
    vec2,
};
use std::time::{Duration, SystemTime};

const SAVE_DELAY: f64 = 0.25;
const TRY_SECS: f64 = 3.0;

enum Mode {
    Normal,
    RecordingHotkey(usize),
    /// A window nab from "Try it": the resident takes it when the countdown ends.
    Trying {
        until: f64,
    },
}

/// What a nab's card asked for this frame (applied after drawing it).
enum Ask {
    Remove(usize),
    Try(usize),
    Record(usize),
    StopRecording,
    Browse(usize),
}

pub struct Settings {
    cfg: Config,
    saved: Config,
    changed_at: Option<f64>,
    file_stamp: Option<SystemTime>,
    welcome: bool,
    autostart: bool,
    live: Option<&'static Live>,
    mode: Mode,
    resident_up: bool,
    last_poll: f64,
    last_nabs: u32,
    nab_flash: f64,
    note: Option<(String, Color32, f64)>,
}

fn file_stamp() -> Option<SystemTime> {
    std::fs::metadata(config::path()).and_then(|m| m.modified()).ok()
}

impl Settings {
    pub fn new(welcome: bool) -> Self {
        let cfg = Config::load();
        let live = crate::live::open();
        Self {
            saved: cfg.clone(),
            cfg,
            changed_at: None,
            file_stamp: file_stamp(),
            welcome,
            autostart: autostart::is_enabled(),
            live,
            mode: Mode::Normal,
            resident_up: ipc::resident().is_some(),
            last_poll: 0.0,
            last_nabs: live.map_or(0, |l| l.nabs()),
            nab_flash: -10.0,
            note: None,
        }
    }

    fn save_now(&mut self) {
        if self.cfg != self.saved {
            if self.cfg.save().is_ok() {
                self.saved = self.cfg.clone();
                self.file_stamp = file_stamp();
                ipc::send(WM_NAB_RELOAD, 0, 0);
            }
        }
        self.changed_at = None;
    }

    fn note(&mut self, text: impl Into<String>, color: Color32, now: f64) {
        self.note = Some((text.into(), color, now + 4.0));
    }

    /// Once a second: is the resident there, did someone else change the file, any new nabs.
    fn poll(&mut self, now: f64) {
        if let Some(live) = self.live {
            let n = live.nabs();
            if n != self.last_nabs {
                self.last_nabs = n;
                self.nab_flash = now;
            }
        }
        if now - self.last_poll < 1.0 {
            return;
        }
        self.last_poll = now;
        self.resident_up = ipc::resident().is_some();
        let stamp = file_stamp();
        if stamp != self.file_stamp && self.changed_at.is_none() {
            self.file_stamp = stamp;
            self.cfg = Config::load();
            self.saved = self.cfg.clone();
        }
    }

    fn record_hotkey(&mut self, ui: &Ui, now: f64) {
        let Mode::RecordingHotkey(i) = self.mode else { return };
        let events = ui.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            match key {
                egui::Key::Escape => self.mode = Mode::Normal,
                egui::Key::Backspace | egui::Key::Delete => {
                    self.cfg.nabs[i].hotkey.clear();
                    self.changed_at = Some(now);
                    self.mode = Mode::Normal;
                }
                _ => {
                    let mut parts = vec![];
                    if modifiers.ctrl {
                        parts.push("Ctrl");
                    }
                    if modifiers.alt {
                        parts.push("Alt");
                    }
                    if modifiers.shift {
                        parts.push("Shift");
                    }
                    parts.push(key.name());
                    let combo = parts.join("+");
                    if hotkey::parse(&combo).is_some() {
                        self.cfg.nabs[i].hotkey = combo;
                        self.changed_at = Some(now);
                        self.mode = Mode::Normal;
                    } else {
                        self.note("Hold Ctrl or Alt with the key (or use F1 to F24).", BAD, now);
                    }
                }
            }
        }
    }
}

impl eframe::App for Settings {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        #[cfg(feature = "scale-test")]
        super::test_shot::frame(ui.ctx());
        let now = ui.input(|i| i.time);
        self.poll(now);
        self.record_hotkey(ui, now);
        if let Mode::Trying { until } = self.mode {
            if now >= until + 0.6 {
                self.mode = Mode::Normal;
            }
        }

        if ui.input(|i| i.viewport().close_requested()) {
            self.save_now();
        }

        egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ui, |ui| {
            self.header(ui, now);
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 20, right: 20, top: 14, bottom: 18 }).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    if self.welcome {
                        self.welcome_card(ui);
                    }
                    self.status_row(ui, now);
                    let mut asks = Vec::new();
                    for i in 0..self.cfg.nabs.len() {
                        if let Some(ask) = self.nab_card(ui, i, now) {
                            asks.push(ask);
                        }
                    }
                    if self.cfg.nabs.len() < MAX_NABS {
                        ui.horizontal(|ui| {
                            if widgets::ghost(ui, "+  Add a nab").clicked() {
                                self.add_nab(now);
                            }
                            ui.label(
                                RichText::new("Each nab is a hotkey with its own folder and name.")
                                    .small()
                                    .color(FAINT),
                            );
                        });
                    }
                    for ask in asks {
                        self.apply_ask(ask, now);
                    }
                    self.after_card(ui, now);
                    self.behaviour_card(ui, now);
                    self.footer(ui);
                });
            });
        });

        if let Some((text, color, until)) = self.note.clone() {
            if now < until {
                egui::Area::new(egui::Id::unique("nab-note"))
                    .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
                    .show(ui.ctx(), |ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(0x1C, 0x17, 0x29))
                            .stroke(Stroke::new(1.0, color))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(14, 9))
                            .show(ui, |ui| ui.label(RichText::new(text).color(TEXT)));
                    });
            } else {
                self.note = None;
            }
        }

        if let Some(t) = self.changed_at.or((self.cfg != self.saved).then_some(now)) {
            self.changed_at = Some(t);
            if now - t >= SAVE_DELAY {
                self.save_now();
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(60));
            }
        }

        let busy = !matches!(self.mode, Mode::Normal) || now - self.nab_flash < 1.0;
        ui.ctx().request_repaint_after(Duration::from_millis(if busy { 16 } else { 250 }));
    }
}

impl Settings {
    fn header(&mut self, ui: &mut Ui, now: f64) {
        let height = 92.0;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let painter = ui.painter_at(rect);

        let since = (now - self.nab_flash) as f32;
        let pulse = if since < 0.5 { 1.0 - (since - 0.25).abs() / 0.25 } else { 0.0 };
        let logo = Rect::from_center_size(rect.left_center() + vec2(58.0, 0.0), vec2(58.0, 58.0));
        widgets::logo(&painter, logo, pulse.max(0.0));

        let x = logo.right() + 16.0;
        painter.text(pos2(x, rect.center().y - 14.0), egui::Align2::LEFT_CENTER, "Nab", theme::bold(28.0), TEXT);
        painter.text(
            pos2(x + 1.0, rect.center().y + 15.0),
            egui::Align2::LEFT_CENTER,
            format!("v{}  \u{00B7}  donationware by Acid Alchamy", crate::VERSION),
            egui::FontId::proportional(13.0),
            MUTED,
        );
        widgets::brand_rule(&painter, Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom()));

        let button = Rect::from_min_size(pos2(rect.right() - 20.0 - 118.0, rect.center().y - 17.0), vec2(118.0, 34.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(button));
        if widgets::donate(&mut child, "Donate").clicked() {
            win::open_url(crate::DONATE_URL);
        }
    }

    fn welcome_card(&mut self, ui: &mut Ui) {
        let key =
            self.cfg.nabs.iter().find(|n| n.kind == Kind::Window && !n.hotkey.is_empty()).map(|n| n.hotkey.clone());
        let how = match key {
            Some(k) => {
                format!("Press {k} and the window you're in is copied and saved, named after its app and the time.")
            }
            None => "Right-click the tray icon and pick a nab.".to_owned(),
        };
        Frame::new()
            .fill(Color32::from_rgb(0x1A, 0x12, 0x2C))
            .stroke(Stroke::new(1.0, VIOLET.gamma_multiply(0.7)))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Welcome to Nab").font(theme::bold(18.0)).color(TEXT));
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "Nab now lives in your system tray, bottom right (it may be tucked under the ^ arrow). {how} \
                             Paste it anywhere, or find it in Pictures\\Nab. Everything below is optional."
                        ))
                        .color(MUTED),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Nab is donationware: free to use, for good. If it earns its place on your keyboard, a small \
                             donation keeps the lab building more little tools like it.",
                        )
                        .color(TEXT),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::donate(ui, "Donate").clicked() {
                        win::open_url(crate::DONATE_URL);
                    }
                    if widgets::ghost(ui, "Got it").clicked() {
                        self.welcome = false;
                    }
                });
            });
    }

    fn status_row(&mut self, ui: &mut Ui, now: f64) {
        ui.horizontal(|ui| {
            if !self.resident_up {
                widgets::status(ui, BAD, "Nab isn't running");
            } else if self.cfg.paused {
                widgets::status(ui, Color32::from_rgb(0xFF, 0xC4, 0x3A), "Hotkeys paused (the tray menu still works)");
            } else {
                let text = match self.last_nabs {
                    0 => "Ready. Press a hotkey below to nab.".to_owned(),
                    1 => "Ready. One nab so far.".to_owned(),
                    n => format!("Ready. {n} nabs so far."),
                };
                let flash = (1.0 - (now - self.nab_flash) as f32).max(0.0);
                widgets::status(ui, theme::lerp(ACID, Color32::WHITE, flash), &text);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.resident_up {
                    if widgets::primary(ui, "Start Nab").clicked() {
                        self.save_now();
                        win::spawn_self(&[]);
                        self.last_poll = 0.0;
                    }
                } else {
                    let mut on = !self.cfg.paused;
                    if toggle(ui, &mut on).changed() {
                        self.cfg.paused = !on;
                        self.changed_at = Some(now - SAVE_DELAY);
                    }
                    ui.label(RichText::new("Hotkeys").color(MUTED));
                }
            });
        });
    }

    fn example(&self, nab: &Nab) -> String {
        let (app, title) = match nab.kind {
            Kind::Window => ("Google Chrome", "Inbox"),
            Kind::Area => ("Area", ""),
            Kind::Screen => ("Screen", ""),
            Kind::AllScreens => ("Desktop", ""),
        };
        let facts = Facts { app, title, nab: &nab.name, w: 1280, h: 720, when: Stamp::now() };
        name::preview(&nab.pattern, &facts, nab.format.ext())
    }

    fn nab_card(&mut self, ui: &mut Ui, i: usize, now: f64) -> Option<Ask> {
        let mut nab = self.cfg.nabs[i].clone();
        let count = self.cfg.nabs.len();
        let duplicate = (!nab.hotkey.is_empty())
            .then(|| self.cfg.nabs.iter().position(|n| n.hotkey.eq_ignore_ascii_case(&nab.hotkey)))
            .flatten()
            .filter(|&j| j != i);
        let state = self.live.map_or(HotkeyState::Off, |l| l.hotkey(i));
        let recording = matches!(self.mode, Mode::RecordingHotkey(j) if j == i);
        let trying = match self.mode {
            Mode::Trying { until } => Some(until),
            _ => None,
        };
        let example = self.example(&nab);
        let mut ask = None;

        card(ui, &format!("Nab {}  \u{00B7}  {}", i + 1, nab.kind.label()), |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut nab.name)
                        .font(theme::semibold(16.0))
                        .desired_width(220.0)
                        .hint_text("Name this nab"),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if count > 1 && widgets::link(ui, "Remove").clicked() {
                        ask = Some(Ask::Remove(i));
                    }
                    let label = match trying {
                        Some(until) if nab.kind == Kind::Window => {
                            format!("Click into a window\u{2026} {:.0}", (until - now).ceil().max(0.0))
                        }
                        _ => "\u{25B6}  Try it".to_owned(),
                    };
                    let hint = match nab.kind {
                        Kind::Window => "Click into the window you want within 3 seconds",
                        Kind::Area => "Shows the area picker now (Esc cancels)",
                        _ => "Takes this nab now",
                    };
                    let enabled = self.resident_up && trying.is_none();
                    if ui.add_enabled(enabled, egui::Button::new(label)).on_hover_text(hint).clicked() {
                        ask = Some(Ask::Try(i));
                    }
                });
            });

            let mut kind = nab.kind;
            let options: Vec<(Kind, &str)> = Kind::ALL.iter().map(|&k| (k, k.label())).collect();
            if widgets::segmented(ui, &mut kind, &options) {
                if nab.name == nab.kind.label() {
                    nab.name = kind.label().into();
                }
                nab.kind = kind;
            }
            ui.label(RichText::new(kind_hint(nab.kind)).small().color(FAINT));

            row(ui, "Hotkey", "", |ui| {
                if !nab.hotkey.is_empty() && !recording && widgets::ghost(ui, "Clear").clicked() {
                    nab.hotkey.clear();
                }
                if widgets::ghost(ui, if recording { "Cancel" } else { "Set" }).clicked() {
                    ask = Some(if recording { Ask::StopRecording } else { Ask::Record(i) });
                }
                if recording {
                    ui.label(RichText::new("Press the keys\u{2026}").color(ACID));
                } else if nab.hotkey.is_empty() {
                    ui.label(RichText::new("None (tray menu only)").color(FAINT));
                } else {
                    if let Some(j) = duplicate {
                        ui.label(RichText::new(format!("also nab {}", j + 1)).small().color(BAD));
                    } else {
                        match state {
                            HotkeyState::Taken => {
                                ui.label(RichText::new("taken by another app").small().color(BAD));
                            }
                            HotkeyState::Invalid => {
                                ui.label(RichText::new("not a usable key").small().color(BAD));
                            }
                            _ => {}
                        }
                    }
                    ui.horizontal(|ui| widgets::keycaps(ui, &nab.hotkey));
                }
            });

            row(ui, "Save a file", "Off: it only goes on the clipboard", |ui| {
                toggle(ui, &mut nab.save);
            });
            row(ui, "Copy to the clipboard", "Paste it straight into a chat or a doc", |ui| {
                toggle(ui, &mut nab.copy);
            });

            ui.add_enabled_ui(nab.save, |ui| {
                let folder = nab.folder_path();
                row(ui, "Folder", "", |ui| {
                    if widgets::ghost(ui, "Open").clicked() {
                        let _ = std::fs::create_dir_all(&folder);
                        win::open_path(&folder);
                    }
                    if widgets::ghost(ui, "Change\u{2026}").clicked() {
                        ask = Some(Ask::Browse(i));
                    }
                    let shown = folder.display().to_string();
                    ui.add(egui::Label::new(RichText::new(shown.clone()).color(TEXT)).truncate()).on_hover_text(shown);
                });
                row(ui, "File type", "", |ui| {
                    widgets::segmented(ui, &mut nab.format, &[(Format::Png, "PNG"), (Format::Jpg, "JPG")]);
                });
                ui.label(RichText::new("File name").color(TEXT));
                ui.add(
                    egui::TextEdit::singleline(&mut nab.pattern)
                        .font(egui::FontId::monospace(13.5))
                        .desired_width(f32::INFINITY)
                        .hint_text(name::DEFAULT_PATTERN),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (token, what) in TOKENS {
                        let chip =
                            egui::Button::new(RichText::new(token).font(egui::FontId::monospace(12.0)).color(ACID))
                                .fill(CARD_HI)
                                .stroke(Stroke::new(1.0, LINE))
                                .corner_radius(CornerRadius::same(12));
                        if ui.add(chip).on_hover_text(what).clicked() {
                            if !nab.pattern.is_empty() && !nab.pattern.ends_with([' ', '/', '\\', '-', '_']) {
                                nab.pattern.push(' ');
                            }
                            nab.pattern.push_str(token);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("\u{2192}").color(FAINT));
                    ui.add(egui::Label::new(RichText::new(example).color(MUTED)).truncate());
                });
            });
        });

        if nab != self.cfg.nabs[i] {
            self.cfg.nabs[i] = nab;
            self.changed_at = Some(now);
        }
        ask
    }

    fn apply_ask(&mut self, ask: Ask, now: f64) {
        match ask {
            Ask::Remove(i) => {
                if self.cfg.nabs.len() > 1 && i < self.cfg.nabs.len() {
                    let gone = self.cfg.nabs.remove(i);
                    self.changed_at = Some(now);
                    self.mode = Mode::Normal;
                    self.note(format!("Removed \u{201C}{}\u{201D}.", gone.name), MUTED, now);
                }
            }
            Ask::Try(i) => {
                self.save_now();
                let window = self.cfg.nabs.get(i).is_some_and(|n| n.kind == Kind::Window);
                if window {
                    ipc::send(WM_NAB_TAKE, i, (TRY_SECS * 1000.0) as isize);
                    self.mode = Mode::Trying { until: now + TRY_SECS };
                } else {
                    ipc::send(WM_NAB_TAKE, i, 0);
                }
            }
            Ask::Record(i) => self.mode = Mode::RecordingHotkey(i),
            Ask::StopRecording => self.mode = Mode::Normal,
            Ask::Browse(i) => {
                let current = self.cfg.nabs[i].folder_path();
                let picked = rfd::FileDialog::new()
                    .set_title("Where should this nab's pictures go?")
                    .set_directory(&current)
                    .pick_folder();
                if let Some(dir) = picked {
                    self.cfg.nabs[i].folder = dir.display().to_string();
                    self.changed_at = Some(now);
                }
            }
        }
    }

    fn add_nab(&mut self, now: f64) {
        let kind = Kind::ALL.into_iter().find(|k| !self.cfg.nabs.iter().any(|n| n.kind == *k)).unwrap_or(Kind::Window);
        let mut nab = Nab::new(kind, "");
        let taken = |name: &str, nabs: &[Nab]| nabs.iter().any(|n| n.name.eq_ignore_ascii_case(name));
        let mut k = 2;
        while taken(&nab.name, &self.cfg.nabs) {
            nab.name = format!("{} {k}", kind.label());
            k += 1;
        }
        self.cfg.nabs.push(nab);
        self.changed_at = Some(now);
    }

    fn after_card(&mut self, ui: &mut Ui, now: f64) {
        card(ui, "After each nab", |ui| {
            let flag = |ui: &mut Ui, value: &mut bool, label: &str, hint: &str| {
                let mut changed = false;
                row(ui, label, hint, |ui| changed = toggle(ui, value).changed());
                changed
            };
            let mut changed = false;
            changed |= flag(
                ui,
                &mut self.cfg.card,
                "Preview card",
                "In the corner for a few seconds: open it or show it in its folder",
            );
            changed |= flag(ui, &mut self.cfg.flash, "Flash", "A quick white flash over what was nabbed");
            changed |= flag(ui, &mut self.cfg.sound, "Shutter sound", "Two soft clicks");
            changed |= flag(ui, &mut self.cfg.pointer, "Include the mouse pointer", "In window and screen nabs");
            changed |= flag(
                ui,
                &mut self.cfg.corners,
                "Clean rounded corners",
                "Windows 11: the bits outside a window's round corners turn transparent",
            );
            if self.cfg.nabs.iter().any(|n| n.format == Format::Jpg) {
                row(ui, "JPG quality", "Higher is sharper and bigger", |ui| {
                    ui.label(RichText::new(self.cfg.jpg_quality.to_string()).font(theme::semibold(14.0)).color(ACID));
                    if ui.add(egui::Slider::new(&mut self.cfg.jpg_quality, 50..=100).show_value(false)).changed() {
                        changed = true;
                    }
                });
            }
            if changed {
                self.changed_at = Some(now);
            }
        });
    }

    fn behaviour_card(&mut self, ui: &mut Ui, now: f64) {
        card(ui, "Behavior", |ui| {
            let mut auto = self.autostart;
            row(ui, "Start with Windows", "Quietly, in the tray", |ui| {
                if toggle(ui, &mut auto).changed() {
                    autostart::set(auto);
                    self.autostart = autostart::is_enabled();
                }
            });
            let mut hello = self.cfg.hello_card;
            row(ui, "Hello card at start", "The little donationware note", |ui| {
                if toggle(ui, &mut hello).changed() {
                    self.cfg.hello_card = hello;
                    self.changed_at = Some(now);
                }
            });
            row(
                ui,
                "Stream Deck, shortcuts, scripts",
                "Nab.exe --nab 1 takes nab 1 (or window, area, screen, all)",
                |ui| {
                    if widgets::ghost(ui, "Copy the command").clicked() {
                        let exe = std::env::current_exe()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "Nab.exe".into());
                        ui.ctx().copy_text(format!("\"{exe}\" --nab 1"));
                        self.note(
                            "Copied. Paste it into a Stream Deck \u{201C}Open\u{201D} action or a shortcut.",
                            ACID,
                            now,
                        );
                    }
                },
            );
        });
    }

    fn footer(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let links_w = 420.0;
                ui.add_space(((ui.available_width() - links_w) / 2.0).max(0.0));
                if widgets::link(ui, "acidalchamy.com/donationware").clicked() {
                    win::open_url(crate::HOME_URL);
                }
                if widgets::link(ui, "Source code").clicked() {
                    win::open_url(crate::REPO_URL);
                }
                if widgets::link(ui, "Settings file").clicked() {
                    win::open_url(&config::dir().display().to_string());
                }
                if self.resident_up && widgets::link(ui, "Quit Nab").clicked() {
                    self.save_now();
                    ipc::send(WM_NAB_QUIT, 0, 0);
                    self.last_poll = 0.0;
                }
            });
            ui.label(
                RichText::new(format!(
                    "Nab {}  \u{00B7}  MIT License  \u{00B7}  Donationware by Acid Alchamy  \u{00B7}  changes save as you go",
                    crate::VERSION
                ))
                .small()
                .color(FAINT),
            );
        });
    }
}

fn kind_hint(kind: Kind) -> &'static str {
    match kind {
        Kind::Window => "The window you're in, edge to edge: no desktop around it, no invisible border.",
        Kind::Area => "The screen freezes: drag out an area, or click a window. A loupe helps you hit the exact pixel.",
        Kind::Screen => "The whole screen the pointer is on.",
        Kind::AllScreens => "Every screen at once, in one picture.",
    }
}
