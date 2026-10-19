# Building GLB protocol

How a building or monument GLB must be authored so that `core/` can drop it onto the cell grid and turn it to face any direction. The first asset built to this protocol is the Fire-Kingdom Shrine (`assets/development/fire_shrine.glb`).

## 1. The frame inside the GLB

| Rule | Value |
|---|---|
| Units | 1 unit = 1 m |
| Up | +Y (glTF convention) |
| East | +X |
| **North = front** | **−Z**. Every building is authored facing north: its entrance, stair or main face is on the −Z side. |
| Origin | the centre of the footprint, at ground level (y = 0) |
| Footprint | whole 5 m cells: `w × d` cells along X × Z when facing north (the shrine is 3 × 2 = 15 × 10 m) |
| Bounds | x ∈ [−w·2.5, w·2.5], z ∈ [−d·2.5, d·2.5], y ≥ 0. Nothing hangs outside the footprint or below the ground. |
| Base | flat at y = 0. A thin ground apron (grass, paving) may cover the footprint so the building couples to plain terrain. |
| Root node | one root node named after the asset, with `extras` `{ footprint_m: [w·5, d·5], cells: [w, d], front: "north", origin: "footprint centre, ground" }` |

Facing is never baked into the file. The game turns the instance instead (section 3), so one GLB serves all four directions.

## 2. Authoring in Blender

Blender is Z up. The glTF exporter with **+Y Up** maps Blender `(x, y, z)` to glTF `(x, z, −y)`. Therefore:

| Blender | three.js / game |
|---|---|
| +X | +X east |
| **+Y** | **−Z north** |
| +Z | +Y up |

So in Blender, **model the front of the building on the +Y side**, keep the origin at the footprint centre on the ground, and keep every vertex inside `x ∈ [−w·2.5, w·2.5]`, `y ∈ [−d·2.5, d·2.5]`, `z ≥ 0`. Blender's own "Front" viewpoint (numpad 1) looks along +Y, so it shows the building's back; numpad Ctrl+1 shows its front.

Export settings (see `assets/development/scripts/shrine_export_glb.py`): GLB, selection = the asset's collection only, `+Y Up` on, apply modifiers, normals and UVs, custom properties (extras) on, WebP textures.

Materials that survive the trip: Principled BSDF base colour texture, roughness, emission (a strength above 1 becomes `KHR_materials_emissive_strength`), and alpha cut-outs through a Math **Round** node on the texture alpha (exported as `alphaMode: MASK`). Procedural nodes do not export; paint them into textures first.

## 3. Placing it in the game (`core/placement/footprint.js`)

```js
const p = placeOnCells({ cx, cz, w: 3, d: 2, facing: 'N' | 'E' | 'S' | 'W' });
// p.rect  = covered cells { x0, z0, x1, z1 } (x1, z1 exclusive); E and W swap w and d
// p.x, p.z = world position of the asset origin (rect centre); p.rotationY = rotation about +Y
const { ok, level } = footprintLevel(grid, p.rect);   // ok when every covered corner is at one height
```

| Facing | rotationY | the asset's front points to |
|---|---|---|
| N | 0 | −Z |
| W | +π/2 | −X |
| S | π | +Z |
| E | −π/2 | +X |

`core/render/asset_view.js` loads the GLB once, clones it per placement and sets `position = (p.x, level, p.z)`, `rotation.y = p.rotationY`. It also draws the covered rect and a front arrow (the panel's **Footprint** toggle) and flickers nodes named `*Flame*`.

## 4. Checking an asset

```
node assets/development/scripts/glb_check.mjs assets/development/fire_shrine.glb 15 10 Shrine_Stair
```

It prints triangles, materials and the bounding box in the three.js frame, and fails unless the base is at y = 0, the box fits the footprint, and the named front node lies on the −Z side. Then open http://127.0.0.1:5174/core/?view=shrine&footprint=1 and use the **N E S W** buttons; the compass rose (bottom right) and the N/E/S/W letters at the map edges show where north is.

`node core/tests/placement.test.mjs` covers the facing table, the swapped rect for E/W, and that the rotated footprint corners stay inside the rect.
