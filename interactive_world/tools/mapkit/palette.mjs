// Shared palette and material set for the map-side assets (tower, trees, rocks, buildings). Colours are sRGB hex like the rest of the generators.
import { Prim, hex } from '../lib/builder.mjs';
import { STONE } from '../lib/masonry.mjs';

export const C = {
  stoneLight: hex('#c9c0aa'), stoneMid: hex('#ada48f'), stoneDark: hex('#8a8372'), stoneDeep: hex('#5f5b52'),
  rock: hex('#6d6c68'), rockDark: hex('#5f5e5b'), rockLight: hex('#96958f'),
  wood: hex('#8a5a32'), woodDark: hex('#5d3b20'), woodLight: hex('#b07a45'), cap: hex('#b87428'), plank: hex('#a06c3c'),
  grass: hex('#6c9b2e'), grassDark: hex('#5f8f2c'), moss: hex('#6f9c3a'), dirt: hex('#a88556'), dirtDark: hex('#8a6a40'), sand: hex('#d9c48a'),
  leaf1: hex('#5c9138'), leaf2: hex('#4f8a3a'), leaf3: hex('#7aa83a'), leafDark: hex('#3f7a35'), pine1: hex('#2a6040'), pine2: hex('#336e46'), pine3: hex('#245a3a'),
  trunk: hex('#6b4426'), thatch: hex('#d6a73a'), thatchDark: hex('#b8862a'), roofRed: hex('#b5532e'), roofRedDark: hex('#8f3f22'), plaster: hex('#e6dcc3'),
  blue: hex('#2a56a8'), gold: hex('#e0b24a'), metal: hex('#4a4f58'), water: hex('#3a97d8'),
};

/** Palette for the tower's masonry (lighter and greyer than the keep's). Call before building; the masonry helpers read STONE. */
export function useTowerStone() {
  STONE.sand = C.stoneLight; STONE.tan = C.stoneMid; STONE.brown = C.stoneDark; STONE.dark = C.stoneDeep; STONE.deep = hex('#55524a'); STONE.core = hex('#3a3833');
}

/** A fresh set of primitives (one per material) for one asset. */
export function makePrims() {
  return {
    stone: new Prim('stone', { roughness: 0.92 }),
    wood: new Prim('wood', { roughness: 0.85 }),
    metal: new Prim('metal', { roughness: 0.4, metallic: 0.5 }),
    gold: new Prim('gold', { roughness: 0.35, metallic: 0.6 }),
    grass: new Prim('grass', { roughness: 1 }),
    dirt: new Prim('dirt', { roughness: 1 }),
    rock: new Prim('rock', { roughness: 0.95 }),
    leaf: new Prim('leaf', { roughness: 0.9 }),
    thatch: new Prim('thatch', { roughness: 1 }),
    tile: new Prim('roof_tile', { roughness: 0.6 }),
    banner: new Prim('team_a_ds', { roughness: 0.7, doubleSided: true }),
  };
}
