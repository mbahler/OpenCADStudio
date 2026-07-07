// PEDIT command — edit a polyline entity.
//
// Supports LwPolyline and (partially) Polyline2D.
// Subcommands (text input after entity pick):
//   C / CLOSE   — toggle closed flag
//   O / OPEN    — clear closed flag
//   W <width>   — set uniform width (LwPolyline)
//   E           — enter vertex editing mode (not implemented, use grips)
//   J           — join (same as JOIN command, triggered as alias)
//   X / EXIT    — exit

use acadrust::{EntityType, Handle};
use glam::DVec3;

use crate::command::{CadCommand, CmdResult};

pub struct PeditCommand {
    target: Option<Handle>,
    /// Set once the bare `W` keyword (or the Width button) is chosen: the next
    /// text input is parsed as the new width instead of an option. (#304)
    awaiting_width: bool,
}

impl PeditCommand {
    pub fn new() -> Self {
        Self {
            target: None,
            awaiting_width: false,
        }
    }
}

impl CadCommand for PeditCommand {
    fn name(&self) -> &'static str {
        "PEDIT"
    }

    fn prompt(&self) -> String {
        if self.target.is_none() {
            "PEDIT  Select polyline:".into()
        } else if self.awaiting_width {
            "PEDIT  Specify new width:".into()
        } else {
            "PEDIT  Enter option:".into()
        }
    }

    fn options(&self) -> Vec<crate::command::CmdOption> {
        use crate::command::CmdOption;
        if self.target.is_none() || self.awaiting_width {
            vec![]
        } else {
            vec![
                CmdOption::new("Close", "C"),
                CmdOption::new("Open", "O"),
                CmdOption::new("Width", "W"),
                CmdOption::new("eXit", "X"),
            ]
        }
    }

    fn needs_entity_pick(&self) -> bool {
        self.target.is_none()
    }

    fn on_entity_pick(&mut self, handle: Handle, _pt: DVec3) -> CmdResult {
        if handle.is_null() {
            return CmdResult::NeedPoint;
        }
        self.target = Some(handle);
        CmdResult::NeedPoint
    }

    fn wants_text_input(&self) -> bool {
        self.target.is_some()
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        let handle = self.target?;
        let up = text.trim().to_uppercase();

        // Width sub-step: the previous input was a bare `W` (typed or via the
        // Width button), so this input is the width value. (#304)
        if self.awaiting_width {
            let w: f64 = up
                .replace(',', ".")
                .parse()
                .ok()
                .filter(|&v: &f64| v >= 0.0)?;
            self.awaiting_width = false;
            return Some(CmdResult::PeditOp {
                handle,
                op: PeditOp::SetWidth(w),
            });
        }

        match up.as_str() {
            "X" | "EXIT" => return Some(CmdResult::Cancel),
            "C" | "CLOSE" => {
                return Some(CmdResult::PeditOp {
                    handle,
                    op: PeditOp::SetClosed(true),
                })
            }
            "O" | "OPEN" => {
                return Some(CmdResult::PeditOp {
                    handle,
                    op: PeditOp::SetClosed(false),
                })
            }
            "W" | "WIDTH" => {
                // Bare width keyword (the Width button): prompt for the value
                // rather than doing nothing.
                self.awaiting_width = true;
                return Some(CmdResult::NeedPoint);
            }
            _ => {}
        }

        // Inline shorthand `W <value>`.
        if let Some(rest) = up.strip_prefix("W ") {
            let w: f64 = rest
                .trim()
                .replace(',', ".")
                .parse()
                .ok()
                .filter(|&v: &f64| v >= 0.0)?;
            return Some(CmdResult::PeditOp {
                handle,
                op: PeditOp::SetWidth(w),
            });
        }

        None
    }

    fn on_point(&mut self, _pt: DVec3) -> CmdResult {
        CmdResult::NeedPoint
    }
    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }
}

// ── Op enum (used in CmdResult) ────────────────────────────────────────────

#[derive(Clone)]
pub enum PeditOp {
    SetClosed(bool),
    SetWidth(f64),
}

// ── Apply logic ────────────────────────────────────────────────────────────

pub fn apply_pedit(entity: &mut EntityType, op: &PeditOp) -> bool {
    match op {
        PeditOp::SetClosed(closed) => match entity {
            EntityType::LwPolyline(p) => {
                p.is_closed = *closed;
                true
            }
            EntityType::Polyline2D(p) => {
                if *closed {
                    p.close();
                } else {
                    p.flags.set_closed(false);
                }
                true
            }
            _ => false,
        },
        PeditOp::SetWidth(w) => match entity {
            EntityType::LwPolyline(p) => {
                p.constant_width = *w;
                for v in &mut p.vertices {
                    v.start_width = *w;
                    v.end_width = *w;
                }
                true
            }
            _ => false,
        },
    }
}


// ── Autocomplete registry ─────────────────────────────────
inventory::submit!(crate::command::CommandRegistration { names: &["PE", "PEDIT"] });  // PeditCommand
