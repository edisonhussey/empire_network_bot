// Procedural map generator: from a size, a seed and the castle / NPC positions it writes a MapField spec (rivers, roads, forests, farms,
// houses, towers, rocks, outcrops). The hand-authored example (sheet_example.js) uses exactly the same spec format, so anything the
// example shows can be produced from a seed, and hand edits and generated maps can be mixed freely.
import { MapField } from './field.js';
import { mulberry, fbm } from './noise.js';

const dist = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
const pt = (p) => (Array.isArray(p) ? { x: p[0], y: p[1] } : { x: p.x, y: p.y });   // castles and npcs may be [x, y] or { x, y, ... }

/**
 * options: size, seed, castles [[x,y]...], npcs [[x,y]...], rivers (count), forestCover (0..1), farms, houses, towers, rocks, outcrops.
 * Returns a spec for `new MapField(spec)` (and for `world.setField(spec)`).
 */
export function generateMap({ size = 50, seed = 1, castles = [], npcs = [], rivers = 1, forestCover = 0.3, farms = true, houses = 8, towers = 3, rocks = 12, outcrops = 5 } = {}) {
  const rng = mulberry(seed * 7919 + 13);
  const R = (a, b) => a + rng() * (b - a);
  const sites = [...castles, ...npcs].map(pt);
  const cast = castles.map(pt);
  const pad = 3;

  // ---- rivers: meander from one edge to the opposite one, pushed away from castles ---------------------------------------------------------------
  const riverSpecs = [];
  for (let i = 0; i < rivers; i++) {
    const vertical = rng() < 0.5, along = (t) => -pad - 3 + t * (size + 2 * pad + 6);
    const offset = size * R(0.3 + 0.4 * (i / Math.max(1, rivers)), 0.5 + 0.35 * ((i + 1) / Math.max(1, rivers)));
    const pts = [];
    const n = Math.max(6, Math.round(size / 6));
    for (let k = 0; k <= n; k++) {
      const t = k / n;
      let lat = offset + Math.sin(t * Math.PI * R(1.6, 2.6) + R(0, 6)) * size * 0.12 + (rng() - 0.5) * size * 0.06;
      let a = along(t);
      let p = vertical ? { x: lat, y: a } : { x: a, y: lat };
      for (const s of sites) { const d = dist(p, s); if (d < 7) { const push = (7 - d) * 1.1, dx = (p.x - s.x) / (d || 1), dy = (p.y - s.y) / (d || 1); p = { x: p.x + dx * push, y: p.y + dy * push }; } }
      pts.push([p.x, p.y]);
    }
    riverSpecs.push({ points: pts, width: R(0.5, 0.95) });   // 5-9.5 m
  }

  // ---- roads: a minimum spanning tree over castles and npcs, each edge a gently wandering line that stops at the gate -----------------------------
  const all = sites;
  const inTree = new Set([0]);
  const edges = [];
  while (inTree.size < all.length) {
    let best = null;
    for (const i of inTree) for (let j = 0; j < all.length; j++) if (!inTree.has(j)) { const d = dist(all[i], all[j]); if (!best || d < best.d) best = { i, j, d }; }
    edges.push(best); inTree.add(best.j);
  }
  for (let i = 0; i < all.length; i++) for (let j = i + 1; j < all.length; j++) if (rng() < 0.12 && dist(all[i], all[j]) < size * 0.5 && !edges.some((e) => (e.i === i && e.j === j) || (e.i === j && e.j === i))) edges.push({ i, j, d: dist(all[i], all[j]) });
  const roadSpecs = edges.map(({ i, j, d }) => {
    const a = all[i], b = all[j], ux = (b.x - a.x) / d, uy = (b.y - a.y) / d, nx = -uy, ny = ux;
    const gate = 2.3, pts = [[a.x + ux * gate, a.y + uy * gate]];
    const n = Math.max(1, Math.round(d / 7));
    for (let k = 1; k < n; k++) {
      const t = k / n, wob = (fbm(t * 3 + i * 5, j * 3, seed, 2) - 0.5) * 6;
      pts.push([a.x + ux * (gate + (d - 2 * gate) * t) + nx * wob, a.y + uy * (gate + (d - 2 * gate) * t) + ny * wob]);
    }
    pts.push([b.x - ux * gate, b.y - uy * gate]);
    return { points: pts, width: 0.3 };   // 3 m paths
  });

  // probe the land so far (rivers + roads) to place everything else sensibly
  const probe = new MapField({ size, seed, rivers: riverSpecs, roads: roadSpecs });
  const placed = sites.map((s) => ({ x: s.x, y: s.y, r: 3 }));
  const free = (x, y, r, { road = 1.5, river = 3 } = {}) => {
    if (x < 1 || y < 1 || x > size - 1 || y > size - 1) return false;
    const rv = probe.riverAt(x, y); if (rv && rv.edge < river + r * 0.3) return false;
    const rd = probe.roadAt(x, y); if (rd && rd.edge < road + r * 0.3) return false;
    for (const p of placed) if (dist(p, { x, y }) < p.r + r) return false;
    return true;
  };
  const place = (x, y, r) => { placed.push({ x, y, r }); };
  const tryPlace = (r, fn, tries = 40, opts) => {
    for (let t = 0; t < tries; t++) { const x = R(2, size - 2), y = R(2, size - 2); if (free(x, y, r, opts)) { fn(x, y); place(x, y, r); return true; } }
    return false;
  };

  const fields = [], buildings = [], rockSpecs = [], outcropSpecs = [];
  // ---- farms beside the castles ------------------------------------------------------------------------------------------------------------------------
  if (farms) for (const c of cast) {
    if (rng() < 0.25) continue;
    for (let t = 0; t < 30; t++) {
      const a = R(0, Math.PI * 2), d = R(5.5, 8), x = c.x + Math.cos(a) * d, y = c.y + Math.sin(a) * d, rot = Math.round(R(0, 3)) * 90;
      if (!free(x, y, 3.2)) continue;
      fields.push({ x, y, w: 3, h: 2, rot, kind: 'crop' });
      fields.push({ x: x + Math.cos(rot * Math.PI / 180) * 3.4, y: y - Math.sin(rot * Math.PI / 180) * 3.4, w: 2, h: 2, rot, kind: rng() < 0.4 ? 'hay' : 'crop' });
      buildings.push({ kind: rng() < 0.5 ? 'farm_building' : 'house_thatch', x: x - Math.sin(rot * Math.PI / 180) * -2.9, y: y - Math.cos(rot * Math.PI / 180) * 2.9, rot });
      place(x, y, 4.4);
      break;
    }
  }
  // ---- houses along the roads ----------------------------------------------------------------------------------------------------------------------------
  for (let i = 0; i < houses && roadSpecs.length; i++) {
    for (let t = 0; t < 30; t++) {
      const road = probe.roads[Math.floor(rng() * probe.roads.length)], s = rng() * road.path.length, p = road.path.pointAt(s);
      const side = rng() < 0.5 ? -1 : 1, d = R(1.0, 1.8), x = p.x - p.dy * d * side, y = p.y + p.dx * d * side;
      if (!free(x, y, 0.8, { road: 0.4 })) continue;
      buildings.push({ kind: rng() < 0.6 ? 'house_thatch' : 'house_tile', x, y, rot: Math.round((Math.atan2(-p.dy * side * -1, p.dx * side) / (Math.PI / 2)) * 90) });
      place(x, y, 1.0);
      break;
    }
  }
  // ---- watchtowers on high ground ---------------------------------------------------------------------------------------------------------------------------
  for (let i = 0; i < towers; i++) {
    let best = null;
    for (let t = 0; t < 40; t++) { const x = R(3, size - 3), y = R(3, size - 3); if (!free(x, y, 1.0, { road: 1 })) continue; const h = probe.heightAt(x, y); if (!best || h > best.h) best = { x, y, h }; }
    if (best) { buildings.push({ kind: 'watchtower_s', x: best.x, y: best.y, rot: Math.round(R(0, 3)) * 90 }); place(best.x, best.y, 1.2); }
  }
  // ---- rocks and outcrops ----------------------------------------------------------------------------------------------------------------------------------------
  for (let i = 0; i < rocks; i++) { const sz = rng() < 0.55 ? 1 : rng() < 0.7 ? 2 : 3; tryPlace(sz * 0.8 + 0.8, (x, y) => rockSpecs.push({ x, y, size: sz, variant: Math.floor(rng() * 2) }), 40, { road: 1.2, river: 2.2 }); }
  for (let i = 0; i < outcrops; i++) { const kind = ['bump', 'cliff', 'slope', 'bump', 'cliff_tall'][Math.floor(rng() * 5)]; tryPlace(2.2, (x, y) => outcropSpecs.push({ kind, x, y, rot: Math.round(R(0, 3)) * 90 }), 40, { road: 1.6, river: 3 }); }

  return {
    size, seed, rivers: riverSpecs, roads: roadSpecs, fields, buildings, rocks: rockSpecs, outcrops: outcropSpecs,
    forests: [{ shape: 'noise', cover: forestCover, scale: 12, density: 0.9, mix: 0.55 }],
  };
}
