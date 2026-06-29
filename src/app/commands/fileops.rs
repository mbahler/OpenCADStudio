use super::*;

impl OpenCADStudio {
    pub(super) fn dispatch_fileops(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
        match cmd {
            "NEW" => return Some(Task::done(Message::TabNew)),
            "OPEN" => return Some(Task::done(Message::OpenFile)),
            "SAVE" | "QSAVE" => return Some(Task::done(Message::SaveFile)),
            // SAVEALL — write every open drawing that already has a file path.
            // Tabs without a path (never saved) are skipped with a note.
            "SAVEALL" => {
                if self.read_only {
                    self.command_line
                        .push_error("Read-only session (--read-only): saving is disabled.");
                    return Some(Task::none());
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let mut saved = 0usize;
                    let mut skipped = 0usize;
                    for t in 0..self.tabs.len() {
                        if self.tabs[t].is_start {
                            continue;
                        }
                        self.sync_vport_display(t);
                        if let Some(path) = self.tabs[t].current_path.clone() {
                            self.tabs[t].scene.document.header.user_real1 =
                                self.tabs[t].scene.annotation_scale as f64;
                            match crate::io::save(&self.tabs[t].scene.document, &path) {
                                Ok(()) => {
                                    self.tabs[t].dirty = false;
                                    saved += 1;
                                }
                                Err(e) => self.command_line.push_error(&format!(
                                    "SAVEALL: {} failed: {e}",
                                    path.display()
                                )),
                            }
                        } else {
                            skipped += 1;
                        }
                    }
                    self.command_line.push_output(&format!(
                        "SAVEALL: saved {saved} drawing(s){}.",
                        if skipped > 0 {
                            format!("; {skipped} need SAVEAS (no file path yet)")
                        } else {
                            String::new()
                        }
                    ));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    self.command_line
                        .push_info("SAVEALL: save each tab individually in the web build.");
                }
                return Some(Task::none());
            }
            "SAVEAS" => return Some(Task::done(Message::SaveAs)),
            // UNDO <n> — step back n operations at once; bare UNDO / U is one step.
            cmd if cmd.starts_with("UNDO ") => {
                let arg = cmd["UNDO ".len()..].trim();
                match arg.parse::<usize>() {
                    Ok(0) => return Some(Task::none()),
                    Ok(n) => return Some(Task::done(Message::UndoMany(n))),
                    Err(_) => {
                        self.command_line
                            .push_error("Usage: UNDO [number of steps]");
                        return Some(Task::none());
                    }
                }
            }
            "UNDO" | "U" => return Some(Task::done(Message::Undo)),
            "REDO" => return Some(Task::done(Message::Redo)),
            // OOPS — restore the objects removed by the most recent ERASE,
            // without undoing any work done since.
            "OOPS" => {
                if self.oops_cache.is_empty() {
                    self.command_line.push_info("OOPS: nothing to restore.");
                } else {
                    self.push_undo_snapshot(i, "OOPS");
                    let restored = std::mem::take(&mut self.oops_cache);
                    let n = restored.len();
                    for e in restored {
                        self.tabs[i].scene.add_entity_clone(e);
                    }
                    self.tabs[i].scene.bump_geometry();
                    self.tabs[i].dirty = true;
                    self.refresh_properties();
                    self.command_line
                        .push_output(&format!("OOPS: restored {n} object(s)."));
                }
            }
            "CLEAR" | "CLR" => return Some(Task::done(Message::ClearScene)),
            "WIREFRAME" | "VW" => return Some(Task::done(Message::SetWireframe(true))),
            // Visual-style commands. OCS renders either a wireframe or a shaded
            // view; the named styles map onto the closest of the two and the
            // chosen style is reported so the mapping is explicit. (`SOLID` is
            // intentionally NOT a visual-style verb — it is the 2D filled-polygon
            // draw command; the shaded ribbon button drives `SetWireframe`.)
            cmd if cmd == "VS"
                || cmd == "VSCURRENT"
                || cmd == "SHADEMODE"
                || cmd == "HIDDENLINE"
                || cmd == "XRAY"
                || cmd == "REALISTIC"
                || cmd == "CONCEPTUAL"
                || cmd == "2DWIREFRAME"
                || cmd == "3DWIREFRAME"
                || cmd.starts_with("VSCURRENT ")
                || cmd.starts_with("SHADEMODE ")
                || cmd.starts_with("VS ") =>
            {
                let style = match cmd {
                    "VS" | "VSCURRENT" | "SHADEMODE" => String::new(),
                    s if s.starts_with("VS ")
                        || s.starts_with("VSCURRENT ")
                        || s.starts_with("SHADEMODE ") =>
                    {
                        cmd.split_whitespace().nth(1).unwrap_or("").to_uppercase()
                    }
                    other => other.to_string(),
                };
                let (wireframe, label) = match style.as_str() {
                    "" | "SHADED" | "S" | "REALISTIC" | "CONCEPTUAL" => (false, "Shaded"),
                    "2DWIREFRAME" | "3DWIREFRAME" | "WIREFRAME" | "W" => (true, "Wireframe"),
                    "HIDDENLINE" | "HIDDEN" | "H" => (false, "Hidden (shown shaded)"),
                    "XRAY" | "X" => (true, "X-Ray (shown as wireframe)"),
                    _ => {
                        self.command_line.push_error(
                            "Usage: VSCURRENT <2dwireframe|wireframe|hidden|realistic|conceptual|shaded|xray>",
                        );
                        return Some(Task::none());
                    }
                };
                self.command_line
                    .push_output(&format!("Visual style: {label}."));
                return Some(Task::done(Message::SetWireframe(wireframe)));
            }
            // CLOSE — close the active drawing tab (with the unsaved-changes
            // prompt the tab-close handler already runs).
            "CLOSE" => {
                return Some(Task::done(Message::TabClose(self.active_tab)));
            }

            "EXIT" | "QUIT" => {
                // Funnel through the OS close path so the unsaved-changes
                // dialog runs before `iced::exit()`. Falls back to a hard
                // exit if there's no main window registered yet.
                if let Some(id) = self.main_window {
                    return Some(Task::done(Message::WindowCloseRequested(id)));
                }
                return Some(iced::exit());
            }

            // ── Frame-budget HUD (Phase 5.3) ───────────────────────────────
            // Toggle the per-rebuild wire-tessellation readout overlay.
            "PERF" => {
                self.perf_hud = !self.perf_hud;
                self.command_line.push_info(if self.perf_hud {
                    "PERF HUD on — shows last wire re-tessellation cost"
                } else {
                    "PERF HUD off"
                });
                return Some(Task::none());
            }

            // ── Background color ───────────────────────────────────────────
            // Usage:  BACKGROUND <r> <g> <b>      (0–255 each)
            //         BACKGROUND WHITE|BLACK|GRAY|DARKGRAY|LTGRAY   (preset)
            //         BACKGROUND RESET            (restore default)
            // The chosen colour is also stored as the persisted default
            // (`default_bg_color` / `default_paper_bg_color`) so it survives
            // restarts and applies to new drawings (#188).
            cmd if cmd == "BACKGROUND" || cmd.starts_with("BACKGROUND ") => {
                let args = cmd.split_whitespace().skip(1).collect::<Vec<_>>();
                let is_paper = self.tabs[i].scene.current_layout != "Model";
                if args
                    .first()
                    .map(|s| s.eq_ignore_ascii_case("RESET"))
                    .unwrap_or(false)
                {
                    if is_paper {
                        self.tabs[i].paper_bg_color = None;
                        self.tabs[i].scene.paper_bg_color = [1.0, 1.0, 1.0, 1.0];
                        self.default_paper_bg_color = None;
                    } else {
                        self.tabs[i].bg_color = None;
                        self.tabs[i].scene.bg_color = [0.11, 0.11, 0.11, 1.0];
                        self.default_bg_color = None;
                    }
                    // Wire colour adaptation (`adapt_to_bg`) reads the bg
                    // at tessellation time, so the cached wires need to
                    // refresh — otherwise a light→dark bg flip leaves
                    // black lines invisible against the new bg. Meshes
                    // bake colour into per-vertex GPU buffers at upload
                    // time; `recolor_meshes` rewrites the CPU side so
                    // the next epoch-driven re-upload picks up the new
                    // colour.
                    self.tabs[i].scene.recolor_meshes();
                    self.tabs[i].scene.bump_geometry();
                    self.command_line
                        .push_output("Background reset to default.");
                } else if let Some(rgba) = parse_background_color(&args) {
                    if is_paper {
                        self.tabs[i].paper_bg_color = Some(rgba);
                        self.tabs[i].scene.paper_bg_color = rgba;
                        self.default_paper_bg_color = Some(rgba);
                    } else {
                        self.tabs[i].bg_color = Some(rgba);
                        self.tabs[i].scene.bg_color = rgba;
                        self.default_bg_color = Some(rgba);
                    }
                    self.tabs[i].scene.recolor_meshes();
                    self.tabs[i].scene.bump_geometry();
                    let [r, g, b, _] = rgba;
                    self.command_line.push_output(&format!(
                        "Background: rgb({}, {}, {})",
                        (r * 255.0).round() as u8,
                        (g * 255.0).round() as u8,
                        (b * 255.0).round() as u8
                    ));
                    // Persisted centrally after this message via
                    // `persist_settings_if_changed()`.
                } else {
                    self.command_line.push_info(
                        "Usage: BACKGROUND <r> <g> <b> (0–255) | WHITE|BLACK|GRAY|DARKGRAY|LTGRAY | RESET",
                    );
                }
            }
            // ORTHO toggles the orthogonal cursor constraint — the standard
            // drafting aid, the same state the status-bar pill drives. Camera
            // projection is a separate concern: PARALLEL / PERSP, driven by the
            // Projection ribbon group.
            "ORTHO" => return Some(Task::done(Message::ToggleOrtho)),
            "PARALLEL" => return Some(Task::done(Message::SetProjection(true))),
            "PERSP" => return Some(Task::done(Message::SetProjection(false))),
            "LAYERS" | "LA" => return Some(Task::done(Message::ToggleLayers)),

            // SCRIPT <path> — run a command script: each non-blank, non-comment
            // line is fed through the same command path the `--script` startup
            // flag uses, so the behaviour matches headless automation exactly.
            cmd if cmd == "SCRIPT"
                || cmd == "SCR"
                || cmd.starts_with("SCRIPT ")
                || cmd.starts_with("SCR ") =>
            {
                let path = cmd.split_once(' ').map(|(_, r)| r.trim().to_string());
                match path {
                    Some(p) if !p.is_empty() => match std::fs::read_to_string(&p) {
                        Ok(text) => {
                            let cmds: Vec<Task<Message>> = text
                                .lines()
                                .map(str::trim)
                                .filter(|l| {
                                    !l.is_empty() && !l.starts_with('#') && !l.starts_with(';')
                                })
                                .map(|l| Task::done(Message::Command(l.to_string())))
                                .collect();
                            self.command_line.push_output(&format!(
                                "SCRIPT: running {} command(s) from {p}.",
                                cmds.len()
                            ));
                            return Some(Task::batch(cmds));
                        }
                        Err(e) => {
                            self.command_line
                                .push_error(&format!("SCRIPT: cannot read {p}: {e}"));
                        }
                    },
                    _ => {
                        self.command_line
                            .push_info("Usage: SCRIPT <path to .scr file>");
                    }
                }
            }

            _ => return None,
        }
        Some(self.finish_dispatch(cmd))
    }
}

/// Parse the argument list of the `BACKGROUND` command into an `[r,g,b,a]`
/// colour (channels 0.0–1.0, `a` always 1.0). Accepts:
///   * three whitespace-separated 0–255 values: `255 255 255`
///   * a named preset: WHITE / BLACK / GRAY|GREY / DARKGRAY|DARKGREY / LTGRAY
/// Returns `None` if the arguments don't match either form.
fn parse_background_color(args: &[&str]) -> Option<[f32; 4]> {
    let to_rgba = |[r, g, b]: [u8; 3]| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
    // Single token: a named preset.
    if args.len() == 1 {
        let preset = match args[0].to_ascii_uppercase().as_str() {
            "WHITE" => [255, 255, 255],
            "BLACK" => [0, 0, 0],
            "GRAY" | "GREY" => [128, 128, 128],
            "DARKGRAY" | "DARKGREY" | "DKGRAY" => [64, 64, 64],
            "LTGRAY" | "LIGHTGRAY" | "LIGHTGREY" => [192, 192, 192],
            _ => return None,
        };
        return Some(to_rgba(preset));
    }
    // Three separate tokens: `r g b`.
    if args.len() >= 3 {
        let r = args[0].parse::<u8>().ok()?;
        let g = args[1].parse::<u8>().ok()?;
        let b = args[2].parse::<u8>().ok()?;
        return Some(to_rgba([r, g, b]));
    }
    None
}
