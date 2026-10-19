// Smooth value noise and fractal sums, used only for broad landforms and gentle perturbation (never as the terrain by itself).
import { hash2 } from './rng.js';

const fade = (t) => t * t * (3 - 2 * t);

/** Value noise in [0, 1], smooth, one feature per unit. */
export function valueNoise(x, y, seed) {
  const x0 = Math.floor(x), y0 = Math.floor(y), fx = fade(x - x0), fy = fade(y - y0);
  const a = hash2(x0, y0, seed), b = hash2(x0 + 1, y0, seed), c = hash2(x0, y0 + 1, seed), d = hash2(x0 + 1, y0 + 1, seed);
  return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy;
}

/** Fractal sum of `octaves` value-noise layers, normalised to [0, 1]. */
export function fbm(x, y, seed, octaves = 4) {
  let sum = 0, amp = 1, norm = 0, f = 1;
  for (let o = 0; o < octaves; o++) { sum += amp * valueNoise(x * f, y * f, seed + o * 101); norm += amp; amp *= 0.5; f *= 2; }
  return sum / norm;
}

export const clamp = (v, lo, hi) => (v < lo ? lo : v > hi ? hi : v);
export const lerp = (a, b, t) => a + (b - a) * t;
export function smoothstep(e0, e1, x) { const t = clamp((x - e0) / (e1 - e0), 0, 1); return t * t * (3 - 2 * t); }
