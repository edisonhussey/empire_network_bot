// The map asset catalogue: every piece the map can use, built individually. Each entry says how to build it (with a seed so variants differ),
// its footprint in tiles and whether it gets a cartoon outline. Heights, rotations and positions are applied when the piece is placed.
import { stoneNode, rockyBump, cliffEdge, slopePiece, cropField, hayBales, fenceSegment, bankStones, grassTuft, T } from './nature.mjs';
import { house, farmBuilding, watchtowerSmall } from './buildings.mjs';
import { bridge, dock } from './bridges.mjs';

const single = (fn) => (p, rng) => fn(p, rng, {});

export const CATALOG = [
  // ---- ground cover. No trees: the only tree is the pine, which comes from assets/pine.glb (tools/build_tree.mjs, src/map/tree_asset.js); oaks and bushes were removed
  { name: 'tuft_a', group: 'trees', fp: [1, 1], build: (p, r) => grassTuft(p, r, {}), seed: 53, outline: false },
  { name: 'tuft_b', group: 'trees', fp: [1, 1], build: (p, r) => grassTuft(p, r, { flower: true }), seed: 54, outline: false },
  // ---- stone nodes and terrain pieces ---------------------------------------------------------------------------------------------------------------
  { name: 'stone_s1', group: 'stone', fp: [1, 1], build: (p, r) => stoneNode(p, r, { size: 1 }), seed: 61, outline: true },
  { name: 'stone_s2', group: 'stone', fp: [1, 1], build: (p, r) => stoneNode(p, r, { size: 1 }), seed: 62, outline: true },
  { name: 'stone_m1', group: 'stone', fp: [2, 2], build: (p, r) => stoneNode(p, r, { size: 2, spires: 1 }), seed: 63, outline: true },
  { name: 'stone_m2', group: 'stone', fp: [2, 2], build: (p, r) => stoneNode(p, r, { size: 2, spires: 2 }), seed: 64, outline: true },
  { name: 'stone_l1', group: 'stone', fp: [3, 3], build: (p, r) => stoneNode(p, r, { size: 3, spires: 4 }), seed: 65, outline: true },
  { name: 'stone_l2', group: 'stone', fp: [3, 3], build: (p, r) => stoneNode(p, r, { size: 3, spires: 5 }), seed: 66, outline: true },
  { name: 'rocky_bump', group: 'terrain', fp: [2, 2], build: single(rockyBump), seed: 71, outline: true },
  { name: 'cliff_edge', group: 'terrain', fp: [3, 2], build: (p, r) => cliffEdge(p, r, { w: 3, d: 2, h: 1 }), seed: 72, outline: true },
  { name: 'cliff_tall', group: 'terrain', fp: [3, 2], build: (p, r) => cliffEdge(p, r, { w: 3, d: 2, h: 1.7 }), seed: 73, outline: true },
  { name: 'slope_piece', group: 'terrain', fp: [2, 2], build: (p, r) => slopePiece(p, r, { h: 0.7 }), seed: 74, outline: true },
  { name: 'bank_stones', group: 'terrain', fp: [1, 1], build: single(bankStones), seed: 75, outline: true },
  // ---- farm -------------------------------------------------------------------------------------------------------------------------------------------
  { name: 'crop_field', group: 'farm', fp: [2, 2], build: (p, r) => cropField(p, r, {}), seed: 81, outline: false },
  { name: 'hay_bales', group: 'farm', fp: [2, 2], build: single(hayBales), seed: 82, outline: false },
  { name: 'fence', group: 'farm', fp: [1, 0.2], build: single(fenceSegment), seed: 83, outline: false },
  // ---- buildings ---------------------------------------------------------------------------------------------------------------------------------------
  { name: 'house_thatch', group: 'buildings', fp: [2, 2], build: (p, r) => house(p, r, { kind: 'thatch' }), seed: 91, outline: true },
  { name: 'house_tile', group: 'buildings', fp: [2, 2], build: (p, r) => house(p, r, { kind: 'tile' }), seed: 92, outline: true },
  { name: 'farm_building', group: 'buildings', fp: [3, 2], build: single(farmBuilding), seed: 93, outline: true },
  { name: 'watchtower_s', group: 'buildings', fp: [2, 2], build: single(watchtowerSmall), seed: 94, outline: true },
  // ---- crossings ----------------------------------------------------------------------------------------------------------------------------------------
  ...[3, 4, 5, 6, 8].map((len, i) => ({ name: `bridge_${len}`, group: 'crossings', fp: [1, len], build: (p, r) => bridge(p, r, { len }), seed: 101 + i, outline: true })),
  { name: 'dock', group: 'crossings', fp: [1, 2], build: (p, r) => dock(p, r, { len: 2 }), seed: 111, outline: true },
];
