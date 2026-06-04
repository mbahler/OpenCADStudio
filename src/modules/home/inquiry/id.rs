// ID command — report coordinates of a picked point.

use glam::Vec3;

use crate::command::{CadCommand, CmdResult};

pub struct IdCommand;

impl IdCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CadCommand for IdCommand {
    fn name(&self) -> &'static str {
        "ID"
    }

    fn prompt(&self) -> String {
        "ID  Specify point:".into()
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        // Drawing plane is world XY (z = elevation).
        let x = pt.x;
        let y = pt.y;
        let z = pt.z;
        let msg = format!("X = {x:.4},  Y = {y:.4},  Z = {z:.4}");
        CmdResult::Measurement(msg)
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }
}


// ── Autocomplete registry ─────────────────────────────────
inventory::submit!(crate::command::CommandRegistration { names: &["ID"] });  // IdCommand
