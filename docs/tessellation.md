# Entity tessellation paths

How each acadrust `EntityType` becomes drawable geometry in OpenCADStudio, and
whether it passes through the [truck](https://github.com/ricosjp/truck) modeling
kernel.

There are **two distinct meanings of "through truck"** in the codebase, and they
must not be conflated:

1. **truck B-rep meshing** — the entity is rebuilt as a truck `Shell`/`Solid`
   and triangulated by `MeshableShape::triangulation()`. This is the only path
   that produces filled *mesh* triangles from a kernel.
2. **truck curve topology** — the entity is expressed as a truck `Edge`/`Wire`/
   `Vertex` and sampled into line segments by truck's `ParameterDivision1D`. It
   touches truck *topology* but never triangulates a surface; the output is a
   polyline.

Everything else is **direct**: points/segments/triangles are emitted straight
from the entity's own data without any truck object.

Entry points: `src/scene/convert/tess.rs` (`tessellate_entity`, the per-entity
dispatcher) → `src/scene/convert/tessellate.rs` (`tessellate`, the `TruckObject`
router) and `src/scene/convert/truck_tess.rs` (curve sampling).

## Summary

| Path | Entities |
|---|---|
| **truck B-rep mesh** (Shell → `triangulation`, with a direct per-surface fallback) | `Solid3D`, `Region`, `Body`, `Surface` |
| **truck curve topology** (Edge/Wire → `ParameterDivision1D` sampling) | `Line`, `Arc`, `Circle`, `Ellipse`, `Spline`, `LwPolyline`, `Polyline`, `Polyline2D`, `Polyline3D`, `Point` — *non-thick variants only* |
| **direct** (no truck — segments/triangles emitted directly) | everything else |

> The Model/Design-tab primitives (BOX, SPHERE, CYLINDER, CONE, WEDGE, TORUS,
> PYRAMID, extrude/revolve/loft/sweep) are also true truck B-rep meshes
> (`truck_tess::tessellate_solid`), but they are created as `Solid3D`
> placeholders carrying a truck B-rep, not as their own `EntityType`.

The only call site of `shell.triangulation()` for an imported entity is
`acis_to_truck::tessellate_sat_truck`. The four ACIS solids dispatch through
`solid3d_tess::tessellate_acis`, which tries that truck kernel first and falls
back to a bespoke per-surface LOD sampler (`tessellate_sat_lods`) when truck
cannot rebuild a face — so their path is **truck-with-direct-fallback**.

Curved-surface tessellation density is radius-relative (`CURVE_REL_TOL`, a
fraction of the surface radius) so a cylinder's facet count matches the
circle/arc wire tessellation instead of exploding on large radii.

## Full table

| Entity | Output | Path | Notes |
|---|---|---|---|
| Arc | wire | direct (truck curve topology) | non-thick: truck `Edge` (`builder::circle_arc`) sampled via `ParameterDivision1D`; thick: direct `Lines` |
| AttributeDefinition | wire | direct | routes through the Text/MText LFF glyph stroke pipeline |
| AttributeEntity | wire | direct | same as AttributeDefinition; values supplied per Insert |
| Block | none | n/a | block-definition sentinel; not tessellated (referenced via Insert) |
| BlockEnd | none | n/a | block-definition end marker; no output |
| Body | mesh | truck-with-direct-fallback | 3D ACIS body → truck `Shell` → `triangulation`; fallback per-surface sampler |
| Circle | wire | direct (truck curve topology) | non-thick: truck `Wire` of two half-circle edges; thick: direct `Lines` |
| Dimension | wire | direct | baked-block path recurses on `D###` sub-entities; synthesis path emits lines/arrows/LFF text |
| Ellipse | wire | direct (truck curve topology) | truck `Wire`/`Edge` over a `BSplineCurve`, `ParameterDivision1D` sampled |
| Face3D | both | direct | edge `Lines` + direct fan-triangulated `fill_tris`; no truck B-rep |
| Hatch | both | direct | boundary outline not emitted to the wire set (#131 OOM); fill rasterized on GPU |
| Insert | wire | direct | expands block children and tessellates each via its own path; XCLIP filter applied |
| Leader | wire | direct | leader path + arrowhead + landing, direct `Lines` |
| Line | wire | direct (truck curve topology) | non-thick: truck `Edge` (`builder::line`); thick: direct `Lines` |
| LwPolyline | wire | direct (truck curve topology) | `plinegen` true: truck `Contour` with bulge arcs; else direct `SegmentedLines` |
| Mesh | both | direct | SubD mesh: edge `Lines` + direct fan-triangulated `fill_tris` |
| MLine | wire | direct | spine + offset lines + caps, direct `Lines` |
| MText | wire | direct | wrap-aware multi-line LFF glyph layout; inline formatting codes |
| MultiLeader | wire | direct | leader + landing + LFF text + frame + fill |
| Ole2Frame | wire | direct | bounding rectangle + diagonal cross |
| Point | wire | direct (truck topology) | truck `Vertex` → cross marker sized by PDSIZE |
| PolyfaceMesh | both | direct | face list: closed-polyline edges + direct fan-triangulated `fill_tris` |
| PolygonMesh | both | direct | M×N grid wireframe + direct fan-triangulated `fill_tris` |
| Polyline | wire | direct (truck curve topology) | heavy 3D polyline; truck `Contour` + bulge arcs |
| Polyline2D | wire | direct (truck curve topology) | 2D polyline with bulge; truck `Contour` |
| Polyline3D | wire | direct (truck curve topology) | linear-edge truck `Contour`, no bulge/thickness |
| RasterImage | wire | direct | boundary rectangle / clipping polygon |
| Ray | wire | direct | two-point `[base, base + dir×1e6]`, no sampling |
| Region | mesh | truck-with-direct-fallback | 2D planar ACIS body; same truck path as Solid3D |
| Seqend | none | n/a | vertex-sequence terminator sentinel; no output |
| Shape | wire | direct | small diamond marker at the insertion point |
| Solid | wire | direct | 2D SOLID: four quad edges as direct `Lines` |
| Solid3D | mesh | truck-with-direct-fallback | 3DSOLID: parse SAT/SAB → truck `Shell` → `triangulation`; fallback sampler |
| Spline | wire | direct (truck curve topology) | NURBS/BSpline truck `Edge`, `ParameterDivision1D` sampled |
| Table | both | direct | cell fills (`fill_tris`) + LFF cell text + grid lines |
| Text | wire | direct | LFF-font stroked glyph polylines |
| Tolerance | wire | direct | feature-control frame grid + per-cell LFF symbols |
| Underlay | wire | direct | boundary rectangle of the PDF/DWF reference |
| Unknown | none | n/a | unrecognized-entity sentinel; no output |
| Viewport | wire | direct | content-viewport frame rectangle (sheet viewport skipped) |
| Wipeout | wire | direct | boundary rectangle / clipping polygon |
| XLine | wire | direct | three-point infinite line, no sampling |
