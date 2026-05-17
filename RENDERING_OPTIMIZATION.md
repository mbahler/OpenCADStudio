# Rendering Optimization Roadmap

Phases 1–4 have all landed. This document is kept as a stub so the
git history points somewhere; the optimisations themselves are
documented inline at the call sites:

- **Phase 1** – per-frame LOD ladder, scissor cull (search for
  `Phase 1.*` in `scene/pipeline/mod.rs`).
- **Phase 2** – quadtree spatial index for top-level entities, plus
  draw-time frustum cull for hatches / wipeouts / meshes
  (`Phase 2.1`, `2.2`, `2.3`).
- **Phase 3** – LOD substitution for far / sub-pixel content
  (`Phase 3.2`, `3.3`, `3.4`).
- **Phase 4-B** – batched hatch pipeline. Every hatch is uploaded
  into shared storage buffers (`hatch_batched_gpu.rs`) and drawn in
  one `pass.draw(0..6N, 0..1)`; the per-instance `visibility` buffer
  doubles as a CPU-driven cull mask (sub-pixel + frustum) that the
  vertex shader honours by emitting an out-of-NDC clip position for
  skipped instances. Replaces N bind-group swaps + N draw calls with
  a single draw.

## Deliberate non-goals

- **Phase 4.1 (multi-draw indirect / compute cull)** — iced 0.14
  doesn't expose `Features::MULTI_DRAW_INDIRECT` to widget pipelines.
  Without it, single-draw indirect adds no real saving over the
  current batched draw. Revisit if iced surfaces the feature flag,
  or if H7CAD ever swaps the shader widget for a custom wgpu surface.
- **Phase 4.2 (Hi-Z occlusion)** — pays off only for perspective 3D
  with many overlapping opaque meshes. H7CAD is orthographic
  top-down for every realistic workflow, so the win is theoretical.
- **Per-mutation incremental quadtree updates** — `entity_index_cache`
  rebuilds lazily on `geometry_epoch` change. Full rebuild is ~50 ms
  on a 100 k-entity doc; promote to per-mutation `insert`/`remove`/
  `update` if profiling shows that as a hot spot during heavy edits.
- **Per-hatch viewport scissor in the batched path** — paper-space
  MSPACE viewports can render hatches past the viewport border for
  now. Add a per-instance scissor rect (computed from `vp_scissor`)
  if it shows up as a visible artefact.
