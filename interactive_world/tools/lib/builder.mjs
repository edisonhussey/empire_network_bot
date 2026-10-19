// Tiny mesh toolkit for hand-built, flat-shaded, vertex-coloured game assets.
//
// A `Prim` is one glTF primitive (one material). Faces are added as triangles with a flat normal; colours are given in sRGB hex and
// stored linear. A transform stack lets a sub-assembly be built in local space and placed anywhere (tower blocks around a circle).

export function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export const srgbToLinear = (c) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));

export function hex(h) {
  const n = parseInt(h.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
}

/** Multiply an sRGB colour by `k` (brightness) and shift it by small per-channel noise. */
export function tint(color, k = 1, rng = null, noise = 0) {
  return color.map((v, i) => {
    const n = rng ? (rng() - 0.5) * noise : 0;
    return Math.min(1, Math.max(0, v * k + n));
  });
}

export function mix(a, b, t) {
  return a.map((v, i) => v + (b[i] - v) * t);
}

// ---- 4x4 matrices (column-major flat arrays are avoided: rows of [x,y,z,w] keep this readable) -------------------------------------------
export const I4 = () => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
export function mul4(a, b) {
  const o = new Array(16).fill(0);
  for (let r = 0; r < 4; r++) for (let c = 0; c < 4; c++) for (let k = 0; k < 4; k++) o[r * 4 + c] += a[r * 4 + k] * b[k * 4 + c];
  return o;
}
export const T4 = (x, y, z) => [1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z, 0, 0, 0, 1];
export const S4 = (x, y = x, z = x) => [x, 0, 0, 0, 0, y, 0, 0, 0, 0, z, 0, 0, 0, 0, 1];
export function RY(a) {
  const c = Math.cos(a), s = Math.sin(a);
  return [c, 0, s, 0, 0, 1, 0, 0, -s, 0, c, 0, 0, 0, 0, 1];
}
export function RX(a) {
  const c = Math.cos(a), s = Math.sin(a);
  return [1, 0, 0, 0, 0, c, -s, 0, 0, s, c, 0, 0, 0, 0, 1];
}
export function RZ(a) {
  const c = Math.cos(a), s = Math.sin(a);
  return [c, -s, 0, 0, s, c, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
}
const apply = (m, p) => [
  m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
  m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
  m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
];

export const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const scale = (a, k) => [a[0] * k, a[1] * k, a[2] * k];
export const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
export const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const len = (a) => Math.hypot(a[0], a[1], a[2]);
export const norm = (a) => {
  const l = len(a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};
export const lerp3 = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

export class Prim {
  constructor(name, material) {
    this.name = name;
    this.material = material;
    this.pos = [];
    this.nor = [];
    this.col = [];
    this.stack = [I4()];
  }

  get m() {
    return this.stack[this.stack.length - 1];
  }

  /** Run `fn` with an extra transform applied (local space -> parent space). */
  with(matrix, fn) {
    this.stack.push(mul4(this.m, matrix));
    try {
      fn();
    } finally {
      this.stack.pop();
    }
  }

  tri(a, b, c, color) {
    const m = this.m;
    const A = apply(m, a), B = apply(m, b), C = apply(m, c);
    const n = norm(cross(sub(B, A), sub(C, A)));
    for (const p of [A, B, C]) {
      this.pos.push(p[0], p[1], p[2]);
      this.nor.push(n[0], n[1], n[2]);
      this.col.push(color[0], color[1], color[2]);
    }
  }

  /** Counter-clockwise as seen from outside. */
  quad(a, b, c, d, color) {
    this.tri(a, b, c, color);
    this.tri(a, c, d, color);
  }

  /** Same quad, also visible from behind (flags, thin sheets). */
  quad2(a, b, c, d, color) {
    this.quad(a, b, c, d, color);
    this.quad(a, d, c, b, color);
  }

  get triangles() {
    return this.pos.length / 9;
  }
}

/**
 * A chamfered ("pillow") block: the soft-edged stone of a stylised game asset.
 * (x0,y0,z0)-(x1,y1,z1) is the outer box; `bevel` is how far the edges are cut back; `faces` selects which faces to emit.
 * The top and the four sides get a chamfer ring; the bottom is never emitted.
 */
export function block(prim, x0, y0, z0, x1, y1, z1, color, { bevel = 0.012, faces = 'nsewtu', shade = 1.0 } = {}) {
  const b = Math.min(bevel, (x1 - x0) / 3, (z1 - z0) / 3, (y1 - y0) / 3);
  const lo = (c, k) => c.map((v) => Math.min(1, v * k));
  const top = lo(color, 1.12 * shade);
  const side = lo(color, 1.0 * shade);
  const cham = lo(color, 1.22 * shade);
  const dark = lo(color, 0.78 * shade);
  const ix0 = x0 + b, ix1 = x1 - b, iz0 = z0 + b, iz1 = z1 - b, iy1 = y1 - b;
  // sides (vertical, from y0 up to the chamfer start)
  if (faces.includes('s')) prim.quad([x0, y0, z1], [x1, y0, z1], [x1, iy1, z1], [x0, iy1, z1], side); // +z
  if (faces.includes('n')) prim.quad([x1, y0, z0], [x0, y0, z0], [x0, iy1, z0], [x1, iy1, z0], dark); // -z
  if (faces.includes('e')) prim.quad([x1, y0, z1], [x1, y0, z0], [x1, iy1, z0], [x1, iy1, z1], lo(color, 0.9 * shade)); // +x
  if (faces.includes('w')) prim.quad([x0, y0, z0], [x0, y0, z1], [x0, iy1, z1], [x0, iy1, z0], dark); // -x
  if (faces.includes('t')) {
    // chamfers (sloped ring) and the top plate
    prim.quad([x0, iy1, z1], [x1, iy1, z1], [ix1, y1, iz1], [ix0, y1, iz1], cham);
    prim.quad([x1, iy1, z0], [x0, iy1, z0], [ix0, y1, iz0], [ix1, y1, iz0], dark);
    prim.quad([x1, iy1, z1], [x1, iy1, z0], [ix1, y1, iz0], [ix1, y1, iz1], cham);
    prim.quad([x0, iy1, z0], [x0, iy1, z1], [ix0, y1, iz1], [ix0, y1, iz0], dark);
    prim.quad([ix0, y1, iz1], [ix1, y1, iz1], [ix1, y1, iz0], [ix0, y1, iz0], top);
  }
  if (faces.includes('u')) prim.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], dark);
}

/** Plain box, flat faces, no chamfer: timber, posts, slabs. */
export function slab(prim, x0, y0, z0, x1, y1, z1, color, faces = 'nsewtu') {
  const k = (v) => color.map((c) => Math.min(1, c * v));
  if (faces.includes('s')) prim.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], k(1.0));
  if (faces.includes('n')) prim.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], k(0.8));
  if (faces.includes('e')) prim.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], k(0.9));
  if (faces.includes('w')) prim.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], k(0.82));
  if (faces.includes('t')) prim.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], k(1.14));
  if (faces.includes('u')) prim.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], k(0.6));
}

/**
 * A box between two points along an arbitrary axis (rafters, beams, posts), `w` wide and `h` tall measured across the axis.
 * `up` says which way "tall" points so beams do not twist.
 */
export function beam(prim, p0, p1, w, h, color, up = [0, 1, 0]) {
  const ax = norm(sub(p1, p0));
  let side = norm(cross(ax, up));
  if (len(side) < 1e-6) side = [1, 0, 0];
  const tall = norm(cross(side, ax));
  const sw = scale(side, w / 2), th = scale(tall, h / 2);
  const c = (p, a, b) => add(add(p, scale(sw, a)), scale(th, b));
  const v = [
    c(p0, -1, -1), c(p0, 1, -1), c(p0, 1, 1), c(p0, -1, 1),
    c(p1, -1, -1), c(p1, 1, -1), c(p1, 1, 1), c(p1, -1, 1),
  ];
  const k = (f) => color.map((x) => Math.min(1, x * f));
  prim.quad(v[3], v[2], v[6], v[7], k(1.15)); // up
  prim.quad(v[0], v[4], v[5], v[1], k(0.6)); // down
  prim.quad(v[1], v[5], v[6], v[2], k(0.92)); // +side
  prim.quad(v[0], v[3], v[7], v[4], k(0.8)); // -side
  prim.quad(v[4], v[7], v[6], v[5], k(1.0)); // end p1
  prim.quad(v[0], v[1], v[2], v[3], k(0.85)); // end p0
}

/** Convex polygon fan (any vertex count), counter-clockwise from outside. */
export function poly(prim, pts, color) {
  for (let i = 1; i < pts.length - 1; i++) prim.tri(pts[0], pts[i], pts[i + 1], color);
}

/** A faceted rock: a jittered, squashed icosphere-ish blob with lighter top planes (stylised, low poly). */
export function rock(prim, cx, cy, cz, rx, ry, rz, rng, base, { detail = 1, flat = 0.0 } = {}) {
  const t = (1 + Math.sqrt(5)) / 2;
  let verts = [
    [-1, t, 0], [1, t, 0], [-1, -t, 0], [1, -t, 0], [0, -1, t], [0, 1, t], [0, -1, -t], [0, 1, -t], [t, 0, -1], [t, 0, 1], [-t, 0, -1], [-t, 0, 1],
  ].map(norm);
  let faces = [
    [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
    [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
  ];
  for (let d = 0; d < detail; d++) {
    const cache = new Map();
    const mid = (a, b) => {
      const key = a < b ? `${a}_${b}` : `${b}_${a}`;
      if (!cache.has(key)) {
        verts.push(norm(lerp3(verts[a], verts[b], 0.5)));
        cache.set(key, verts.length - 1);
      }
      return cache.get(key);
    };
    const next = [];
    for (const [a, b, c] of faces) {
      const ab = mid(a, b), bc = mid(b, c), ca = mid(c, a);
      next.push([a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]);
    }
    faces = next;
  }
  const jit = verts.map((v) => 0.78 + rng() * 0.44);
  const P = verts.map((v, i) => {
    let y = v[1];
    if (flat && y < -0.1) y = -0.1 - (y + 0.1) * (1 - flat); // flatten the underside so it sits on the ground
    return [cx + v[0] * rx * jit[i], cy + y * ry * jit[i], cz + v[2] * rz * jit[i]];
  });
  for (const [a, b, c] of faces) {
    const n = norm(cross(sub(P[b], P[a]), sub(P[c], P[a])));
    const up = Math.max(0, n[1]);
    const col = tint(base, 0.78 + up * 0.5 + (rng() - 0.5) * 0.08);
    prim.tri(P[a], P[b], P[c], col);
  }
}

/** A quad that is guaranteed to face `towards` (flips the winding if needed): roofs, floors, cloth. */
export function facing(prim, a, b, c, d, color, towards) {
  const n = cross(sub(b, a), sub(c, a));
  if (dot(n, towards) >= 0) prim.quad(a, b, c, d, color);
  else prim.quad(a, d, c, b, color);
}

/** A pillowed tile on a parallelogram patch: used for roof tiles and paving. `o` origin, `u`/`v` edge vectors, `up` the face direction. */
export function tile(prim, o, u, v, color, up, bevel = 0.006, lift = 0.012) {
  const n = norm(up);
  const lu = len(u), lv = len(v);
  const b = Math.min(bevel, lu / 3, lv / 3);
  const eu = scale(norm(u), b), ev = scale(norm(v), b);
  const top = (p) => add(p, scale(n, lift));
  const p00 = o, p10 = add(o, u), p11 = add(add(o, u), v), p01 = add(o, v);
  const q00 = top(add(add(p00, eu), ev)), q10 = top(sub(add(p10, ev), eu)), q11 = top(sub(sub(p11, eu), ev)), q01 = top(add(sub(p01, ev), eu));
  const k = (f) => color.map((x) => Math.min(1, x * f));
  facing(prim, q00, q10, q11, q01, k(1.08), n);
  facing(prim, p00, p10, q10, q00, k(0.7), sub(scale(v, -1), scale(n, 0)));
  facing(prim, p10, p11, q11, q10, k(0.85), u);
  facing(prim, p11, p01, q01, q11, k(1.22), v);
  facing(prim, p01, p00, q00, q01, k(0.75), scale(u, -1));
}
