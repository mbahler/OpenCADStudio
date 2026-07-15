// Persistent per-entity wire instance arena (native, behind OCS_WIRE_GPU_PATCH).
//
// The normal wire path re-emits EVERY wire into a fresh, tightly-packed instance
// buffer whenever the resident set's content id changes — so moving one line on a
// million-wire drawing re-uploads all million instances. This arena instead keeps
// one persistent instance buffer (plus its shared WireConst storage) with per-
// entity *slabs*, so an in-place edit is a `queue.write_buffer` of just that
// entity's slab.
//
// Scope (v1): IN-PLACE patches only. A move / rotate / scale / colour change keeps
// an entity's slab, its instance count, and — crucially — the block's entity count,
// so `draw_depth_map`'s per-entity z-bias (normalised by that count) is unchanged
// and every other slab stays correct. `patch` returns false (⇒ the caller does a
// full batched rebuild) for anything else: an Add or Remove (which re-scales every
// entity's z-bias), a segment-count change, or a set that isn't a single scissor-
// free batch (mesh-edge fills, paper viewport scissor). Because a full rebuild is
// always the fallback, correctness never rides on the fast path.

#![cfg(not(target_arch = "wasm32"))]

use super::wire_gpu::{emit_wire_native, wire_draw_depth, WireConst, WireGpu, WireInstance};
use crate::scene::model::wire_model::WireModel;
use crate::scene::ChangeKind;
use acadrust::Handle;
use iced::wgpu;
use rustc_hash::FxHashMap;

struct Slab {
    inst_off: u32,
    inst_len: u32,
    const_off: u32,
    const_len: u32,
}

pub struct WireArena {
    inst_buf: wgpu::Buffer,
    inst_tail: u32,
    const_buf: wgpu::Buffer,
    const_bind_group: std::sync::Arc<wgpu::BindGroup>,
    slabs: FxHashMap<Handle, Slab>,
}

fn handle_of(w: &WireModel) -> Option<Handle> {
    crate::scene::Scene::handle_from_wire_name(&w.name)
}

/// True if the whole set is arena-eligible: a single batch that draws with no
/// viewport scissor — no mesh/solid fill (which forces the multi-batch draw-order
/// split) and no per-wire scissor (paper content viewports, which the arena's
/// scissor-free wrapper would fail to clip). Model tiles qualify; scissored paper
/// viewports fall back to the normal batched path.
pub fn is_arena_eligible(wires: &[WireModel]) -> bool {
    wires
        .iter()
        .all(|w| w.fill_tris.is_empty() && w.vp_scissor.is_none())
}

/// handle → wire-slot index for the selection / text-highlight overlays, built
/// from the resident Vec (independent of the arena's slab layout, exactly like
/// the batched path's index).
pub fn build_handle_index(wires: &[WireModel]) -> std::sync::Arc<FxHashMap<u64, Vec<u32>>> {
    let mut index: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
    index.reserve(wires.len());
    for (idx, w) in wires.iter().enumerate() {
        if let Ok(h) = w.name.parse::<u64>() {
            index.entry(h).or_default().push(idx as u32);
        }
    }
    std::sync::Arc::new(index)
}

/// Group `wires` (draw-order sorted, entity-contiguous) into per-handle ranges.
fn handle_ranges(wires: &[WireModel]) -> Option<Vec<(Handle, usize, usize)>> {
    let mut out: Vec<(Handle, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < wires.len() {
        let h = handle_of(&wires[i])?;
        let mut j = i + 1;
        while j < wires.len() && handle_of(&wires[j]) == Some(h) {
            j += 1;
        }
        out.push((h, i, j));
        i = j;
    }
    Some(out)
}

fn build_const_bind_group(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    buf: &wgpu::Buffer,
) -> std::sync::Arc<wgpu::BindGroup> {
    std::sync::Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wire_arena.const.bg"),
        layout: bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    }))
}

fn alloc_inst(device: &wgpu::Device, cap: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wire_arena.ibuf"),
        size: (cap as u64) * std::mem::size_of::<WireInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn alloc_const(device: &wgpu::Device, cap: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wire_arena.cbuf"),
        size: (cap as u64) * std::mem::size_of::<WireConst>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl WireArena {
    /// Build a fresh arena from the full resident set, or `None` if it isn't
    /// single-batch (has mesh-edge fills) or a wire is unnamed — in which case
    /// the caller keeps using the normal batched path.
    pub fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        wires: &[WireModel],
        depth_map: &FxHashMap<u64, f32>,
        const_bgl: &wgpu::BindGroupLayout,
    ) -> Option<Self> {
        if !is_arena_eligible(wires) {
            return None;
        }
        let ranges = handle_ranges(wires)?;

        let mut instances: Vec<WireInstance> = Vec::new();
        let mut consts: Vec<WireConst> = Vec::new();
        let mut slabs: FxHashMap<Handle, Slab> = FxHashMap::default();
        for (h, i, j) in ranges {
            let inst_off = instances.len() as u32;
            let const_off = consts.len() as u32;
            for w in &wires[i..j] {
                let wire_id = consts.len() as u32;
                let dd = wire_draw_depth(w, depth_map);
                let (mut insts, cst) = emit_wire_native(w, wire_id, w.color, dd);
                instances.append(&mut insts);
                consts.push(cst);
            }
            slabs.insert(
                h,
                Slab {
                    inst_off,
                    inst_len: instances.len() as u32 - inst_off,
                    const_off,
                    const_len: consts.len() as u32 - const_off,
                },
            );
        }

        // In-place-only: allocate exactly the set's size (no append headroom).
        let inst_tail = instances.len() as u32;
        let inst_buf = alloc_inst(device, inst_tail.max(1));
        let const_buf = alloc_const(device, (consts.len() as u32).max(1));
        if inst_tail > 0 {
            queue.write_buffer(&inst_buf, 0, bytemuck::cast_slice(&instances));
        }
        queue.write_buffer(&const_buf, 0, bytemuck::cast_slice(&consts));
        let const_bind_group = build_const_bind_group(device, const_bgl, &const_buf);

        Some(Self {
            inst_buf,
            inst_tail,
            const_buf,
            const_bind_group,
            slabs,
        })
    }

    /// Apply the changed handles to the arena in place. Returns false (⇒ the
    /// caller does a full batched rebuild) for any change that isn't an in-place
    /// slab overwrite — see the module header.
    pub fn patch(
        &mut self,
        queue: &wgpu::Queue,
        changes: &[(Handle, ChangeKind)],
        wires: &[WireModel],
        depth_map: &FxHashMap<u64, f32>,
    ) -> bool {
        if !is_arena_eligible(wires) {
            return false;
        }
        let Some(ranges) = handle_ranges(wires) else {
            return false;
        };
        let range_of: FxHashMap<Handle, (usize, usize)> =
            ranges.into_iter().map(|(h, i, j)| (h, (i, j))).collect();

        let inst_sz = std::mem::size_of::<WireInstance>() as u64;
        let const_sz = std::mem::size_of::<WireConst>() as u64;

        // Only IN-PLACE updates are safe to patch: an entity keeps its slab and
        // its instance/const count (a move / rotate / scale / colour change).
        // Add / Remove / any count-changed edit alter the layout AND — because
        // draw_depth_map normalises each entity's z-bias by the block's entity
        // count — re-scale EVERY entity's draw order, which we can't patch slab-
        // by-slab. Those fall back to a full rebuild (return false), where the
        // batched path re-emits everything with the current depth map.
        for &(h, kind) in changes {
            if matches!(kind, ChangeKind::Added | ChangeKind::Removed) {
                return false;
            }
            let Some(&(i, j)) = range_of.get(&h) else {
                return false; // Modified but now absent (hidden / not in set)
            };
            let Some(&Slab {
                inst_off,
                inst_len,
                const_off,
                const_len,
            }) = self.slabs.get(&h)
            else {
                return false; // no existing slab to overwrite
            };
            let run = &wires[i..j];

            // Emit into the entity's existing const slots (wire_id = const_off + k).
            let mut insts: Vec<WireInstance> = Vec::new();
            let mut csts: Vec<WireConst> = Vec::new();
            for w in run {
                let wire_id = const_off + csts.len() as u32;
                let dd = wire_draw_depth(w, depth_map);
                let (mut wi, c) = emit_wire_native(w, wire_id, w.color, dd);
                insts.append(&mut wi);
                csts.push(c);
            }
            if insts.len() as u32 != inst_len || csts.len() as u32 != const_len {
                return false; // segment count changed ⇒ not in place ⇒ rebuild
            }
            if inst_len > 0 {
                queue.write_buffer(
                    &self.inst_buf,
                    inst_off as u64 * inst_sz,
                    bytemuck::cast_slice(&insts),
                );
            }
            queue.write_buffer(
                &self.const_buf,
                const_off as u64 * const_sz,
                bytemuck::cast_slice(&csts),
            );
        }
        true
    }

    /// One draw batch wrapping the persistent instance buffer. `instance_count`
    /// is the whole tail (tombstones included — they draw nothing).
    pub fn wire_gpus(&self) -> Vec<WireGpu> {
        if self.inst_tail == 0 {
            return vec![];
        }
        vec![WireGpu {
            instance_buffer: self.inst_buf.clone(),
            instance_count: self.inst_tail,
            vp_scissor: None,
            is_3d_mesh_edge: false,
            const_bind_group: Some(self.const_bind_group.clone()),
        }]
    }
}
