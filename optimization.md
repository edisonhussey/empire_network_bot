Asset Construction & Optimization Ledger
Project: Stylized medieval strategy world (Three.js / Blender / GLB)
Purpose: A decision framework for Claude to select how to construct each visual asset, rather than approximating every detail with separate 3D geometry.
Art direction: Beautiful Goodgame Empire-inspired buildings, subtle Japanese illustrative influence, coherent warm pastoral terrain, stylized readable silhouettes, and genuine 3D camera movement.
Governing principle
Spend polygons on silhouette, depth, and parallax—not on detail that can be represented in a texture. Work from primary form (recognizable outline) to secondary form (depth and overlaps) to tertiary detail (painted texture). Preserve genuine 3D on visible structural parts and use image-based detail where it survives camera motion.

A texture is not automatically cheaper than geometry: test GPU overdraw, alpha fill rate, draw calls, texture memory, shadow passes, and triangle count. Crossed cards can reduce 30k triangles to around 2k in an asset-specific case, but those figures are not universal budgets or guarantees.
Technique registry
ID	Technique	How it works	Best uses	Failure mode / safeguard
T01	Crossed foliage cards	Two to six intersecting planes (X, star, or irregular radial arrangement) use painted foliage textures with alpha cutouts.	Bushes, leafy tufts, distant tree crowns	Thin side views, repetitive silhouettes, alpha overdraw. Inspect at 0°, 45°, 90° and from above.
T02	Branch cards on a real trunk	Real low-poly trunk and major branches with textured two-sided branch clusters arranged in genuine 3D.	Pine/spruce at mid range; deciduous trees	Flat fern/frond look. Vary cluster depth, shape, tilt, length, UVs; preserve foliage volume.
T03	Volumetric foliage shells	Simplified polygonal clump meshes with gradients or painted detail; small concealed underside masses.	Close-up hero trees, stylized shrubs	Too many plates/spikes; sculpt a few thick masses first.
T04	Billboard impostor	One or two planes displaying a pre-rendered image, possibly view-dependent.	Very distant trees, castles, rock clusters	Fails when camera drops/rotates; switch based on projected size and view.
T05	Multi-angle impostor atlas	Bake renders from 8–16 directions, selecting/cross-fading depending on view angle.	Distant complex castles or groves with camera orbit	Angle popping, atlas VRAM; keep proper near-field 3D asset.
T06	Texture atlas and shared materials	Consolidate stone, timber, roofs, banners, foliage into small atlases and repeated materials.	Almost all assets	Bleeding or incompatible UVs; margin/padding + mipmap tests.
T07	Normal/ambient-occlusion baking	Bake high-poly creases and details onto low-poly textures/maps.	Roof tiles, stone seams, carved doors, carved trim	Tangent-space errors or Blender-to-glTF mismatch; export and verify.
T08	Geometry for silhouettes	Spend actual polygons on roof eaves, towers, crenellations, bridges, prominent rocky contours, large boughs.	Near and medium distance assets	Atomic overmodelling; merge invisible micro-details into texture.
T09	Instancing	Reuse meshes/materials with per-instance transforms/colors in THREE.InstancedMesh.	Trees, stones, props, wheat, posts	One huge batch can defeat individual culling; group by spatial chunk and type.
T10	LOD / hysteresis	Switch between dense mesh, simplified mesh, and impostor based on projected screen size, with hysteresis/cross-fading where possible.	Nearly every repeated asset	Popping; test continuous camera motion and appearance at thresholds.
T11	Vertex colour painting	Large low-frequency value/hue variation interpolated across mesh.	Terrain, rocks, painted roofs, simple trees	Random speckle/noise; use large coherent patches.
T12	Spline ribbons + flowing shader	River mesh generated from smooth centreline and variable-width banks; UVs follow arc length; shader animates highlights downstream.	Rivers, streams	Rigid shapes, stretched UVs, overly flashy water. Keep banks separate, preserve curves.
T13	Decal/overlay details	Blend soft path edges, dirt, footprints, bank wetness, moss or cracks through textures or overlay geometry.	Terrain and architecture	Z-fighting, overdraw, excessive draw calls; atlas or shader-blend repeated decals.
T14	Modular kits	Walls, towers, roofs, stairs, parapets as reusable GLB modules with consistent metres, pivots and UV/material standards.	Castles, gates, houses, forts	Modular repetition; a few distinctive hero assemblies and controlled variation.
T15	Painted alpha ground tufts	2–3 intersecting grass/flower cards or small simple blades sharing an atlas.	Scattered grasses, wheat accents, flowers	Alpha overdraw at scale; use sparse instanced patches and reduce draw distance.
T16	Shadow/detail baking	Bake static occlusion where appropriate, use one soft dynamic sun and limited shadow casters.	Buildings, rocks, forests	Double-darkening; do not bake directional shadows that contradict moving light.


Decision rules (choose representation before modelling)
1. Can the object rotate into close camera view? Yes: give it genuine 3D structural form. No / tiny on screen: allow impostors or cards.
2. Does geometry change the outer silhouette or cast an important shadow? Yes: mesh it. No: bake/paint it.
3. Does it repeat hundreds of times? Yes: use atlas/shared material + instancing + LOD + spatial chunking.
4. Is the material mainly foliage, grass, or needles? Evaluate crossed cards / branch cards and alpha cutoff against thick clump meshes; prefer the version with better total GPU cost and view stability.
5. Is the asset architecture? Solid 3D forms first; bake stone courses, roofing micro-details and weathering.
6. Is the surface continuous and moving? Use shared procedural geometry and shaders (rivers), not individually modelled objects.
7. Does it contain a large transparent surface? Measure overdraw and shadow cost, use alpha clipping if visually acceptable, and compare with opaque clump geometry.
Asset-specific presets
Asset	Close camera	Standard map camera	Far camera
Pine 9 m	Low-poly real trunk + volumetric branch clumps + branch cards	Branch cards on trunk, 3–5 meaningful overlapping clumps per tier	2–4 crossed cards or impostor
Deciduous tree	Real low-poly branches + thick crown clumps	Clump meshes and sparse crossed cards	Billboard / view atlas
Bush	3D core with textured outer cards	2–4 crossed cards or 3–5 clumps	Single crossed pair / omit
Grass/flowers	Small grouped instanced geometry + atlas	Sparse alpha tufts	Ground colour only
Rock resource node	3D silhouette with a few facets and vertex colour	Same low-poly mesh instanced	Simplified rock / impostor
Castle/robber baron	Full modular 3D + baked detail	LOD solid model	Simplified mesh or multi-view impostor
Fire Kingdom fortress	True 3D roof curvature, walls, stairs; baked stone/tiles	Reduced geometry + shared atlas	Multi-angle impostor if it retains angle fidelity
River	Curved terrain banks + animated ribbon shader	Same ribbon shader, reduced samples	Simplified coloured ribbon / lower shader cost
Farm/hay	3D building silhouettes + instanced crop rows, small card accents	Reduced rows & atlas	Painted land patch and building proxy


Authoring standards
- Blender/GLB units: 1 unit = 1 metre; Y up in Three.js, XZ ground, front +Z; origin ground-contact centre. Export transforms and validate bounding box. Blender typically uses Z up; convert correctly on export/import.
- Maintain editable .blend masters and versioned exported .glb files. Keep separate source textures and procedural generator scripts.
- Do not assume Blender procedural nodes will export to glTF: bake to supported textures or translate intentionally; verify in Three.js.
- Alpha materials: compare alphaTest / MASK versus blending; masked foliage often avoids transparency sorting but still incurs cutout fill and can shimmer.
- Treat two-sided cards, shadow casting, mipmaps and alpha cutoff deliberately. Avoid unnecessarily double-sided every mesh.
- Distant views are part of design, not only optimization. The asset must read at realistic on-map screen size.
Initial trial budgets (measure, do not treat as mandates)
- Hero pine: ~700–2,000 triangles, if dense close-up foliage is necessary.
- Standard pine: ~250–900 triangles or a few layered cards; count both geometry and alpha cost.
- Distant pine: ~2–12 triangles per crossed-card/impostor form depending on number of planes; atlas/detail cost remains.
- Shrub: ~4–40 triangles if cards suffice; 50–250 for solid clumps.
- Rocks: 30–250 triangles for ordinary props; larger nodes may require more.
- Fortress/castle: derive budget from screen size, LOD, shading and frequency of use. Do not hardcode universal budgets from a concept painting.
Validation protocol: do not call an asset 'optimized' without evidence
1. Capture reference and current asset using identical front, 3/4, side, high-angle views, including an actual gameplay zoom.
2. Measure triangle count, draw calls, texture/VRAM use, material count, GPU frame time and alpha overdraw where available.
3. Test stable 3D camera orbit; inspect crossed cards at edge-on views and their shadows.
4. Check silhouette, depth/parallax, material cohesion, and flicker on zoom and pan.
5. Export GLB, load in Three.js, and repeat with production lighting and tone mapping.
6. Compare two or more construction methods before committing the asset to the shared library.
7. Document method, parameters, observed quality, observed cost, and decision in the ledger below.
Repeatable experiment record
asset: pine_tall_A1
visual_target: strategy_map_reference
candidate_methods:
  - trunk_plus_volumetric_clumps
  - trunk_plus_alpha_branch_cards
  - crossed_cards_only
camera_tests: [front, three_quarter, side, high_angle, gameplay_zoom]
metrics:
  triangles: TBD
  draw_calls: TBD
  texture_mb: TBD
  gpu_ms_100_instances: TBD
  visual_quality_1_to_10: TBD
chosen_method: TBD
reason: TBD
known_limitations: TBD
Instructions for Claude / Blender MCP
Before creating an asset, consult this ledger and state which construction technique IDs are being used and why. Prototype the simplest feasible representation first. Do not simulate every leaf, stone joint, nail, plank or roof tile as separate 3D primitives. Preserve high-value 3D silhouette and parallax; bake other details or place them on appropriate cards. Create at least one visual comparison render and, when making a performance claim, provide measured GPU and draw-call results rather than estimates alone. Implement the result in source .blend or procedural scripts, export GLB, and validate the imported appearance in Three.js.
Video note
User-provided inspiration: https://www.youtube.com/watch?v=AlMLFkN79dA (time ~4:09). The specific video demonstration could not be independently inspected during preparation; the crossed-plane foliage technique described above is based on the user's summary, not a verified transcription of the video.

Ledger entry — pine_tall_A1 (2026-10-19)
asset: pine_tall_A1 (strategy_pine_hd_refined -> strategy_pine_lp, interactive_world/assets/development)
methods: T02 branch cards on a real trunk + T06 atlas + T07 bake + T11 vertex shading; T01 crossed cards only for the crown core
how: the 769 curved HD feather cards (32 tris each) were rendered in Cycles as 20 fan-cluster variants into one 2048 atlas (albedo x softened AO, tangent normal, emissive). Each HD tier's feathers are k-means grouped by tip position (4 per card, 2-3 in the crown) and replaced by one arched, V-folded 8-tri card using the variant whose dark/light mix matches the group; yaw/roll/pitch jitter and random mirroring break repetition. Vertex colour darkens card roots (inner volume) and 15 upward crossed cards hide the trunk inside the crown. Trunk decimated 672 -> 79.
metrics: tree triangles 25,280 -> 1,887 (foliage 1,808 + trunk 79); tree draw calls 2 (1 foliage material, 1 bark); pebbles 6,144 -> 288; atlas 3 x 2048^2 (GLB embeds WebP, tree GLB 0.89 MB)
camera_tests: hero, eye-level side, high gameplay, top-down in Blender (previews/pine_compare_*.png); GLB loaded in three.js with ACES + shadows (pine_viewer.html)
known_limitations: no GPU frame-time measured yet; baked fans make dark feathers read as slightly bigger patches than in the HD version; alpha-cut cards still cost overdraw at scale
rebuild: scripts/pine_bake_atlas.py -> pine_build_lowpoly.py (in Blender, HD file open) -> pine_export_glb.py (headless)

Ledger entry — pine_tall_A1 crossed-plane pass (2026-10-19, after maple reference)
methods: T01 crossed cards on a T08 3D trunk + main-branch skeleton, T06 atlas, T07 bake, T11 vertex shading
how: same 20 baked clusters, now in a 5x8 atlas with a top view and an edge-on view of each. Every cluster is a flat card (top view) plus a perpendicular card along its length (edge-on view) = 4 tris; 167 boughs below the crown get a 3-sided bark cone (3 tris). Autumn palette is a per-tile hue remap of the green bake onto the maple reference band (hue 15-27 deg, sat 0.6-0.88).
metrics: tree triangles 25,280 -> 1,444 (foliage 864 + trunk/branches 580); draw calls 2; GLB 1.3 MB per palette
files: strategy_pine_lp.blend (first-pass version kept by the user in development/backup/), pine_cross_autumn.glb, pine_cross_green.glb
rebuild: pine_cross_build.ensure_hd() -> pine_cross_bake.py -> pine_cross_build.build("autumn") -> drop_hd() -> pine_export_glb.py
known_limitations: two planes per cluster, not three like the maple reference (a pine bough seen end-on is a thin arc); darker under ACES in three.js than in Blender

Ledger entry — fire_shrine (2026-10-19)
asset: Fire-Kingdom Shrine, 15 x 10 m (3 x 2 cells), ~14.6 m tall, faces north (interactive_world/assets/development/fire_shrine.blend -> fire_shrine.glb)
methods: T08 geometry for silhouettes (curved hip roof with upturned corners, front gable, stairs, banners, 34 lens-section crest blades), T06 one 1024 trim atlas for every coloured detail, T13 painted panels / carpet / emblems / door instead of geometry, T14 modular parts with shared meshes (4 braziers, 4 posts, 2 banner posts, 5 flames), T01 crossed cards on a small opaque core for 42 bushes and 46 grass tufts
how: everything is generated by scripts/shrine_textures.py (numpy-painted tiling stone, roof and grass, the trim atlas, a maple / shrub / tuft alpha atlas) and scripts/shrine_build.py (one function per part); tiling stone and roof use world-scale UVs, flames are emissive meshes on the atlas flame gradient
metrics: 8,817 triangles (crest ~2.9k, roof module ~2.1k, foliage ~1.5k, flames 5 x 204); 7 materials (stone wall, stone floor, roof, trim, grass, flame emissive, foliage MASK); 6 WebP images; GLB 0.62 MB; 23 nodes, 11 meshes
camera_tests: Blender hero, rear 3/4, east side, ortho top (previews/shrine_*.png); GLB in core/ (three.js, ACES, shadows) on the middle pad, turned N / E / S / W
known_limitations: no GPU frame-time measured; the roof underside is only modelled near the eave (grazing views from below can see into it); with the prototype stage's south-west sun the north-facing front is in shade
rebuild: shrine_textures.build_all() -> shrine_build.build() -> shrine_export_glb.py; check with scripts/glb_check.mjs

Ledger entry — terrain_kit (2026-10-20)
asset: terrain kit for the 1 m / 0.2 m tile terrain (interactive_world/assets/development/terrain_kit.blend -> terrain_kit.glb), 32 pieces: 5 tile shapes, 6 cliff/corner modules, 2 banks, grass lip, log step, stepping stones, 4 rocks, 4 trees, 4 bushes, 4 ground-cover cards
methods: T08 faceted cliff columns, boulder steps and rocks; T11 vertex colour (AO at the foot, moss on top faces, crown light top-to-bottom); T03 broadleaf crowns as displaced low-poly leaf masses with tiling leaf textures + T01 leaf cards on their surface (outward custom normals, so the crown lights as one volume); T15 crossed grass/flower/reed cards with up-pointing normals; T06 one 2048x1024 foliage atlas; T09 every piece instanced in three.js (one InstancedMesh per primitive)
how: scripts/terrain_kit_textures.py paints every texture with numpy (seamless FFT noise, Worley rock plates, stamped leaves and blades); scripts/terrain_kit_build.py builds each piece procedurally (one function per piece); the game dresses tile edges and scatters props from data (core/tiles/dress.js) and merges tile tops into smooth-shaded chunks
metrics: kit 5,550 triangles in total; trees 230-820, bushes 260, rocks 80-400, cliff modules 95-240, cards 6-10; 8 materials, GLB 1.7 MB. Example island (48 x 40 tiles, ~4,500 instances): 61k tile-top + 1k wall triangles + ~158k instanced piece triangles, 65 draw calls, scene build ~90 ms (renderer.info on this Mac)
camera_tests: Blender lineups (previews/terrain_kit_terrain.png, terrain_kit_plants.png); three.js with ACES + shadows in hero, shrine, waterfall, slope path, rocky rise and top-down grid views (interactive_world/architecture/renders/tiles_*.png)
known_limitations: no GPU frame time measured; no LOD or spatial chunking of instances yet; tall cliffs stretch one module vertically; grass cards cost alpha overdraw when the camera is low
rebuild: terrain_kit_textures.build_all() -> terrain_kit_build.build() -> terrain_kit_export_glb.py; check with scripts/kit_check.mjs
