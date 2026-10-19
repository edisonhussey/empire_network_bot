// World constants. Coordinates are integer grid *nodes* (x right, y down the map = scene z). World position = node * CELL.
// 1 scene unit = 1 metre. A node is 10 m, so the 3x3-node castle island is 30 m across (the castle itself stands 24 m tall).
export const MAP = { size: 50, pad: 3 };      // default map: nodes 0..size plus `pad` border nodes (a World can be built with any size)
export const CELL = 10;                       // metres per node
export const MODEL_TILE = 1.25;               // units per tile the older generated pieces (map_library.glb, castle, tower, squad) were modelled at
export const LEGACY_SCALE = CELL / MODEL_TILE;   // placing one of those pieces at this scale keeps it filling the same number of nodes (see src/map/asset_scale.js)
export const FOOTPRINT_HALF = { castle: 1, dungeon: 1 };   // footprint = (2 * half + 1) square nodes; see npc_kinds.js for NPCs

// Rounded margin (in nodes) kept free around each kind of solid when drawing routes, so squads never graze an obstacle.
export const CLEARANCE = { boulder: 1.6, wall: 1.2, river: 0, castle: 0, npc: 0, default: 1.0 };

// Routes are only drawn, never used for timing, so they are cheap and best-effort.
export const ROUTE = {
  maxExpansions: 60000,    // A* budget; beyond it the route falls back to a straight line (marked approximate)
  extraClearance: 0.7,     // extra margin the search keeps so the rounded corners stay clear too
  cornerRadius: 1.0,       // visual corner rounding (nodes)
  edgeInset: 0.15,         // how far inside the footprint edge a route starts and ends (nodes), so squads reach the building itself
  // clearances tried in turn when drawing a route over a MapField: [rocks/buildings, single trees]. Dense forest can wall a castle in, so later
  // levels squeeze between trees, then ignore everything but water, before giving up and drawing a straight line.
  levels: [{ clearance: 1.0, tree: 0.5 }, { clearance: 0.6, tree: 0 }, { clearance: 0, tree: 0 }],
  fieldClearance: 1.0,     // nodes squads keep from rocks, buildings, fields and trees on a MapField
  cacheSize: 600,
};

// What the renderer draws: only things inside the window (nodes), never the whole map.
export const WINDOW = { margin: 6, maxRadius: 90, bucket: 8 };
export const LOD = { maxCastles: 40, maxArmies: 14, maxNpcs: 40 };   // detailed models; the rest are cheap markers
// Pines (assets/pine.glb): the one tree. One height; the field adds a little per-tree variation on top.
export const TREE = {
  height: 18,                                // metres (the model is built to this height by tools/build_tree.mjs)
  detailRadius: 25,                          // nodes from the camera target within which pines use the full model
  detailCap: 120,                            // at most this many full models (28k triangles each); the nearest win, the rest use the cheap cones
  extrasRadius: 8, extrasCap: 40,            // the pine's own pebbles, grass and sparkles (heavy: ~25k triangles per tree) only on the nearest few
  farLeaf: '#4d7f45', farTrunk: '#5b4636',   // colours of the cheap far version
};
// Landmarks are scaled so they stand this tall (metres), whatever size they were modelled at.
export const BUILDING_HEIGHT = { castle: 24, tower: 24 };
// Squads: soldiers are modelled ~0.3 units tall; SOLDIER_SCALE makes that a real 1.8 m, SQUAD_EXAGGERATION then draws the whole squad bigger so it can be seen
// from the strategy camera (set it to 1 for true scale).
export const SOLDIER_SCALE = 6;
export const SQUAD_EXAGGERATION = 2.5;
export const UNIT_SCALE = SOLDIER_SCALE * SQUAD_EXAGGERATION;          // squads are drawn this much bigger than modelled
