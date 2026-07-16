use super::{
    document::{DeltaSnapshot, FullSnapshot, HistorySnapshot},
    OpenCADStudio,
};
use acadrust::{EntityType, Handle};
use rustc_hash::FxHashSet as HashSet;

/// Above this many touched entities a "delta-safe" command falls back to a full
/// snapshot: deep-cloning that many before-images would cost more than the (now
/// Arc-cheap) document clone, and huge bulk edits are rare. Below it, the delta
/// keeps a per-edit undo at roughly zero cost.
const DELTA_UNDO_MAX_ENTITIES: usize = 5000;

/// Pre-command state captured by [`OpenCADStudio::begin_undo`] and handed back to
/// [`OpenCADStudio::commit_undo_delta`] to close a delta entry. Lives on the
/// stack across the (synchronous) command body — no per-tab field needed.
pub(super) struct PendingDelta {
    label: String,
    current_layout: String,
    selected_before: Vec<Handle>,
    dirty_before: bool,
}

impl OpenCADStudio {
    pub(super) fn history_label_from_active_cmd(&self, i: usize, fallback: &'static str) -> String {
        self.tabs[i]
            .active_cmd
            .as_ref()
            .map(|cmd| cmd.name().to_string())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(super) fn capture_history_snapshot(
        &self,
        i: usize,
        label: impl Into<String>,
    ) -> HistorySnapshot {
        HistorySnapshot::Full(FullSnapshot {
            document: self.tabs[i].scene.document.clone(),
            current_layout: self.tabs[i].scene.current_layout.clone(),
            selected: self.tabs[i].scene.selected.iter().copied().collect(),
            dirty: self.tabs[i].dirty,
            label: label.into(),
        })
    }

    pub(super) fn push_undo_snapshot(&mut self, i: usize, label: impl Into<String>) {
        let snapshot = self.capture_history_snapshot(i, label);
        self.tabs[i].history.undo_stack.push(snapshot);
        self.tabs[i].history.redo_stack.clear();
    }

    /// Begin undo capture for an entity edit that will touch `touched` entities.
    /// When `delta_safe` (the caller's per-command predicate guarantees the edit
    /// mutates only entities, through the five Scene primitives) and the edit is
    /// small enough, starts a cheap Scene delta recording and returns the
    /// pre-command state to pass to [`OpenCADStudio::commit_undo_delta`] after the
    /// mutation. Otherwise pushes a full snapshot and returns `None`.
    pub(super) fn begin_undo(
        &mut self,
        i: usize,
        label: impl Into<String>,
        touched: usize,
        delta_safe: bool,
    ) -> Option<PendingDelta> {
        let label = label.into();
        if delta_safe && touched <= DELTA_UNDO_MAX_ENTITIES {
            self.tabs[i].scene.begin_undo_recording();
            Some(PendingDelta {
                label,
                current_layout: self.tabs[i].scene.current_layout.clone(),
                selected_before: self.tabs[i].scene.selected.iter().copied().collect(),
                dirty_before: self.tabs[i].dirty,
            })
        } else {
            self.push_undo_snapshot(i, label);
            None
        }
    }

    /// Copy is delta-safe only when no target is a Dimension: copying a
    /// dimension clones a fresh anonymous `*D` block record (non-entity state a
    /// pure-entity delta can't restore).
    pub(super) fn delta_copy_safe(&self, i: usize, handles: &[Handle]) -> bool {
        let doc = &self.tabs[i].scene.document;
        !handles
            .iter()
            .any(|h| matches!(doc.get_entity(*h), Some(EntityType::Dimension(_))))
    }

    /// Erase is delta-safe only when no target belongs to a group: erasing a
    /// grouped entity rewrites the group's membership in `document.objects`.
    pub(super) fn delta_erase_safe(&self, i: usize, handles: &[Handle]) -> bool {
        use acadrust::objects::ObjectType;
        let doc = &self.tabs[i].scene.document;
        !doc.objects.values().any(|o| match o {
            ObjectType::Group(g) => g.entities.iter().any(|h| handles.contains(h)),
            _ => false,
        })
    }

    /// Add is delta-safe only for a plain drawable on an already-existing layer:
    /// an insert / block / image / dimension add also creates block records,
    /// image definitions or layers (non-entity state).
    pub(super) fn delta_add_safe(&self, i: usize, entity: &EntityType) -> bool {
        if matches!(
            entity,
            EntityType::Insert(_)
                | EntityType::Block(_)
                | EntityType::BlockEnd(_)
                | EntityType::RasterImage(_)
                | EntityType::Dimension(_)
                // A viewport commit routes through add_entity_to_layout +
                // bump_geometry_no_blocks, bypassing the recorded Scene::add_entity
                // — nothing would be captured, so keep the full snapshot.
                | EntityType::Viewport(_)
        ) {
            return false;
        }
        let layer = entity.common().layer.clone();
        layer.trim().is_empty() || self.tabs[i].scene.document.layers.contains(&layer)
    }

    /// Close the delta transaction opened by [`OpenCADStudio::begin_undo`]:
    /// harvests the recorded before-images, pairs each with the entity's current
    /// (after) state, and pushes a symmetric [`DeltaSnapshot`] onto the undo
    /// stack. Called after the command's mutations (and after `dirty`/selection
    /// reach their final values).
    pub(super) fn commit_undo_delta(&mut self, i: usize, pending: PendingDelta) {
        let Some(rec) = self.tabs[i].scene.take_undo_recording() else {
            return;
        };
        // The predicate should have kept the command entity-only; if a primitive
        // still poisoned the recording we've already mutated and the pre-command
        // full state is gone, so the entity delta is the best we can do (its
        // entity part stays correct; a leaked layer/group/block just won't
        // revert). Loud in debug, a warning in release — never a silent wrong.
        debug_assert!(
            !rec.is_poisoned(),
            "delta command '{}' mutated non-entity state",
            pending.label
        );
        if rec.is_empty() {
            // Nothing actually changed (e.g. every target was on a locked
            // layer). Leave the undo/redo stacks untouched.
            return;
        }
        if rec.is_poisoned() {
            eprintln!(
                "[undo] delta '{}' touched non-entity state; undo may be incomplete",
                pending.label
            );
        }
        let entities: Vec<(Handle, Option<EntityType>, Option<EntityType>)> = rec
            .into_before_images()
            .into_iter()
            .map(|(h, before)| {
                let after = self.tabs[i].scene.document.get_entity(h).cloned();
                (h, before, after)
            })
            .collect();
        let selected_after = self.tabs[i].scene.selected.iter().copied().collect();
        let dirty_after = self.tabs[i].dirty;
        let delta = DeltaSnapshot {
            entities,
            current_layout: pending.current_layout,
            selected_before: pending.selected_before,
            selected_after,
            dirty_before: pending.dirty_before,
            dirty_after,
            label: pending.label,
        };
        self.tabs[i]
            .history
            .undo_stack
            .push(HistorySnapshot::Delta(delta));
        self.tabs[i].history.redo_stack.clear();
    }

    /// Apply one side of a delta entry in place: `undo` restores each entity's
    /// before-image, `!undo` (redo) its after-image. `None` on the chosen side
    /// means the entity is absent there (erase it). Reports the exact touched
    /// handles through `bump_entities` so the incremental caches patch per-handle
    /// — no full re-tessellation, no `populate_*` document walk.
    fn apply_delta(&mut self, i: usize, d: &DeltaSnapshot, undo: bool) {
        // Install the chosen side of every entity image (Scene handles the
        // in-place / re-insert / remove, the derived-cache reseed and the
        // block-record dedup); we report the touched handles incrementally.
        let changes = self.tabs[i].scene.apply_entity_delta(&d.entities, undo);
        let scene = &mut self.tabs[i].scene;
        scene.bump_entities(&changes);
        let (sel, dirty) = if undo {
            (&d.selected_before, d.dirty_before)
        } else {
            (&d.selected_after, d.dirty_after)
        };
        let restored: HashSet<Handle> = sel
            .iter()
            .copied()
            .filter(|h| scene.document.get_entity(*h).is_some())
            .collect();
        scene.selected = restored;
        // Delta commands never change the layout; this is a no-op unless a
        // future caller widens the delta scope, in which case it stays correct.
        scene.set_current_layout(d.current_layout.clone());
        scene.clear_preview_wire();
        self.tabs[i].dirty = dirty;
        self.tabs[i].active_cmd = None;
        self.tabs[i].snap_result = None;
        self.tabs[i].active_grip = None;
        self.refresh_properties();
    }

    pub(super) fn restore_history_snapshot(&mut self, i: usize, snapshot: FullSnapshot) {
        self.tabs[i].scene.document = snapshot.document;
        self.tabs[i].scene.set_current_layout(snapshot.current_layout);
        // Force a re-tessellation: the cached wires were keyed against the
        // outgoing document / layout and would be returned unchanged
        // otherwise (`set_current_layout` only bumps on actual change).
        self.tabs[i].scene.bump_geometry();
        self.tabs[i].scene.selected = snapshot
            .selected
            .into_iter()
            .filter(|h| self.tabs[i].scene.document.get_entity(*h).is_some())
            .collect::<HashSet<_>>();
        self.tabs[i].scene.populate_hatches_from_document();
        self.tabs[i].scene.populate_images_from_document();
        self.tabs[i].scene.populate_meshes_from_document();
        // Keep the Isolate/Hide set in step with the restored per-entity
        // visibility so End Isolation stays correct after undo/redo.
        self.tabs[i].scene.sync_hidden_from_invisible();
        self.tabs[i].scene.clear_preview_wire();
        self.tabs[i].scene.images.clear();
        self.tabs[i].active_cmd = None;
        self.tabs[i].snap_result = None;
        self.tabs[i].active_grip = None;
        self.tabs[i].dirty = snapshot.dirty;
        let doc_layers = self.tabs[i].scene.document.layers.clone();
        let vp_info = self.tabs[i].scene.viewport_list();
        self.tabs[i]
            .layers
            .sync_with_viewports(&doc_layers, vp_info);
        self.sync_ribbon_layers();
        self.refresh_properties();
    }

    pub(super) fn undo_active_tab(&mut self) {
        self.undo_steps(1);
    }

    pub(super) fn redo_active_tab(&mut self) {
        self.redo_steps(1);
    }

    pub(super) fn undo_steps(&mut self, steps: usize) {
        let i = self.active_tab;
        let available = self.tabs[i].history.undo_stack.len();
        let steps = steps.min(available);
        if steps == 0 {
            self.command_line.push_info("Nothing to undo.");
            return;
        }

        let mut last_label = String::new();
        for _ in 0..steps {
            let Some(snapshot) = self.tabs[i].history.undo_stack.pop() else {
                break;
            };
            last_label = snapshot.label().to_string();
            match snapshot {
                HistorySnapshot::Full(f) => {
                    let current = self.capture_history_snapshot(i, f.label.clone());
                    self.tabs[i].history.redo_stack.push(current);
                    self.restore_history_snapshot(i, f);
                }
                HistorySnapshot::Delta(d) => {
                    // Symmetric: undo applies the before side, then the same
                    // delta rides to the redo stack (it still holds the after
                    // side) — no current-state capture needed.
                    self.apply_delta(i, &d, true);
                    self.tabs[i].history.redo_stack.push(HistorySnapshot::Delta(d));
                }
            }
        }
        self.command_line
            .push_output(&format!("Undo: {last_label}"));
    }

    pub(super) fn redo_steps(&mut self, steps: usize) {
        let i = self.active_tab;
        let available = self.tabs[i].history.redo_stack.len();
        let steps = steps.min(available);
        if steps == 0 {
            self.command_line.push_info("Nothing to redo.");
            return;
        }

        let mut last_label = String::new();
        for _ in 0..steps {
            let Some(snapshot) = self.tabs[i].history.redo_stack.pop() else {
                break;
            };
            last_label = snapshot.label().to_string();
            match snapshot {
                HistorySnapshot::Full(f) => {
                    let current = self.capture_history_snapshot(i, f.label.clone());
                    self.tabs[i].history.undo_stack.push(current);
                    self.restore_history_snapshot(i, f);
                }
                HistorySnapshot::Delta(d) => {
                    self.apply_delta(i, &d, false);
                    self.tabs[i].history.undo_stack.push(HistorySnapshot::Delta(d));
                }
            }
        }
        self.command_line
            .push_output(&format!("Redo: {last_label}"));
    }
}

pub(super) fn history_dropdown_labels(stack: &[HistorySnapshot]) -> Vec<String> {
    stack
        .iter()
        .rev()
        .map(|snapshot| snapshot.label().to_string())
        .collect()
}
