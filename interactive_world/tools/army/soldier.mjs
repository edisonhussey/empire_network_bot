// Chibi soldier parts, each modelled around its own joint so it can be a node in an animated tree. Soldier faces +z, stands on y = 0.
// Proportions: big head, short legs; about 0.27 tall (the castle is about 2.0), like a Clash-of-Clans troop next to a building.
import { Prim, block, rock, beam, poly, hex, tint } from '../lib/builder.mjs';
import { TEAM_GREY as G } from '../../src/world/team_slots.js';

export const MAT = {
  cloth: { roughness: 0.9 }, team_a: { roughness: 0.85 }, team_b: { roughness: 0.85 }, team_a_ds: { roughness: 0.7, doubleSided: true }, team_b_ds: { roughness: 0.7, doubleSided: true },
  skin: { roughness: 0.75 }, metal: { roughness: 0.38, metallic: 0.55 }, wood: { roughness: 0.85 },
  leather: { roughness: 0.85 }, gold: { roughness: 0.35, metallic: 0.6 }, flag: { roughness: 0.7, doubleSided: true },
  dirt: { roughness: 1 },
};
export const mk = (name) => new Prim(name, MAT[name]);

export const C = {
  team: [G, G, G], teamDark: [G * 0.7, G * 0.7, G * 0.7], gold: hex('#d9a93a'), skin: hex('#e8b48c'), steel: hex('#aab3bd'),
  wood: hex('#7a5a3a'), leather: hex('#4a3a2c'), trouser: hex('#3b4252'), eye: hex('#231a15'), dirt: hex('#7a6a4f'),
};

// Joint heights in the unit's frame (y = 0 is the ground).
export const J = { hip: 0.072, shoulder: 0.147, neck: 0.16, shoulderX: 0.054, legX: 0.02, hand: -0.056 };

const SPHERE = () => 0.5; // constant "rng": rock() then yields a clean, un-jittered, un-noised sphere
const ball = (prim, x, y, z, rx, ry, rz, color, detail = 1) => rock(prim, x, y, z, rx, ry, rz, SPHERE, color, { detail });

export function leg() {
  const cloth = mk('cloth'), leather = mk('leather');
  block(cloth, -0.014, -0.05, -0.016, 0.014, 0.0, 0.016, C.trouser, { bevel: 0.006 });
  block(leather, -0.016, -0.072, -0.018, 0.016, -0.045, 0.032, C.leather, { bevel: 0.007 });
  return [cloth, leather];
}

export function torso() {
  const teamA = mk('team_a'), leather = mk('leather'), gold = mk('gold');
  block(teamA, -0.042, 0.012, -0.03, 0.042, 0.088, 0.03, C.team, { bevel: 0.01 });
  block(teamA, -0.046, -0.008, -0.033, 0.046, 0.02, 0.033, C.teamDark, { bevel: 0.008 });   // tunic skirt
  block(leather, -0.044, 0.014, -0.031, 0.044, 0.028, 0.031, C.leather, { bevel: 0.004 });   // belt
  block(gold, -0.009, 0.014, 0.03, 0.009, 0.028, 0.035, C.gold, { bevel: 0.003 });           // buckle
  block(gold, -0.013, 0.05, 0.029, 0.013, 0.078, 0.033, C.gold, { bevel: 0.004 });           // chest crest
  return [teamA, leather, gold];
}

/** Head with helmet. `crest` is the plume: 'a' / 'b' (team colours) or 'gold' (tells unit types apart at a glance). */
export function head(crest) {
  const skin = mk('skin'), metal = mk('metal'), leather = mk('leather');
  const plume = crest === 'gold' ? mk('gold') : mk(crest === 'a' ? 'team_a' : 'team_b');
  ball(skin, 0, 0.044, 0.004, 0.043, 0.04, 0.041, C.skin);
  for (const sx of [-1, 1]) block(leather, sx * 0.015 - 0.004, 0.03, 0.037, sx * 0.015 + 0.004, 0.044, 0.046, C.eye, { bevel: 0.002 }); // eyes
  ball(metal, 0, 0.074, -0.004, 0.052, 0.034, 0.05, C.steel);                                                                         // helmet dome
  block(metal, -0.0035, 0.034, 0.045, 0.0035, 0.07, 0.051, C.steel, { bevel: 0.0015 });                                               // nose guard
  block(plume, -0.005, 0.1, -0.034, 0.005, 0.126, 0.034, crest === 'gold' ? C.gold : C.team, { bevel: 0.004 });                                                    // crest
  return [skin, metal, leather, plume];
}

/** Arm hanging from the shoulder (origin at the shoulder). `kit` adds what the hand holds. */
export function arm(kit = 'none') {
  const teamA = mk('team_a'), skin = mk('skin'), metal = mk('metal');
  block(teamA, -0.011, -0.046, -0.011, 0.011, 0.0, 0.011, C.team, { bevel: 0.004 });
  ball(skin, 0, J.hand, 0, 0.013, 0.013, 0.013, C.skin, 0);
  ball(metal, 0, 0.0, 0, 0.017, 0.013, 0.017, C.steel, 0);   // shoulder pad
  const prims = [teamA, skin, metal];
  const y = J.hand;
  if (kit === 'sword') {
    const gold = mk('gold');
    block(metal, -0.0035, y - 0.009, 0.014, 0.0035, y + 0.009, 0.118, C.steel, { bevel: 0.0015 });
    poly(metal, [[-0.0035, y - 0.009, 0.118], [0.0035, y - 0.009, 0.118], [0, y, 0.134]], C.steel);
    poly(metal, [[0.0035, y + 0.009, 0.118], [-0.0035, y + 0.009, 0.118], [0, y, 0.134]], tint(C.steel, 1.1));
    block(gold, -0.005, y - 0.021, 0.008, 0.005, y + 0.021, 0.015, C.gold, { bevel: 0.002 });
    prims.push(gold);
  } else if (kit === 'spear') {
    const wood = mk('wood');
    block(wood, -0.004, y - 0.004, -0.07, 0.004, y + 0.004, 0.2, C.wood, { bevel: 0.0015 });
    block(metal, -0.0025, y - 0.011, 0.2, 0.0025, y + 0.011, 0.234, C.steel, { bevel: 0.001 });
    prims.push(wood);
  } else if (kit === 'shield') {
    const wood = mk('wood'), gold = mk('gold');
    const cx = -0.016, cy = -0.05, cz = 0.034, R = 0.052, t = 0.009, n = 14;
    const p = (a, rad, z) => [cx + Math.cos(a) * rad, cy + Math.sin(a) * rad, z];
    for (let i = 0; i < n; i++) {
      const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
      const zf = cz + t / 2, zb = cz - t / 2;
      wood.tri([cx, cy, zf], p(a0, R * 0.66, zf), p(a1, R * 0.66, zf), tint(C.wood, i % 2 ? 1.0 : 0.85));                    // boards
      teamA.quad(p(a0, R * 0.66, zf), p(a0, R, zf), p(a1, R, zf), p(a1, R * 0.66, zf), C.team);                                // blue rim face
      metal.quad(p(a0, R, zb), p(a1, R, zb), p(a1, R, zf), p(a0, R, zf), C.steel);                                             // rim edge
      wood.tri([cx, cy, zb], p(a1, R, zb), p(a0, R, zb), C.leather);                                                           // back
    }
    ball(gold, cx, cy, cz + t / 2 + 0.002, 0.013, 0.013, 0.01, C.gold, 1);
    prims.push(wood, gold);
  }
  return prims;
}

/** Standard bearer's pole and banner, in the unit's frame (static: held by the right hand). Returns { pole, banner } prim lists; the banner pivots at the pole top. */
export function banner() {
  const wood = mk('wood'), gold = mk('gold');
  const x = J.shoulderX, z = 0.05, top = 0.43;
  beam(wood, [x, 0, z], [x, top, z], 0.008, 0.008, C.wood);
  beam(wood, [x - 0.06, top - 0.02, z], [x + 0.06, top - 0.02, z], 0.006, 0.006, C.wood);
  ball(gold, x, top + 0.012, z, 0.012, 0.012, 0.012, C.gold, 1);
  const fa = mk('team_a_ds'), fb = mk('team_b_ds'), emblem = mk('gold');
  // cloth hangs from the crossbar, pivot at the crossbar centre; primary band over secondary band, a swallow tail, a gold emblem
  const w = 0.052, h = 0.13;
  const q = (prim, x0, y0, x1, y1, col, zz = 0) => prim.quad([x0, y0, zz], [x1, y0, zz], [x1, y1, zz], [x0, y1, zz], col);
  q(fa, -w, -h * 0.5, w, 0, C.team);
  q(fb, -w, -h * 0.9, w, -h * 0.5, C.team);
  fb.tri([-w, -h * 0.9, 0], [0, -h * 0.74, 0], [0, -h * 0.9, 0], C.team);
  q(emblem, -0.012, -h * 0.4, 0.012, -h * 0.18, C.gold, 0.002);
  return { pole: [wood, gold], flag: [fa, fb, emblem], pivot: [x, top - 0.02, z + 0.004] };
}

/** A flat parade ground with a blue rim, so the squad reads as one group from far away. */
export function squadBase(cx, cz, rx, rz) {
  const dirt = mk('dirt'), ring = mk('team_a');
  const n = 56;
  const P = (a, k, y) => [cx + Math.sin(a) * rx * k, y, cz + Math.cos(a) * rz * k];
  for (let i = 0; i < n; i++) {
    const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
    dirt.tri([cx, 0.003, cz], P(a0, 0.96, 0.003), P(a1, 0.96, 0.003), tint(C.dirt, 0.92 + (i % 3) * 0.05));
    ring.quad(P(a0, 0.96, 0.012), P(a0, 1.0, 0.012), P(a1, 1.0, 0.012), P(a1, 0.96, 0.012), tint(C.team, 1.0));     // top of the rim
    ring.quad(P(a0, 1.0, 0.0), P(a1, 1.0, 0.0), P(a1, 1.0, 0.012), P(a0, 1.0, 0.012), C.teamDark);                  // outer wall
    ring.quad(P(a1, 0.96, 0.0), P(a0, 0.96, 0.0), P(a0, 0.96, 0.012), P(a1, 0.96, 0.012), C.teamDark);             // inner wall
  }
  return [dirt, ring];
}
