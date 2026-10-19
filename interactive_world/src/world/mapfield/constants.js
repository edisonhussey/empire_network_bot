// Numbers the map field shares with the asset generators and the views. Heights and widths are in metres (1 scene unit = 1 m); anything called
// "nodes" is in grid nodes (1 node = CELL = 10 m).
export const WATER_LEVEL = -0.5;                         // metres
export const BRIDGE = { deck: 0.5, arch: 0.4, lengths: [3], halfWidth: 0.5 };   // metres above the banks; halfWidth in nodes (the walkable strip, wider than the 4.5 m deck so the 1-node lattice always meets it)

/** Solid footprint (nodes) of each placeable building: circle { r } or box { hx, hy }. Nodes and tiles are the same unit. */
export const BUILDING_SOLIDS = {
  house_thatch: { hx: 0.45, hy: 0.4 }, house_tile: { hx: 0.45, hy: 0.4 }, farm_building: { hx: 0.92, hy: 0.55 }, watchtower_s: { r: 0.45 },
};

export const STONE_ASSETS = { 1: ['stone_s1', 'stone_s2'], 2: ['stone_m1', 'stone_m2'], 3: ['stone_l1', 'stone_l2'] };
