//! Status-bar customization — which toggle pills are shown on the bar.
//!
//! The customization menu (opened from the bar's far-right handle) lists every
//! pill with a check mark next to the ones currently shown. Toggling a row
//! adds or removes that pill from the bar. The choice is persisted so it
//! survives across sessions.

use rustc_hash::FxHashSet as HashSet;
use std::path::PathBuf;

/// Identifies a toggleable status-bar pill.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StatusPill {
    Coords,
    Ortho,
    Lwt,
    Polar,
    Dyn,
    Otrack,
    Osnap,
    Space,
    Scale,
    Units,
    Transparency,
    Isolate,
    QuickProps,
    SelFilter,
    SelCycle,
    Vp,
    CleanScreen,
}

impl StatusPill {
    /// Every pill, in status-bar display order. Drives both the bar layout and
    /// the customization menu.
    pub const ALL: &'static [StatusPill] = &[
        StatusPill::Coords,
        StatusPill::Ortho,
        StatusPill::Lwt,
        StatusPill::Polar,
        StatusPill::Dyn,
        StatusPill::Otrack,
        StatusPill::Osnap,
        StatusPill::Space,
        StatusPill::Scale,
        StatusPill::Units,
        StatusPill::Transparency,
        StatusPill::Isolate,
        StatusPill::QuickProps,
        StatusPill::SelFilter,
        StatusPill::SelCycle,
        StatusPill::Vp,
        StatusPill::CleanScreen,
    ];

    /// Stable identifier used for persistence.
    pub fn id(self) -> &'static str {
        match self {
            StatusPill::Coords => "coords",
            StatusPill::Ortho => "ortho",
            StatusPill::Lwt => "lwt",
            StatusPill::Polar => "polar",
            StatusPill::Dyn => "dyn",
            StatusPill::Otrack => "otrack",
            StatusPill::Osnap => "osnap",
            StatusPill::Space => "space",
            StatusPill::Scale => "scale",
            StatusPill::Units => "units",
            StatusPill::Transparency => "transparency",
            StatusPill::Isolate => "isolate",
            StatusPill::QuickProps => "quickprops",
            StatusPill::SelFilter => "selfilter",
            StatusPill::SelCycle => "selcycle",
            StatusPill::Vp => "vp",
            StatusPill::CleanScreen => "cleanscreen",
        }
    }

    /// Label shown in the customization menu.
    pub fn label(self) -> &'static str {
        match self {
            StatusPill::Coords => "Coordinates",
            StatusPill::Ortho => "Ortho Mode",
            StatusPill::Lwt => "Show Lineweight",
            StatusPill::Polar => "Polar Tracking",
            StatusPill::Dyn => "Dynamic Input",
            StatusPill::Otrack => "Object Snap Tracking",
            StatusPill::Osnap => "Object Snap",
            StatusPill::Space => "Model/Paper Space",
            StatusPill::Scale => "Annotation Scale",
            StatusPill::Units => "Drawing Units",
            StatusPill::Transparency => "Show Transparency",
            StatusPill::Isolate => "Isolate Objects",
            StatusPill::QuickProps => "Quick Properties",
            StatusPill::SelFilter => "Selection Filtering",
            StatusPill::SelCycle => "Selection Cycling",
            StatusPill::Vp => "Viewport Count",
            StatusPill::CleanScreen => "Clean Screen",
        }
    }

    fn from_id(s: &str) -> Option<StatusPill> {
        StatusPill::ALL.iter().copied().find(|p| p.id() == s)
    }
}

/// Tracks which pills the user has hidden.
#[derive(Clone)]
pub struct StatusBarConfig {
    hidden: HashSet<StatusPill>,
}

impl Default for StatusBarConfig {
    /// The out-of-the-box bar: a few informational / niche pills are hidden by
    /// default to keep it uncluttered, and the user turns them on from the
    /// customization (⚙) menu if wanted. The mode toggles that matter most
    /// (Ortho, Polar, Otrack, Osnap, …) stay visible.
    fn default() -> Self {
        let hidden = [
            StatusPill::Coords,
            StatusPill::Lwt,
            StatusPill::Dyn,
            StatusPill::Space,
            StatusPill::Units,
            StatusPill::Transparency,
            StatusPill::SelCycle,
            StatusPill::Vp,
        ]
        .into_iter()
        .collect();
        Self { hidden }
    }
}

impl StatusBarConfig {
    /// Load the saved customization. When no config file exists yet, fall back
    /// to the shipped defaults ([`StatusBarConfig::default`]); an existing file
    /// (even an empty one — the user showed every pill) is authoritative.
    pub fn load() -> Self {
        match config_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(body) => {
                let hidden = body
                    .lines()
                    .filter_map(|l| StatusPill::from_id(l.trim()))
                    .collect();
                Self { hidden }
            }
            None => Self::default(),
        }
    }

    pub fn is_visible(&self, pill: StatusPill) -> bool {
        !self.hidden.contains(&pill)
    }

    /// Flip a pill's visibility and persist the change.
    pub fn toggle(&mut self, pill: StatusPill) {
        if !self.hidden.remove(&pill) {
            self.hidden.insert(pill);
        }
        self.save();
    }

    fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let body: String = StatusPill::ALL
            .iter()
            .filter(|p| self.hidden.contains(p))
            .map(|p| p.id())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(path, body);
    }
}

/// `<config-dir>/OpenCADStudio/statusbar.txt`, matching the recent-files store.
fn config_path() -> Option<PathBuf> {
    Some(crate::config::config_dir()?.join("statusbar.txt"))
}
