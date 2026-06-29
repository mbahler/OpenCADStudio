use super::*;

impl OpenCADStudio {
    pub(super) fn dispatch_blocks(&mut self, cmd: &str, i: usize) -> Option<Task<Message>> {
        match cmd {
            // ── BASE — drawing insertion base point ───────────────────────
            // Bare BASE picks a point interactively; BASE <x> <y> [z] sets it
            // directly. The base point is stored per active space (model/paper).
            cmd if cmd == "BASE" || cmd.starts_with("BASE ") => {
                let rest = cmd.strip_prefix("BASE").unwrap_or("").trim();
                if rest.is_empty() {
                    use crate::modules::insert::base_point::BaseCommand;
                    let c = BaseCommand::new();
                    self.command_line.push_info(&c.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(c));
                } else {
                    let nums: Vec<f64> = rest
                        .split(|ch| ch == ' ' || ch == ',')
                        .filter(|s| !s.is_empty())
                        .filter_map(|s| s.parse::<f64>().ok())
                        .collect();
                    if nums.len() >= 2 {
                        let z = nums.get(2).copied().unwrap_or(0.0);
                        let pt = acadrust::types::Vector3::new(nums[0], nums[1], z);
                        let is_paper = self.tabs[i].scene.current_layout != "Model";
                        self.push_undo_snapshot(i, "BASE");
                        if is_paper {
                            self.tabs[i]
                                .scene
                                .document
                                .header
                                .paper_space_insertion_base = pt;
                        } else {
                            self.tabs[i]
                                .scene
                                .document
                                .header
                                .model_space_insertion_base = pt;
                        }
                        self.tabs[i].dirty = true;
                        let space = if is_paper {
                            "paper space"
                        } else {
                            "model space"
                        };
                        self.command_line.push_output(&format!(
                            "Base point ({}, {}, {}) set for {space}.",
                            nums[0], nums[1], z
                        ));
                    } else {
                        self.command_line.push_error("Usage: BASE <x> <y> [z]");
                    }
                }
            }

            "COPYCLIP" | "CC" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .into_iter()
                    .map(|(h, _)| h)
                    .collect();
                if handles.is_empty() {
                    use crate::modules::draw::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("COPYCLIP");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let entities: Vec<_> = handles
                        .iter()
                        .filter_map(|&h| self.tabs[i].scene.document.get_entity(h).cloned())
                        .collect();
                    self.clipboard_centroid = super::super::helpers::entities_centroid(
                        &self.tabs[i].scene.wire_models_for(&handles),
                    );
                    self.clipboard = entities;
                    self.clipboard_deps = super::super::ClipboardDeps::capture(
                        &self.tabs[i].scene.document,
                        &self.clipboard,
                    );
                    self.command_line.push_info(&format!(
                        "{} object(s) copied to clipboard.",
                        self.clipboard.len()
                    ));
                }
            }

            // COPYBASE — copy the selection to the clipboard with a picked base
            // point (vs COPYCLIP, which uses the selection centroid).
            "COPYBASE" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .into_iter()
                    .map(|(h, _)| h)
                    .collect();
                if handles.is_empty() {
                    use crate::modules::draw::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("COPYBASE");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::draw::clipboard::copy_base::CopyBaseCommand;
                    let cmd = CopyBaseCommand::new();
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            // Internal: the base point picked by COPYBASE — perform the copy.
            cmd if cmd.starts_with("COPYBASE_AT ") => {
                let coords: Vec<f64> = cmd
                    .trim_start_matches("COPYBASE_AT")
                    .split_whitespace()
                    .filter_map(|s| s.parse::<f64>().ok())
                    .collect();
                let handles: Vec<_> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .into_iter()
                    .map(|(h, _)| h)
                    .collect();
                if coords.len() == 3 && !handles.is_empty() {
                    let base = glam::DVec3::new(coords[0], coords[1], coords[2]);
                    let entities: Vec<_> = handles
                        .iter()
                        .filter_map(|&h| self.tabs[i].scene.document.get_entity(h).cloned())
                        .collect();
                    self.clipboard_centroid = base;
                    self.clipboard = entities;
                    self.clipboard_deps = super::super::ClipboardDeps::capture(
                        &self.tabs[i].scene.document,
                        &self.clipboard,
                    );
                    self.command_line.push_info(&format!(
                        "{} object(s) copied to clipboard (base {:.3},{:.3}).",
                        self.clipboard.len(),
                        base.x,
                        base.y
                    ));
                }
            }

            "CUTCLIP" | "CX" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .into_iter()
                    .map(|(h, _)| h)
                    .collect();
                if handles.is_empty() {
                    use crate::modules::draw::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("CUTCLIP");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    let entities: Vec<_> = handles
                        .iter()
                        .filter_map(|&h| self.tabs[i].scene.document.get_entity(h).cloned())
                        .collect();
                    self.clipboard_centroid = super::super::helpers::entities_centroid(
                        &self.tabs[i].scene.wire_models_for(&handles),
                    );
                    let count = entities.len();
                    self.clipboard = entities;
                    self.clipboard_deps = super::super::ClipboardDeps::capture(
                        &self.tabs[i].scene.document,
                        &self.clipboard,
                    );
                    self.push_undo_snapshot(i, "CUTCLIP");
                    self.tabs[i].scene.erase_entities(&handles);
                    self.tabs[i].scene.deselect_all();
                    self.tabs[i].dirty = true;
                    self.refresh_properties();
                    self.command_line
                        .push_info(&format!("{} object(s) cut to clipboard.", count));
                }
            }

            "PASTECLIP" | "PC" => {
                if self.clipboard.is_empty() {
                    self.command_line.push_error("Clipboard is empty.");
                } else {
                    let wires = self.tabs[i].scene.wires_for_entities(&self.clipboard);
                    let centroid = self.clipboard_centroid;
                    use crate::modules::draw::clipboard::paste::PasteCommand;
                    // The ghost anchor is a display-only offset; the precise
                    // paste delta is computed in f64 at commit time.
                    let cmd = PasteCommand::new(wires, centroid.as_vec3());
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            // PASTEORIG — paste at the entities' original coordinates (no pick).
            "PASTEORIG" => {
                if self.clipboard.is_empty() {
                    self.command_line
                        .push_error("PASTEORIG: clipboard is empty.");
                } else {
                    let count = self.clipboard.len();
                    self.push_undo_snapshot(i, "PASTEORIG");
                    // No transform: entities keep their original coordinates.
                    let _ = self.finalize_paste(i, None);
                    self.tabs[i].dirty = true;
                    self.refresh_layer_panel();
                    self.refresh_properties();
                    self.command_line.push_output(&format!(
                        "PASTEORIG: {} object(s) pasted at original coordinates.",
                        count
                    ));
                }
            }

            // PASTEBLOCK — wrap the clipboard contents in a new block definition
            // and place one insert of it at the clipboard's original location.
            "PASTEBLOCK" => {
                if self.clipboard.is_empty() {
                    self.command_line
                        .push_error("PASTEBLOCK: clipboard is empty.");
                } else {
                    self.push_undo_snapshot(i, "PASTEBLOCK");
                    self.merge_clipboard_deps(i);
                    // Recreate any block definition the clipboard's INSERTs
                    // reference, so nested blocks inside the new wrapper block
                    // don't render empty. (#135 / #158)
                    self.merge_clipboard_blocks(i);
                    // Recreate each entity's xdictionary graph (XCLIP filters)
                    // and stamp the new root onto the wrapped entity, so the
                    // block's nested insert keeps its clip. (#xclip-paste)
                    let ext_roots = self.recreate_clipboard_ext_roots(i);
                    let name = self.unique_block_name("Block");
                    let base = self.clipboard_centroid;
                    let mut entities = self.clipboard.clone();
                    for (idx, root) in ext_roots {
                        if let Some(e) = entities.get_mut(idx) {
                            e.common_mut().xdictionary_handle = Some(root);
                        }
                    }
                    match self.tabs[i]
                        .scene
                        .define_block_from_owned_entities(entities, &name, base)
                    {
                        Ok(()) => {
                            // Block defined; now place it interactively so the
                            // user picks the drop point (insertion uses the
                            // clipboard centroid as the block's base). The
                            // clipboard wires rubber-band under the cursor.
                            self.tabs[i].scene.populate_meshes_from_document();
                            self.tabs[i].dirty = true;
                            let wires = self.tabs[i].scene.wires_for_entities(&self.clipboard);
                            use crate::modules::insert::insert_block::InsertBlockCommand;
                            let cmd =
                                InsertBlockCommand::new_for_block(name, wires, base.as_vec3());
                            self.command_line.push_info(&cmd.prompt());
                            self.tabs[i].active_cmd = Some(Box::new(cmd));
                        }
                        Err(e) => self.command_line.push_error(&format!("PASTEBLOCK: {e}")),
                    }
                }
            }

            "BLOCK" | "BMAKE" => {
                let handles: Vec<_> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .into_iter()
                    .map(|(h, _)| h)
                    .collect();
                if handles.is_empty() {
                    use crate::modules::draw::select::SelectObjectsCommand;
                    let cmd = SelectObjectsCommand::new("BLOCK");
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                } else {
                    use crate::modules::insert::create_block::CreateBlockCommand;
                    let cmd = CreateBlockCommand::new(handles);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "INSERT" => {
                let blocks = self.tabs[i].scene.custom_block_names();
                if blocks.is_empty() {
                    self.command_line
                        .push_error("No user-defined blocks found in this drawing.");
                } else {
                    use crate::modules::insert::insert_block::InsertBlockCommand;
                    let cmd = InsertBlockCommand::new(blocks);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            "MINSERT" => {
                let blocks = self.tabs[i].scene.custom_block_names();
                if blocks.is_empty() {
                    self.command_line
                        .push_error("No user-defined blocks found in this drawing.");
                } else {
                    use crate::modules::insert::minsert::MinsertCommand;
                    let cmd = MinsertCommand::new(blocks);
                    self.command_line.push_info(&cmd.prompt());
                    self.tabs[i].active_cmd = Some(Box::new(cmd));
                }
            }

            // ATTSYNC <block> — reconcile every insert of <block> against the
            // block's current attribute definitions: drop attributes whose tag
            // no longer exists and add any newly-defined ones (keeping the values
            // of attributes that remain).
            cmd if cmd == "ATTSYNC" || cmd.starts_with("ATTSYNC ") => {
                let arg = cmd.trim_start_matches("ATTSYNC").trim();
                let blocks = self.tabs[i].scene.custom_block_names();
                if arg.is_empty() {
                    self.command_line.push_info(&format!(
                        "Usage: ATTSYNC <block name>.  Blocks: {}",
                        if blocks.is_empty() {
                            "(none)".to_string()
                        } else {
                            blocks.join(", ")
                        }
                    ));
                    return Some(Task::none());
                }
                let Some(block) = blocks.iter().find(|b| b.eq_ignore_ascii_case(arg)).cloned()
                else {
                    self.command_line
                        .push_error(&format!("ATTSYNC: no block named \"{arg}\"."));
                    return Some(Task::none());
                };
                // Gather the block's attribute definitions (tag, default value).
                let doc = &self.tabs[i].scene.document;
                let attdefs: Vec<(String, String)> = doc
                    .block_records
                    .get(&block)
                    .map(|br| {
                        br.entity_handles
                            .iter()
                            .filter_map(|h| doc.get_entity(*h))
                            .filter_map(|e| match e {
                                acadrust::EntityType::AttributeDefinition(a) => {
                                    Some((a.tag.clone(), a.default_value.clone()))
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.push_undo_snapshot(i, "ATTSYNC");
                let mut synced = 0usize;
                for e in self.tabs[i].scene.document.entities_mut() {
                    if let acadrust::EntityType::Insert(ins) = e {
                        if ins.block_name.eq_ignore_ascii_case(&block) {
                            ins.attributes.retain(|a| {
                                attdefs.iter().any(|(t, _)| t.eq_ignore_ascii_case(&a.tag))
                            });
                            for (tag, default) in &attdefs {
                                if !ins
                                    .attributes
                                    .iter()
                                    .any(|a| a.tag.eq_ignore_ascii_case(tag))
                                {
                                    ins.attributes
                                        .push(acadrust::entities::AttributeEntity::new(
                                            tag.clone(),
                                            default.clone(),
                                        ));
                                }
                            }
                            synced += 1;
                        }
                    }
                }
                self.tabs[i].scene.bump_geometry();
                self.tabs[i].dirty = true;
                self.command_line.push_output(&format!(
                    "ATTSYNC: synchronised {synced} insert(s) of \"{block}\" against {} attribute definition(s).",
                    attdefs.len()
                ));
            }

            // ADCENTER / CONTENTBROWSER — report the drawing's named content
            // (blocks and layers) from the command line in place of the browser
            // panel.
            "ADCENTER" | "CONTENTBROWSER" | "ADC" => {
                let blocks = self.tabs[i].scene.custom_block_names();
                let layers: Vec<String> = self.tabs[i]
                    .scene
                    .document
                    .layers
                    .names()
                    .map(|s| s.to_string())
                    .collect();
                self.command_line.push_output(&format!(
                    "Blocks ({}): {}",
                    blocks.len(),
                    if blocks.is_empty() {
                        "(none)".to_string()
                    } else {
                        blocks.join(", ")
                    }
                ));
                self.command_line.push_output(&format!(
                    "Layers ({}): {}",
                    layers.len(),
                    layers.join(", ")
                ));
            }

            // BLOCKPALETTE — list the blocks available to insert.
            "BLOCKPALETTE" | "BLOCKSPALETTE" => {
                let blocks = self.tabs[i].scene.custom_block_names();
                if blocks.is_empty() {
                    self.command_line.push_info(
                        "No user-defined blocks. Define one with BLOCK, then INSERT it.",
                    );
                } else {
                    self.command_line.push_output(&format!(
                        "Blocks ({}) — insert with INSERT <name>:  {}",
                        blocks.len(),
                        blocks.join(", ")
                    ));
                }
            }

            // ATTMAN — list the attribute definitions of each block (or one named
            // block). The full graphical manager is not built; this reports the
            // same information from the command line.
            cmd if cmd == "ATTMAN" || cmd == "BATTMAN" || cmd.starts_with("ATTMAN ") => {
                let arg = cmd.trim_start_matches("ATTMAN").trim();
                let blocks = self.tabs[i].scene.custom_block_names();
                let targets: Vec<String> = if arg.is_empty() {
                    blocks
                } else {
                    blocks
                        .into_iter()
                        .filter(|b| b.eq_ignore_ascii_case(arg))
                        .collect()
                };
                if targets.is_empty() {
                    self.command_line
                        .push_info("ATTMAN: no matching block.  Usage: ATTMAN [block name]");
                } else {
                    let doc = &self.tabs[i].scene.document;
                    let mut lines: Vec<String> = Vec::new();
                    for block in &targets {
                        let attdefs: Vec<String> = doc
                            .block_records
                            .get(block)
                            .map(|br| {
                                br.entity_handles
                                    .iter()
                                    .filter_map(|h| doc.get_entity(*h))
                                    .filter_map(|e| match e {
                                        acadrust::EntityType::AttributeDefinition(a) => {
                                            Some(format!("{}={}", a.tag, a.default_value))
                                        }
                                        _ => None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        lines.push(if attdefs.is_empty() {
                            format!("  {block}: no attributes.")
                        } else {
                            format!("  {block}: {}", attdefs.join(", "))
                        });
                    }
                    for l in lines {
                        self.command_line.push_output(&l);
                    }
                }
            }

            "XATTACH" | "XA" => {
                // Launch the file picker; XAttachPickResult will start the command.
                return Some(Task::done(Message::XAttachPick));
            }

            cmd if cmd == "WBLOCK" || cmd == "WB" || cmd.starts_with("WBLOCK ") => {
                let arg = cmd.splitn(2, ' ').nth(1).unwrap_or("").trim();
                if arg.is_empty() {
                    // No argument: use selected entities (*) if any, else ask.
                    let sel: Vec<_> = self.tabs[i].scene.selected.iter().copied().collect();
                    if sel.is_empty() {
                        self.command_line.push_error(
                            "WBLOCK  Select entities first, or: WBLOCK <block name>  or  WBLOCK *",
                        );
                    } else {
                        return Some(Task::done(Message::WblockSave("*".to_string())));
                    }
                } else {
                    return Some(Task::done(Message::WblockSave(arg.to_string())));
                }
            }

            "XREF" | "XR" => {
                // List all xref blocks in the current drawing.
                let xrefs: Vec<String> = self.tabs[i]
                    .scene
                    .document
                    .block_records
                    .iter()
                    .filter(|br| br.flags.is_xref || br.flags.is_xref_overlay)
                    .map(|br| {
                        format!(
                            "  {} — {}",
                            br.name,
                            if br.xref_path.is_empty() {
                                "(no path)".to_string()
                            } else {
                                br.xref_path.clone()
                            }
                        )
                    })
                    .collect();
                if xrefs.is_empty() {
                    self.command_line
                        .push_output("XREF  No external references in this drawing.");
                } else {
                    self.command_line.push_output("XREF  External references:");
                    for line in xrefs {
                        self.command_line.push_output(&line);
                    }
                }
            }

            "XRELOAD" => {
                // Reload all xrefs for the current drawing.
                if let Some(path) = &self.tabs[i].current_path.clone() {
                    if let Some(base_dir) = path.parent() {
                        let (infos, _dropped) = crate::io::xref::resolve_xrefs(
                            &mut self.tabs[i].scene.document,
                            base_dir,
                        );
                        for info in &infos {
                            match info.status {
                                crate::io::xref::XrefStatus::Loaded => {
                                    self.command_line
                                        .push_output(&format!("XREF  Reloaded \"{}\"", info.name));
                                }
                                crate::io::xref::XrefStatus::NotFound => {
                                    self.command_line.push_error(&format!(
                                        "XREF  Not found: \"{}\" ({})",
                                        info.name, info.path
                                    ));
                                }
                                crate::io::xref::XrefStatus::Unloaded => {
                                    self.command_line.push_info(&format!(
                                        "XREF  Unloaded (skipped): \"{}\"",
                                        info.name
                                    ));
                                }
                            }
                        }
                        self.tabs[i].scene.populate_hatches_from_document();
                        self.tabs[i].scene.populate_images_from_document();
                        self.tabs[i].scene.populate_meshes_from_document();
                    }
                } else {
                    self.command_line
                        .push_error("XREF  Save the drawing first to resolve relative XREF paths.");
                }
            }

            // XOPEN — open the source file of the selected external reference in
            // a new tab (reuses the existence-checked OpenRecent path).
            "XOPEN" => {
                let names: Vec<String> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .iter()
                    .filter_map(|(_, e)| match e {
                        acadrust::EntityType::Insert(ins) => Some(ins.block_name.clone()),
                        _ => None,
                    })
                    .collect();
                let path = names.iter().find_map(|bn| {
                    let br = self.tabs[i].scene.document.block_records.get(bn)?;
                    if br.xref_path.is_empty() {
                        None
                    } else {
                        Some(br.xref_path.clone())
                    }
                });
                match path {
                    Some(p) => {
                        return Some(Task::done(Message::OpenRecent(std::path::PathBuf::from(p))))
                    }
                    None => self
                        .command_line
                        .push_error("XOPEN: select an external reference (xref) to open."),
                }
            }

            // NCOPY — copy the nested objects of the selected block reference(s)
            // into model space, keeping the block (a non-destructive extraction;
            // every nested object is copied — the block is not exploded).
            "NCOPY" | "NCOPYALL" => {
                use crate::modules::draw::modify::explode::explode_entity;
                let inserts: Vec<acadrust::Handle> = self.tabs[i]
                    .scene
                    .selected_entities()
                    .iter()
                    .filter(|(_, e)| matches!(e, acadrust::EntityType::Insert(_)))
                    .map(|(h, _)| *h)
                    .collect();
                if inserts.is_empty() {
                    self.command_line
                        .push_error("NCOPY: select a block reference first.");
                    return Some(Task::none());
                }
                self.push_undo_snapshot(i, "NCOPY");
                let mut n = 0usize;
                for h in &inserts {
                    let nested = match self.tabs[i].scene.document.get_entity(*h).cloned() {
                        Some(ins) => explode_entity(&ins, &self.tabs[i].scene.document),
                        None => Vec::new(),
                    };
                    for e in nested {
                        self.tabs[i].scene.add_entity(e);
                        n += 1;
                    }
                }
                self.tabs[i].dirty = true;
                self.command_line.push_output(&format!(
                    "NCOPY: copied {n} nested object(s) into model space (blocks kept)."
                ));
            }

            _ => return None,
        }
        Some(self.finish_dispatch(cmd))
    }
}
