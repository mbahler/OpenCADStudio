// Command system — trait for interactive CAD commands.
//
// Each tool that requires user interaction (point picks, object selection,
// numeric input) implements `CadCommand`.  The active command receives
// viewport events from main.rs and returns `CmdResult` tokens that tell
// the host what to do next.

use crate::scene::hatch_model::HatchModel;
use crate::scene::wire_model::WireModel;
use acadrust::{EntityType, Handle};
use glam::Vec3;

// ── Transform ─────────────────────────────────────────────────────────────

/// A geometric transformation applied to existing entities.
#[derive(Clone)]
pub enum EntityTransform {
    /// Move every point by the given world-space delta (Y-up XZ plane).
    Translate(Vec3),
    /// Rotate around `center` by `angle_rad` in the XZ plane (Y-up).
    Rotate { center: Vec3, angle_rad: f32 },
    /// Uniform scale from `center` by `factor`.
    Scale { center: Vec3, factor: f32 },
    /// Mirror across the line through `p1`→`p2` in the XZ plane (Y-up).
    Mirror { p1: Vec3, p2: Vec3 },
}

// ── Tangent object ─────────────────────────────────────────────────────────

/// Geometric representation of a tangent-snap target.
#[derive(Clone, Copy, Debug)]
pub enum TangentObject {
    /// Infinite line through two world-space XZ-plane points.
    Line { p1: Vec3, p2: Vec3 },
    /// Circle in the XZ plane.
    Circle { center: Vec3, radius: f32 },
}

// ── Result token ──────────────────────────────────────────────────────────

/// Returned by every `CadCommand` method to tell main.rs what to do.
#[allow(dead_code)]
pub enum CmdResult {
    /// Command is still waiting for the next point; show updated prompt.
    NeedPoint,
    /// Update the committed-segment wire (normal colour) and keep collecting points.
    InterimWire(WireModel),
    /// Update the in-progress (cyan) preview wire in the viewport.
    Preview(WireModel),
    /// Commit an acadrust entity to the document; keep the command active.
    CommitEntity(EntityType),
    /// Commit an acadrust entity to the document and end the command.
    CommitAndExit(EntityType),
    /// Commit a Model-tab 3D solid: the acadrust entity (for selection /
    /// persistence) plus its truck B-rep (cached for boolean ops + shaded
    /// rendering). Ends the command.
    CommitSolid {
        entity: EntityType,
        solid: Box<truck_modeling::Solid>,
    },
    /// Commit an acadrust entity, end the command, and open the in-place text
    /// editor on it (used by MLEADER to type the annotation after placement).
    CommitAndEditText(EntityType),
    /// Commit several entities, end the command, and open the in-place text
    /// editor on the one at `edit_index` (used by LEADER to place the leader
    /// line plus an empty MText annotation, then type into the MText).
    CommitManyAndEditText {
        entities: Vec<EntityType>,
        edit_index: usize,
    },
    /// Create a block definition from existing entities and insert one reference.
    CreateBlock {
        handles: Vec<Handle>,
        name: String,
        base: Vec3,
    },
    /// Apply a transform to selected entities and end the command.
    TransformSelected(Vec<Handle>, EntityTransform),
    /// Copy selected entities with a transform; command stays active for more copies.
    CopySelected(Vec<Handle>, EntityTransform),
    /// Commit a hatch fill (stored in Scene::hatches, not the DXF document).
    CommitHatch(HatchModel),
    /// Copy selected entities with multiple transforms (e.g. rectangular array); end command.
    BatchCopy(Vec<Handle>, Vec<EntityTransform>),
    /// Erase `handle` and replace with new entities; command stays active.
    ReplaceEntity(Handle, Vec<EntityType>),
    /// Replace / delete multiple entities and add new ones; command ends.
    /// Each pair: (handle_to_erase, replacement_entities) — empty vec = delete only.
    ReplaceMany(Vec<(Handle, Vec<EntityType>)>, Vec<EntityType>),
    /// Cancel: discard any preview and end the command.
    Cancel,
    /// End the selection-gather phase and re-dispatch the named command
    /// with the gathered handles installed as the active scene selection.
    Relaunch(String, Vec<Handle>),
    /// Move `dest` entities to the layer of the `src` entity; end command.
    MatchEntityLayer { dest: Vec<Handle>, src: Handle },
    /// Copy all visual properties (layer/color/linetype/lineweight) from `src` to `dest`; end command.
    MatchProperties { dest: Vec<Handle>, src: Handle },
    /// Create a named group from the given entity handles; end command.
    CreateGroup { handles: Vec<Handle>, name: String },
    /// Dissolve all groups that contain any of the given handles; end command.
    DeleteGroups { handles: Vec<Handle> },
    /// Freeze or thaw layers by name in the given viewport; command stays active.
    VpLayerUpdate {
        vp_handle: Handle,
        freeze: Vec<String>,
        thaw: Vec<String>,
    },
    /// Paste clipboard entities translated so their centroid lands at `base_pt`; end command.
    PasteClipboard { base_pt: Vec3 },
    /// Zoom the model-space camera to fit the given corner points; end command.
    ZoomToWindow { p1: Vec3, p2: Vec3 },
    /// Print a measurement result to the command line and end the command.
    Measurement(String),
    /// Break `handle` at points `p1` and `p2`; replace with computed fragments.
    BreakEntity { handle: Handle, p1: Vec3, p2: Vec3 },
    /// Attempt to join the given entities into fewer merged entities.
    JoinEntities(Vec<Handle>),
    /// Apply a polyline-edit operation to one entity; keep command active.
    PeditOp {
        handle: Handle,
        op: crate::modules::home::modify::pedit::PeditOp,
    },
    /// Place Point entities at N equal intervals along the entity.
    DivideEntity { handle: Handle, n: usize },
    /// Place Point entities at `segment_length` intervals along the entity.
    MeasureEntity { handle: Handle, segment_length: f64 },
    /// Extend/trim a Line or Arc by the given mode; end command.
    LengthenEntity {
        handle: Handle,
        pick_pt: Vec3,
        mode: crate::modules::home::modify::lengthen::LenMode,
    },
    /// Align selected entities: translate to dst1, rotate by angle_rad, optional scale.
    AlignSelected {
        handles: Vec<Handle>,
        src1: Vec3,
        dst1: Vec3,
        angle_rad: f32,
        scale: f32,
    },
    /// Set the plot window on the active layout's PlotSettings.
    SetPlotWindow { p1: Vec3, p2: Vec3 },
    /// Replace the text content of a Text/MText entity in-place.
    DdeditEntity { handle: Handle, new_text: String },
    /// Open the in-place editor (plain box or rich MText editor, per type) for
    /// a text-bearing entity picked by a command such as DDEDIT.
    EditTextEntity { handle: Handle },
    /// Open the in-place MText editor (formatting toolbar + multi-line text
    /// area with live viewport preview). `handle` is `Some` when editing an
    /// existing MText, `None` when creating a new one at `pos`.
    OpenMTextEditor {
        pos: Vec3,
        handle: Option<Handle>,
        initial: String,
        height: f64,
    },
    /// Open the in-place single-line TEXT editor (a plain text-entry box, no
    /// formatting toolbar). `handle` is `Some` when editing an existing Text,
    /// `None` when creating a new one at `pos`.
    OpenTextEditor {
        pos: Vec3,
        handle: Option<Handle>,
        initial: String,
        height: f64,
    },
    /// Apply new pattern/scale/angle to an existing hatch entity.
    HatcheditApply {
        handle: Handle,
        name: String,
        scale: f32,
        angle: f32,
    },
    /// Stretch entities: move only vertices/endpoints inside the crossing window.
    StretchEntities {
        handles: Vec<Handle>,
        /// Min corner of the crossing window in world XZ (= DXF XY).
        win_min: Vec3,
        /// Max corner of the crossing window in world XZ (= DXF XY).
        win_max: Vec3,
        /// Translation vector to apply to vertices inside the window.
        delta: Vec3,
    },
    /// Create a Solid3D placeholder entity + associated MeshModel.
    /// `mesh_fn` is called with the entity's handle string to build the mesh.
    CommitSolid3D {
        mesh_fn: Box<dyn FnOnce(String) -> Option<crate::scene::mesh_model::MeshModel> + Send>,
    },
    /// Extrude the profile entity `handle` by `height` along Z.
    ExtrudeEntity {
        handle: Handle,
        height: f32,
        color: [f32; 4],
    },
    /// Revolve the profile entity `handle` around the given axis by `angle_deg`.
    RevolveEntity {
        handle: Handle,
        axis_start: glam::Vec3,
        axis_end: glam::Vec3,
        angle_deg: f32,
        color: [f32; 4],
    },
    /// Sweep the profile entity `profile_handle` along `path_handle`.
    SweepEntity {
        profile_handle: Handle,
        path_handle: Handle,
        color: [f32; 4],
    },
    /// Loft through a series of profile entities.
    LoftEntities {
        handles: Vec<Handle>,
        color: [f32; 4],
    },
    /// INSERT landed on a block that has AttributeDefinitions.
    /// The host should look up the attdefs for `block_name` from the document
    /// and call `attreq_set_attdefs()` on the command, then loop on text input.
    AttreqNeeded { block_name: String },
}

/// What kind of value the active command is currently asking for. Drives
/// the dynamic-input overlay so the tooltip shows the relevant quantity
/// (coordinates for a point pick, a single length for a radius/distance
/// prompt, degrees for an angle prompt).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DynField {
    /// A position — X/Y coordinates (or distance+angle relative to the
    /// last point). The default for every command step.
    #[default]
    Point,
    /// A single linear distance (radius, length, offset) measured from
    /// the last point.
    Distance,
    /// An angle, shown in degrees, measured from the last point.
    Angle,
}

// ── Trait ─────────────────────────────────────────────────────────────────

/// An interactive CAD command that collects user input step-by-step.
pub trait CadCommand: Send {
    /// Short name shown in the command line prompt, e.g. `"LINE"`.
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    /// Current prompt string to display in the command line.
    fn prompt(&self) -> String;

    /// Called when the user left-clicks in the viewport (point pick).
    fn on_point(&mut self, pt: Vec3) -> CmdResult;

    /// Called when the user presses Enter (finalize / next option).
    fn on_enter(&mut self) -> CmdResult;

    /// Called when the user presses Escape (cancel).
    #[allow(dead_code)]
    fn on_escape(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    /// Returns `true` when the command needs entity picking (hit-test) instead of point picking.
    fn needs_entity_pick(&self) -> bool {
        false
    }

    /// Called when the user clicks and `needs_entity_pick()` is true.
    /// `handle` is the nearest wire's entity handle (Handle::NULL if nothing found).
    fn on_entity_pick(&mut self, _handle: Handle, _pt: Vec3) -> CmdResult {
        CmdResult::Cancel
    }

    /// Called after `CmdResult::ReplaceEntity` is applied to the document.
    /// `old` is the erased handle; `new_handles` are the handles assigned to the replacement entities.
    /// Commands that stay active across replaces should update their internal snapshots here.
    fn on_entity_replaced(&mut self, _old: Handle, _new_handles: &[Handle]) {}

    /// Called on every mouse-move when `needs_entity_pick()` is true.
    /// Return preview wires showing the operation result under the cursor.
    /// Default: empty (no preview).
    fn on_hover_entity(&mut self, _handle: Handle, _pt: Vec3) -> Vec<WireModel> {
        vec![]
    }

    /// Called on every mouse-move in the viewport.
    /// Return `Some(WireModel)` to update the rubber-band preview, `None` to skip.
    fn on_mouse_move(&mut self, _pt: Vec3) -> Option<WireModel> {
        None
    }

    /// Called on every mouse-move; return all preview wires to show (object ghosts + rubber-band).
    /// Default: forwards to `on_mouse_move` for backwards compatibility.
    fn on_preview_wires(&mut self, pt: Vec3) -> Vec<WireModel> {
        self.on_mouse_move(pt).into_iter().collect()
    }

    /// Returns `true` when the command is waiting for text typed in the command line.
    fn wants_text_input(&self) -> bool {
        false
    }

    /// Returns `true` when the active text prompt expects free-form prose
    /// that can legitimately contain whitespace (the body of a TEXT /
    /// MTEXT / DDEDIT entity, an attribute default value, etc.). For
    /// these prompts the command-line input must let `Space` be typed as
    /// a literal character; for every other prompt `Space` submits the
    /// input the same way `Enter` does.
    ///
    /// Default `false` — single-token prompts (option letters, numeric
    /// radius, block name) do not embed spaces.
    fn wants_text_with_spaces(&self) -> bool {
        false
    }

    /// Called when the user submits text via the command line while `wants_text_input` is true.
    fn on_text_input(&mut self, _text: &str) -> Option<CmdResult> {
        None
    }

    /// Returns `true` when the command is in a selection-gathering phase.
    /// While true, viewport clicks are routed through the normal selection
    /// system (single / box / polygon) instead of the command's point-pick path.
    /// After each completed selection action the host calls `on_selection_complete`.
    fn is_selection_gathering(&self) -> bool {
        false
    }

    /// Called after a selection action completes while `is_selection_gathering` is true.
    /// `handles` is the full set of currently selected entities.
    /// Return `Relaunch` to fire the pending command, or `NeedPoint` to keep gathering.
    fn on_selection_complete(&mut self, _handles: Vec<Handle>) -> CmdResult {
        CmdResult::Cancel
    }

    /// Returns `true` when the command wants object picks via Tangent snap.
    fn needs_tangent_pick(&self) -> bool {
        false
    }

    /// If this command is XATTACH, returns the file path to attach.
    /// Default: None.
    fn xattach_path(&self) -> Option<String> {
        None
    }

    /// If this command needs attribute data injected (ATTEDIT), returns the
    /// INSERT handle awaiting attr initialization; else None.
    fn attedit_pending_handle(&self) -> Option<acadrust::Handle> {
        None
    }

    /// Inject attribute (tag, value) pairs into the command after entity pick.
    fn attedit_set_attrs(&mut self, _attrs: Vec<(String, String)>) {}

    /// Inject attribute definitions (tag, prompt, default_value) for ATTREQ
    /// attr-filling after INSERT point is picked.
    fn attreq_set_attdefs(&mut self, _attdefs: Vec<(String, String, String)>) {}

    /// Returns the INSERT entity built so far (pending attr fill) if this is an
    /// ATTREQ-aware INSERT command waiting for attdef injection.
    /// Called by the host after `AttreqNeeded` to commit the completed Insert.
    fn attreq_take_insert(&mut self) -> Option<acadrust::EntityType> {
        None
    }

    /// Called instead of `on_point` when the command needs a tangent pick
    /// and the snap system found a tangent object.
    fn on_tangent_point(&mut self, obj: TangentObject, hit: Vec3) -> CmdResult {
        let _ = obj;
        self.on_point(hit)
    }

    /// Called by update.rs after `on_entity_pick` to inject the cloned entity into commands
    /// that need to read/modify it (e.g. DIMTEDIT, MLEADERADD, MLEADERREMOVE).
    /// Default: no-op.
    fn inject_picked_entity(&mut self, _entity: acadrust::EntityType) {}

    /// What the command is asking for at this step, used to label the
    /// dynamic-input overlay. Default is a point pick; commands waiting
    /// on a radius/length return `Distance` and angle prompts return
    /// `Angle`.
    fn dyn_field(&self) -> DynField {
        DynField::Point
    }
}

// ── Autocomplete registry ─────────────────────────────────────────────────
//
// Every `impl CadCommand for Foo` module submits the names it answers to
// at compile time via `inventory::submit!`. The command-line autocomplete
// then iterates the resulting collection at runtime — no central list to
// keep in sync.
//
// Non-interactive one-shot dispatch arms (NEW, OPEN, SAVE, …) live in
// `app/commands.rs` and don't have a `CadCommand` impl; they're absent
// from autocomplete by design. Add an explicit `inventory::submit!` next
// to their dispatch arm if you want them surfaced.

pub struct CommandRegistration {
    pub names: &'static [&'static str],
}

inventory::collect!(CommandRegistration);

/// All registered command names, including aliases.
pub fn all_registered_command_names() -> Vec<&'static str> {
    inventory::iter::<CommandRegistration>
        .into_iter()
        .flat_map(|r| r.names.iter().copied())
        .collect()
}
