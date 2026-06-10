# Open CAD Studio — File Open & Render Speed Roadmap

This document lists the planned improvements for cutting **file open time**
and **on-screen draw (render) time**. It builds on the already-landed
**Rendering Optimization** work (Phase 1-4); what is left now sits on the
open-time, allocation, and draw-call sides.

Source-scan summary (references):

- File open flow: [`src/io/mod.rs`](src/io/mod.rs)
  (`open_path_with_phase`, `load_file`, `purge_corrupt_entities`).
- Post-open UI work: [`src/app/update.rs:90-192`](src/app/update.rs#L90-L192)
  (`FileOpened` handler — xref resolve, second purge, linetype populate, etc.).
- Derived-cache build: [`src/scene/mod.rs:90-212`](src/scene/mod.rs#L90-L212)
  (`build_derived_caches` — rayon-parallel for hatch / image / mesh).
- Wire tessellation: [`src/scene/mod.rs:1328-1402`](src/scene/mod.rs#L1328-L1402)
  (rayon, zoom-adaptive curve tol).
- Block defn cache: [`src/scene/block_cache.rs:238`](src/scene/block_cache.rs#L238)
  (`build_defn` — single-threaded today; nested expansion is topological).
- Pipeline: [`src/scene/pipeline/mod.rs`](src/scene/pipeline/mod.rs)
  (batched hatch, frustum cull, LOD — Phase 1-4 done).

---

## Phase 1 — File Open Time

**Goal:** measurably halve the wall time between "Open" click and "first
frame" for a 50 MB DWG.

### 1.1 Drop the second `purge_corrupt_entities` ✅ DONE

### 1.3 Single-pass entity walk (parse + purge + cache planning)

`load_file` → `purge` → `build_derived_caches` does three separate
`entities()` walks. A single pass can produce:

- corrupt-entity detection,
- hatch / image / mesh handle lists,
- AABB accumulation for `world_offset` (the world_offset AABB scan is now
  folded into the cache-handle walk — see 2.4; corrupt-detect + hatch/image/
  mesh planning remain a follow-up).

Target: three `O(N)` passes → one.

### 1.4 Memory-mapped file reads (DWG / DXF)

`DwgReader::from_file` / `DxfReader::from_file` likely load the whole file
into RAM with `std::fs::read`. Switching to `memmap2`:

- eliminates the cold-cache read syscall on large files,
- lets the DWG section index be walked on disk (if the acadrust API
  supports it).

**Dependency:** acadrust upstream may need a `from_reader` / `from_slice`
API; add it in our patched fork (`hakanaktt/acadrust`).

### 1.5 Parallelize the acadrust parser (long-term)

acadrust's DWG parser is single-threaded. Section-based parallelism
(header / classes / objects / blocks / entities — independent offsets) is
the biggest unrealized win. Lives in the upstream fork.

**Order:** profile first — is this really the largest slice? Measure with
`puffin`.

### 1.6 Defer raster image decode

[`build_derived_caches`](src/scene/mod.rs#L177-L186) calls
`ImageModel::from_raster_image` for every `RasterImage` entity — pixel
decode happens up front. Wasted if the entity is off-screen. Defer the
decode until **first render** (per-handle lazy `OnceCell`).

### 1.7 File-hash cache (warm re-open)

When re-opening the same file (`(path, mtime, size)` key) keep a disk
snapshot of `CadDocument` + `DerivedCaches` (e.g. `~/.cache/OpenCADStudio/`). Skip
DWG parse entirely. **Win:** most-recently-opened file goes from 1-2 s to
sub-100 ms.

**Risk:** cache invalidation. Stay conservative — load only on exact
`mtime + size` match, otherwise normal parse.

---

## Phase 2 — First-Frame Wire Tessellation

After `FileOpened`, `bump_geometry()` fires; the first frame tessellates
**every** model-space wire. Measurable hitch at ~100 k entities.

### 2.1 Parallelize block-definition build ✅ DONE

### 2.2 Incremental wire cache (delta tessellation) ✅ DONE (render path)

### 2.3 Progressive first render

On the first frame emit a **coarse**-tol wire pass (e.g. 4× the normal
tol); refine to full tol on the second frame. The user sees *something*
within 16 ms; detail snaps in smoothly afterwards.

### 2.4 Merge the world-offset scan into the single-pass walk ✅ DONE

---

## Phase 3 — Per-Frame Render Cost

After Phase 1-4 culling/LOD, what's left is **upload bytes** and **draw
call count**.

### 3.1 Camera-only invalidation: don't re-tessellate

The wire cache key today is `(geometry_epoch, camera_generation)`
([`scene/mod.rs:414`](src/scene/mod.rs#L414)). A camera change should not
force re-tessellation — only zoom-adaptive curve-tol changes need
resampling, and only for curve entities (Arc / Spline / Ellipse). Straight
geometry is camera-invariant.

**Practical:** split the wire cache in two:

- `tess_cache[handle] → WireModel` (rebuild only if tol-invariant content
  changed),
- `frame_visible[handle] → bool` (recomputed per `camera_generation`).

Partials already landed: pan reuse, selection/hover decoupled from
tessellation, per-frame `split_face3d_wires` memoized. **Still open:** the
full camera/selection-from-tessellation split — highlight colour is still
baked into `WireModel.color` across several tessellation sites, so finishing
this needs running-app verification.

### 3.2 Persistent GPU buffer pool — diff upload ✅ DONE (wire pan path)

### 3.3 Single-draw batched wire pipeline (Phase 4-B-style) ✅ DONE

### 3.4 Hardware instancing for repeated block inserts

When the same block defn is `INSERT`-ed N times (every door / window in
an architectural drawing) each instance currently renders as its own wire
set. Hardware instancing:

- upload the block defn vertex buffer once,
- one 4×4 transform row per Insert in an instance buffer,
- `pass.draw_indexed(0..V, 0..N_instances)`.

Typical architectural DWGs: 10-100× faster.

### 3.5 Glyph-stroke batching ✅ DONE

---

## Phase 4 — Allocation & Memory

### 4.1 Swap `HashMap` for `rustc-hash::FxHashMap` ✅ DONE

### 4.2 Arena (`bumpalo`) for transient wire vertices

Tessellation allocates millions of small `Vec<Vec3>`s. A bump arena —
single allocation, frame-end reset — kills the per-vertex malloc cost.
`bumpalo` plays well with rayon (per-thread arenas).

### 4.3 `SmallVec` for small collections

`Polyline.vertices`, `Hatch.boundary_paths`, glyph-stroke lists are
typically < 8-16 entries. `SmallVec<[T; 8]>` skips the heap on the common
case.

### 4.4 Compact entity-ID representation

`Handle` is 8 bytes. 100 k entities → 800 KB just in keys. Hot handle
HashSet / HashMap usage can be flattened to `Vec<u32>` indices plus a
single `FxHashMap<Handle, u32>` translation table — cache-friendlier.

---

## Phase 5 — Profiling Infrastructure (prerequisite)

Don't start any of the above **without measuring first**.

### 5.1 Add `puffin` or `tracy` spans

- `io::open_path_with_phase` → `parse`, `purge`, `caches` spans.
- `Scene::wires_for_block` → `block_cache`, `tess`, `sort` spans.
- `Pipeline::prepare` → `upload`, `cull`, `draw` spans.

Gate behind `debug_assertions` or a `--features profile` flag.

### 5.2 Open-time breakdown log ✅ DONE

### 5.3 Frame-budget HUD ✅ DONE (CPU tess slice)

---

## Priority Order

**Phase 5.1 first** (the remaining profiling span work) — avoids speculation.

Then, measurement-guided, the remaining items:

1. **Phase 1.3 + 1.6** (single-pass walk + lazy image decode).
2. **Phase 3.1** (camera-only invalidation — users pan/zoom constantly; the
   hard part of the tess/selection split is what's left).
3. **Phase 3.4** (block instancing) — biggest render win, highest complexity.
4. **Phase 1.7** (warm cache) — dramatic UX, but invalidation must be
   correct or it creates nasty bugs.
5. **Phase 1.5** (acadrust parallel parse) — hardest, longest-term; only
   worth it if profiling confirms it is the dominant slice.

## Deliberate non-goals (for now)

- **GPU compute culling:** for orthographic 2D CAD the CPU quadtree is
  enough. Already covered by Phase 1-4.
- **Out-of-core entity streaming:** meaningful for 100 MB+ single files;
  typical Open CAD Studio files are not there yet.
- **Multi-frame async tessellation pipeline:** if 2.3 progressive render
  works cleanly, this isn't needed.
