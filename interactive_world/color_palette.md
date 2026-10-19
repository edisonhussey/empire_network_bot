Art Direction — Warm Pastoral Medieval Strategy Map
Purpose
Define a coherent, replicable art style for a browser-based fully 3D medieval strategy game built with Three.js. The default presentation is an elevated, map-like strategy camera, but all important objects must work when the camera tilts, orbits, zooms, and transitions into cinematic perspectives.
Primary reference: the user's older illustrated landscape map. Preserve its warm, sunlit atmosphere, pale blue water and distance haze, economical shapes, and uncluttered readability. Retain the previously developed robber-baron towers, rocky outcrops, bridges, and organic river geometry, but render them in this unified style.
Style name: Warm Pastoral / Atmospheric Medieval Illustration.
Guiding principle: beauty from silhouette, composition, palette, and lighting—not dense geometry, procedural micro-noise, or photorealistic texture.
Visual Identity
- Warm-dominant landscape: wheat, straw, faded gold, muted ochre, parchment, and earth brown. Avoid conventional saturated-green grass as the main ground colour.
- Cool atmospheric counterbalance: light, desaturated blue/teal for river, haze, and distant ridgelines. The blues should be gentle, not bright cyan.
- Quiet vegetation: subdued pine green and blue-grey silhouettes. Trees should read as simple clustered masses, not finely detailed foliage.
- Selective architecture detail: recognizable castle and robber-baron tower silhouettes with simple stone, timber, roofs, and battlements. Buildings may have more geometry than surrounding terrain.
- Japanese-illustration influence: restrained forms, deliberate negative space, subtle painterly colour transitions, and atmospheric depth. No requirement for Japanese-style buildings.
- Grounded medieval world: no glow, magical crystals, supernatural effects, or exaggerated fantasy lighting.
- Full 3D geometry: stylized materials and painterly shading are an artistic choice, not a substitute for real 3D terrain or assets.
Colour Palette
These HEX codes are art-direction approximations informed by the reference, not certified pixel-perfect samples or original artist specifications. Tune them against in-engine lighting and colour management.
Landscape & Earth — dominant
Role	Hex	Usage
Sunlit straw	#F1C886	Warm illuminated grass and dry highlights
Dry grass	#CFB279	Primary terrain base
Meadow ochre	#B29358	Large muted ground variation
Harvest gold	#C9914E	Farmland, wheat, sunny terrain accents
Warm earth	#A06A38	Soil, cut banks, dry path edges


Water, Sky & Atmosphere — cool contrast
Role	Hex	Usage
Warm sky	#F4FBDF	Bright warm atmospheric light
Horizon haze	#DDECDA	Pale horizon transition
Pale blue mist	#B1CAC7	Atmospheric depth, distant cool shift
River blue	#7B9C9E	Main muted water hue
Deep blue	#5D7C85	Deep channels and shaded water


Forest & Mountains — restrained depth
Role	Hex	Usage
Distant ridge	#BED0C5	Hazy mountains
Misty mountain	#A1B4AE	Mid-distance ridges
Near ridge	#799092	Closer blue-grey hills and foliage
Pine midtone	#5D6765	Primary tree masses
Deep forest	#424C4E	Foliage shadows and dark silhouettes


Architecture & Accents — limited focal detail
Role	Hex	Usage
Sunlit stone	#E5D0A6	Main stone faces
Stone shadow	#917F6D	Recesses and cooler stone faces
Roof highlight	#DBAC78	Warm roof edges and timber highlights
Terracotta	#AB5532	Sparing roof/banner accents
Dark timber	#574244	Doors, beams, structural contrast


Suggested scene colour balance
A composition target, not a measured pixel breakdown:
- ~55% warm terrain
- ~18% subdued vegetation
- ~12% water and atmospheric blues
- ~10% stone and architecture
- ~5% accents
These proportions will change with the camera view and biome. Do not impose them as literal shader limits.
Material and Rendering Rules
1. Use broad, low-frequency colour variation across terrain. Avoid high-frequency speckles, excessive grass decals, repeated tiny rocks, and noisy procedural textures.
2. Prefer small, cohesive material sets and vertex colours or compact painted texture atlases for buildings. Preserve large colour regions.
3. Use soft directional sunlight, consistent shadow direction, subtle ambient occlusion/contact shadows, and warm/cool tonal separation.
4. Create distance depth through restrained atmospheric fog and desaturation: warm foreground → ochre midground → pale blue-grey distance.
5. Avoid harsh black outlines, aggressive bloom, glossy plastic, saturated cyan water, and hyperreal PBR roughness detail.
6. Maintain consistent colour management and tone mapping across all assets; validate swatches in the rendered scene, not just on a flat palette sheet.
7. Let landmarks carry fine detail. Keep terrain, forests, and resource props visually quieter.
Asset Design Rules
Trees
- 3–5 reusable silhouettes for the first iteration; mix sparse rounded crowns and slender conifers.
- Build trees as genuine low-poly 3D objects with simple trunks and a few overlapping foliage masses.
- Cluster trees intentionally, leaving visible empty land between groups; variation should come from scale, rotation, spacing, and silhouette.
- No fractal twig detail or individually modelled leaves at the gameplay camera distance.
- Use instancing and simplified distant LODs.
Rocks & Stone Harvesting Nodes
- Retain the attractive angular boulder/outcrop forms from the earlier designs.
- Make harvestable stone formations visually distinct through grouped volumes and clear silhouette, not noisy textures.
- Use a limited set of modular rock meshes with different rotations/scales.
Rivers
- Represent river paths with editable curved centreline splines, with varying widths and nonuniform, naturally sloped banks. Avoid rigid, tile-straight waterways.
- Surface water should use a low-cost animated shader: subtle downstream motion, sparse highlights, calmer central water, restrained edge foam.
- Integrate the banks smoothly into 3D terrain; bridges must have real clearance and proper 3D geometry.
- No fluid simulation is necessary for the intended map view.
Farmland & Other Resources
- Clearly readable field plots, simplified crop rows, and occasional hay bales.
- Reuse resource node templates; do not scatter decorative objects uniformly.
- Paths should follow terrain, bend organically, and remain visually distinguishable from rivers.
Structures
- Preserve the robber-baron tower as a recognizable repeated map landmark.
- Modular stone walls, roofs, timber, towers, battlements, doors, and banners.
- Model visible sides of major structures to support future orbit/close-up camera shots.
- Favor bold rooflines and layered volumes over tiny ornamentation.
Three.js Production Strategy
World representation: actual 3D mesh-based terrain, water, vegetation, rocks, and buildings, with game/simulation coordinates independent of rendering and cameras.
Cameras: elevated strategy camera initially; support perspective/orbit/tilt/cinematic flyover later without replacing assets.
Performance: prefer shared materials, geometry reuse, InstancedMesh for repeated small assets, frustum culling, terrain chunks, and distance-based LOD. Avoid unnecessary transparent layers and shadow casters.
Illustrative triangle targets (profile and adjust for actual browser/GPU targets):
Asset	Nearby	Distant LOD
Major castle	5k–15k	300–1,500
Robber-baron tower	1k–4k	150–500
Tree	80–400	12–50
Rock group	100–600	20–80
Farm	300–1,000	30–150


Polygon count is only one cost. Draw calls, shader complexity, texture memory, transparency, and shadows can dominate. Target smooth frame times on a representative populated map rather than relying on triangle budgets alone.
Prototype Acceptance Test
Before populating the world, create one polished 32 × 32 map region featuring:
- One robber-baron tower, designed as a repeatable landmark.
- A naturally winding, width-varying animated river with 3D banks.
- A bridge and connected curved path.
- Three to five minimalist tree variants arranged in clusters.
- An angular stone harvesting outcrop.
- A small farmland plot with hay bales.
- Open stretches of warm golden terrain so landmarks can breathe.
Judge it at three views: normal high-angle strategy framing, medium zoom, and a low tilted cinematic angle. The style should remain consistent and compelling in all three.
Success criteria: immediately readable map, unmistakably warm pastoral palette, restrained pale blues, no excessive micro-detail, clear reusable asset structure, natural river curves, polished animation, and smooth browser rendering.
Compact Implementer Brief for Claude
Build a real, fully 3D Three.js medieval strategy-map prototype using the Warm Pastoral / Atmospheric Medieval Illustration direction in this document. Treat the old illustrated reference as the art-direction authority: sunlit straw/ochre ground, pale mist-blue water and distance haze, muted pine silhouettes, warm stone buildings, spacious uncluttered composition. Keep boulders, robber-baron towers, bridges, and harvesting resources, but simplify vegetation radically. Generate naturally winding spline-based rivers with inexpensive shader animation. Use modular geometry, shared materials, instancing, LOD, and chunk-friendly coordinates. Preserve arbitrary future camera tilt/orbit/flyover. First perfect a 32 × 32 demonstration area instead of generating a sprawling low-quality map. The outcome should be production-minded, beautiful through shape/colour/lighting, and feasible on a normal desktop browser. No fantasy magic, saturated mobile-game colours, noisy textures, or fractal foliage.
