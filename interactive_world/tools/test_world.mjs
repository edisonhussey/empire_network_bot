// Tests for the world logic (no browser, no dependencies):  node tools/test_world.mjs
import assert from 'node:assert/strict';
import { World } from '../src/world/world.js';
import { Router } from '../src/world/routing.js';
import { DEMO, loadScenario } from '../src/world/scenario.js';
import { Grid } from '../src/world/grid.js';
import { findPath, lineOfSight } from '../src/world/pathfinding.js';
import { Polyline } from '../src/world/polyline.js';
import { SpatialIndex } from '../src/world/spatial.js';
import { Focus } from '../src/world/focus.js';
import { CLEARANCE } from '../src/world/config.js';

let passed = 0;
const test = (name, fn) => { fn(); passed++; console.log('ok  ', name); };
const demo = () => { const w = new World({ size: DEMO.size }); loadScenario(w, DEMO); return w; };
const distToBox = (px, py, b) => Math.hypot(Math.max(b.x - 0.5 - px, 0, px - (b.x + b.w - 0.5)), Math.max(b.y - 0.5 - py, 0, py - (b.y + b.h - 0.5)));

// ---- pathfinding -------------------------------------------------------------------------------------------------------------------------------
test('A* goes straight on an empty grid', () => {
  const r = findPath(new Grid(20, 2), { starts: [{ x: 0, y: 0 }], goals: [{ x: 10, y: 0 }] });
  assert.equal(r.cost, 10);
  assert.deepEqual(r.nodes.map((n) => [n.x, n.y]), [[0, 0], [10, 0]]);
});
test('A* routes around a wall and never touches a blocked node', () => {
  const g = new Grid(20, 2);
  g.blockBox(5, -2, 1, 16);
  const r = findPath(g, { starts: [{ x: 2, y: 5 }], goals: [{ x: 9, y: 5 }], smooth: false });
  assert.ok(r.cost > 17);
  for (const n of r.nodes) assert.equal(g.isBlocked(n.x, n.y), false);
});
test('A* returns null when walled in, and { exceeded } over budget', () => {
  const g = new Grid(10, 2); g.blockBox(4, 4, 3, 3);
  assert.equal(findPath(g, { starts: [{ x: 0, y: 0 }], goals: [{ x: 5, y: 5 }] }), null);
  assert.equal(findPath(new Grid(40, 2), { starts: [{ x: 0, y: 0 }], goals: [{ x: 39, y: 39 }], smooth: false, maxExpansions: 5 }).exceeded, true);
});
test('no diagonal corner cutting, and smoothing cannot squeeze between two blocks', () => {
  const g = new Grid(10, 2); g.block(1, 0); g.block(0, 1);
  assert.ok(findPath(g, { starts: [{ x: 0, y: 0 }], goals: [{ x: 1, y: 1 }] }).cost > 2);
});
test('repeated searches reuse scratch buffers without leaking state', () => {
  const g = new Grid(30, 2); g.blockBox(10, 0, 1, 25);
  const a = findPath(g, { starts: [{ x: 0, y: 5 }], goals: [{ x: 20, y: 5 }] }).cost;
  findPath(new Grid(30, 2), { starts: [{ x: 0, y: 0 }], goals: [{ x: 29, y: 29 }] });
  assert.equal(findPath(g, { starts: [{ x: 0, y: 5 }], goals: [{ x: 20, y: 5 }] }).cost, a);
});

// ---- clearance and curves -------------------------------------------------------------------------------------------------------------------------
test('rounded margin: corners curve like a circle, not a square', () => {
  const g = new Grid(30, 4); g.blockRounded(10, 10, 4, 4, 2);
  assert.equal(g.isBlocked(9 - 1, 9 - 1), false); // the diagonal corner of the square margin stays free
  assert.equal(g.isBlocked(9, 9), true);
  assert.equal(g.isBlocked(10, 8), true);          // straight out from an edge, within r
  assert.equal(g.isBlocked(10, 7), false);
});
test('drawn routes keep the boulder clearance, everywhere along the curve', () => {
  const w = demo(), router = new Router(w);
  const boulders = w.terrain.ofKind('boulder');
  for (const [a, b] of [[[3, 3], [25, 25]], [[3, 46], [46, 46]], [[25, 25], [21, 29]], [[46, 3], [46, 46]]]) {
    const route = router.route({ x: a[0], y: a[1] }, { x: b[0], y: b[1] });
    assert.equal(route.approximate, false);
    for (let d = 0; d <= route.length; d += 0.1) {
      const p = route.polyline.pointAt(d);
      for (const bx of boulders) assert.ok(distToBox(p.x, p.y, bx) >= CLEARANCE.boulder - 0.05, `route ${a}->${b} within ${distToBox(p.x, p.y, bx).toFixed(2)} of a boulder`);
    }
  }
});
test('every castle can reach every other, and routes cross the river only on a bridge', () => {
  const w = demo(), router = new Router(w);
  const castles = [...w.castles.values()];
  const river = w.terrain.ofKind('river')[0], bridges = w.terrain.ofKind('bridge');
  for (const a of castles) for (const b of castles) {
    if (a === b) continue;
    const route = router.route(a, b);
    assert.equal(route.approximate, false, `${a.key}->${b.key} fell back to a straight line`);
    for (let d = 0; d <= route.length; d += 0.1) {
      const p = route.polyline.pointAt(d);
      const inWater = p.x > river.x - 0.5 && p.x < river.x + river.w - 0.5;
      if (inWater) assert.ok(bridges.some((br) => p.y >= br.y - 0.8 && p.y <= br.y + br.h - 0.2), `${a.key}->${b.key} wades the river at y=${p.y.toFixed(1)}`);
    }
  }
});
test('routes start and end at the footprint edge, not inside castles', () => {
  const w = demo(), router = new Router(w);
  const r = router.route({ x: 3, y: 3 }, { x: 25, y: 25 });
  const s = r.points[0], e = r.points[r.points.length - 1];
  const edge = (p, c) => Math.max(Math.abs(p.x - c[0]), Math.abs(p.y - c[1]));
  assert.ok(edge(s, [3, 3]) > 1.2 && edge(s, [3, 3]) < 1.6, `start ${edge(s, [3, 3])}`);   // on the 3x3 footprint's edge (1.5), not at the centre
  assert.ok(edge(e, [25, 25]) > 1.2 && edge(e, [25, 25]) < 1.6);
});
test('adding terrain invalidates cached routes', () => {
  const w = demo(), router = new Router(w);
  const before = router.route({ x: 3, y: 3 }, { x: 14, y: 3 }).length;
  w.terrain.add({ kind: 'boulder', x: 8, y: -2, w: 3, h: 9 });
  assert.ok(router.route({ x: 3, y: 3 }, { x: 14, y: 3 }).length > before);
});

// ---- marches: timing is independent of routes ------------------------------------------------------------------------------------------------
test('a march takes exactly its duration, wherever the route goes', () => {
  const w = demo(), arrived = [];
  w.on('arrive', (m) => arrived.push([m.id, w.now]));
  const m = w.sendMarch({ from: [3, 3], to: [46, 46], duration: 40, kingdomId: 'blue' });
  w.advance(39.9); assert.equal(arrived.length, 0);
  w.advance(0.2); assert.deepEqual(arrived.map((a) => a[0]), [m.id]);
  assert.equal(w.marches.size, 0);
});
test('progress is linear and can start in the past (a march already under way)', () => {
  const w = demo(); w.setNow(100);
  const m = w.sendMarch({ from: [0, 0], to: [10, 0], duration: 20, startedAt: 90 });
  assert.equal(w.progress(m), 0.5);
  assert.deepEqual([w.positionOf(m).x, w.positionOf(m).y], [5, 0]);
  w.setNow(95); assert.equal(w.progress(m), 0.25);
});
test('marches that start in the future are not drawn yet; arrivals fire in time order', () => {
  const w = demo(), order = [];
  w.on('arrive', (m) => order.push(m.kind));
  w.sendMarch({ from: [0, 0], to: [5, 5], duration: 30, kind: 'slow' });
  w.sendMarch({ from: [0, 0], to: [5, 5], duration: 10, kind: 'fast' });
  const later = w.sendMarch({ from: [0, 0], to: [5, 5], duration: 5, startedAt: 50, kind: 'later' });
  assert.ok(!w.marchesInWindow({ x0: -5, y0: -5, x1: 60, y1: 60 }).includes(later));
  w.setNow(100);
  assert.deepEqual(order, ['fast', 'slow', 'later']);
});
test('a march needs a positive duration', () => {
  assert.throws(() => demo().sendMarch({ from: [0, 0], to: [1, 1], duration: 0 }));
});

// ---- kingdoms, window queries, focus ------------------------------------------------------------------------------------------------------------
test('kingdoms own castle and npc arrays; bounds cover both', () => {
  const w = demo(), b = w.kingdomBounds('blue');
  assert.equal(w.kingdoms.get('blue').castles.length, 4);
  assert.deepEqual(b, { x0: 3, y0: 3, x1: 18, y1: 12 });
  assert.equal(w.entityAt(4, 4).key, 'c:3,3');   // inside the 3x3 footprint
  assert.equal(w.entityAt(30, 10), null);
});
test('window queries return only what is inside', () => {
  const w = demo();
  const { castles, npcs } = w.queryWindow({ x0: 0, y0: 0, x1: 20, y1: 15 });
  assert.equal(castles.length, 4); assert.equal(npcs.length, 1);
  assert.equal(w.queryWindow({ x0: 100, y0: 100, x1: 120, y1: 120 }).castles.length, 0);
});
test('duplicate castles are rejected; replacing a kingdom removes the old entities', () => {
  const w = demo();
  const before = w.castles.size;
  assert.throws(() => w.addCastle('blue', [3, 3]));
  w.addKingdom({ id: 'blue', castles: [[30, 30]] });
  assert.equal(w.kingdoms.get('blue').castles.length, 1);
  assert.equal(w.castles.size, before - 4 + 1);
  assert.equal(w.entityAt(3, 3), null);
  assert.equal(w.entityAt(30, 30).key, 'c:30,30');
});
test('focus selects known kingdoms only and notifies listeners', () => {
  const w = demo(), f = new Focus(w), seen = [];
  f.on((id) => seen.push(id));
  assert.equal(f.selectKingdom('nope'), false);
  assert.equal(f.selectKingdom('red'), true);
  assert.deepEqual(seen, ['red']);
});

// ---- map field: land, bridges, trees, generator ------------------------------------------------------------------------------------------------
import { MapField } from '../src/world/mapfield/field.js';
import { generateMap } from '../src/world/mapfield/generate.js';
import { SHEET, generated } from '../src/world/scenario.js';

const land = () => new MapField({
  size: 50, seed: 3,
  rivers: [{ points: [[-5, 24], [15, 28], [30, 22], [55, 28]], width: 2.4 }],
  roads: [{ points: [[10, 10], [20, 26], [40, 40]], width: 1 }],
  forests: [{ shape: 'circle', x: 30, y: 30, r: 12, density: 0.9, mix: 0.5 }],
  rocks: [{ x: 8, y: 8, size: 2 }], fields: [{ x: 10, y: 40, w: 3, h: 2, rot: 0 }], buildings: [{ kind: 'house_thatch', x: 40, y: 10, rot: 30 }],
});
test('a road that crosses a river gets a bridge; water blocks everywhere else', () => {
  const f = land();
  assert.equal(f.bridges.length, 1);
  const b = f.bridges[0];
  assert.equal(f.isBridge(b.x, b.y), true);
  assert.equal(f.blockedAt(b.x, b.y), false);                      // walkable on the bridge
  assert.equal(f.blockedAt(2, 24.8, 0), true);                      // open water far from it
  assert.ok(f.walkHeightAt(b.x, b.y) > 0.1, 'deck stands above the water');
  assert.ok(f.heightAt(b.x + 6, b.y + 6) > f.heightAt(2, 24.8), 'river bed is carved lower than the land');
});
test('trees stay off roads, water, solids and castle clearings; same seed gives the same trees', () => {
  const f = land();
  f.addClearing(30, 30, 3);
  const trees = f.propsIn({ x0: 0, y0: 0, x1: 50, y1: 50 }).filter((p) => /^(oak|pine)/.test(p.asset));
  assert.ok(trees.length > 50);
  for (const t of trees) {
    const rd = f.roadAt(t.x, t.y), rv = f.riverAt(t.x, t.y);
    assert.ok(!rd || rd.edge >= 0.9, 'tree on the road');
    assert.ok(!rv || rv.edge >= 0.95, 'tree in the water');
    assert.ok(f.solids.nearest(t.x, t.y) >= 0.9, 'tree inside a rock or building');
    assert.ok(Math.hypot(t.x - 30, t.y - 30) >= 3, 'tree in the clearing');
  }
  const again = land(); again.addClearing(30, 30, 3);
  const t2 = again.propsIn({ x0: 0, y0: 0, x1: 50, y1: 50 }).filter((p) => /^(oak|pine)/.test(p.asset));
  assert.deepEqual(t2.map((t) => [t.x, t.y, t.asset]), trees.map((t) => [t.x, t.y, t.asset]));
});
test('the ground is levelled under castles and the rasterised bitmap marks water and solids', () => {
  const w = new World({ size: 50, field: land() });
  w.addKingdom({ id: 'k', castles: [[25, 10]] });
  assert.ok(Math.abs(w.heightAt(25, 10)) < 0.01);
  const bm = w.field.rasterise({ x0: 0, y0: 18, x1: 50, y1: 32 }, { res: 1 });
  const bits = new Set(bm.mask);
  assert.ok(bits.has(1) && bits.has(0), 'water and dry land present');
  assert.equal(bm.mask.length, bm.w * bm.h);
});
test('squads follow roads when that is cheaper (the smoother may not cut across country)', () => {
  const w = new World({ size: 50, field: { size: 50, roads: [{ points: [[8, 5], [25, 22], [42, 5]], width: 1 }] } });
  w.addKingdom({ id: 'a', castles: [[5, 5]] }); w.addKingdom({ id: 'b', castles: [[45, 5]] });
  const r = new Router(w).route({ x: 5, y: 5 }, { x: 45, y: 5 });
  assert.ok(Math.max(...r.points.map((p) => p.y)) > 15, 'went round by the road');
});
test('generated maps: every castle pair routable on several seeds, and generation is deterministic', () => {
  for (const seed of [1, 2, 3, 4, 5]) {
    const sc = generated(seed);
    const w = new World({ size: 50 }); loadScenario(w, sc);
    const router = new Router(w), cs = [...w.castles.values()];
    for (const a of cs) for (const b of cs) if (a !== b) assert.equal(router.route(a, b).approximate, false, `seed ${seed}: ${a.key} -> ${b.key} has no route`);
  }
  assert.deepEqual(generateMap({ seed: 9, castles: [[5, 5], [40, 40]] }), generateMap({ seed: 9, castles: [[5, 5], [40, 40]] }));
  assert.notDeepEqual(generateMap({ seed: 9, castles: [[5, 5], [40, 40]] }).rivers, generateMap({ seed: 10, castles: [[5, 5], [40, 40]] }).rivers);
});
test('the hand-authored sheet map is fully routable with at least two bridges', () => {
  const w = new World({ size: 50 }); loadScenario(w, SHEET);
  assert.ok(w.field.bridges.length >= 2);
  const router = new Router(w), cs = [...w.castles.values()];
  for (const a of cs) for (const b of cs) if (a !== b) assert.equal(router.route(a, b).approximate, false, `${a.key} -> ${b.key}`);
});
test('a castle walled in by dense forest still gets a drawn route (relaxed levels), never a straight line through water', () => {
  const w = new World({ size: 50, field: { size: 50, seed: 5, forests: [{ shape: 'circle', x: 25, y: 25, r: 40, density: 1, mix: 0.5 }], rivers: [{ points: [[-5, 40], [55, 40]], width: 2.4 }] } });
  w.addKingdom({ id: 'a', castles: [[10, 10]] }); w.addKingdom({ id: 'b', castles: [[40, 10]] });
  const r = new Router(w).route({ x: 10, y: 10 }, { x: 40, y: 10 });
  assert.equal(r.approximate, false);
  assert.ok(r.relaxed >= 1 && r.length >= 25 && r.length < 80);
});
test('terrain height and bridge height controls: outcrops and buildings scale with h / height', () => {
  const f = new MapField({ size: 50, outcrops: [{ kind: 'cliff', x: 10, y: 10, h: 1.5 }], buildings: [{ kind: 'watchtower_s', x: 30, y: 30, height: 1.4 }] });
  const props = f.propsIn({ x0: 0, y0: 0, x1: 50, y1: 50 }).filter((p) => !/^(tuft|oak|pine)/.test(p.asset));
  assert.equal(props.find((p) => p.asset === 'cliff_edge').sy, 1.5);
  assert.equal(props.find((p) => p.asset === 'watchtower_s').sy, 1.4);
});
test('a 1286-node field answers queries lazily (no per-node storage)', () => {
  const big = generateMap({ size: 1286, seed: 3, castles: [[100, 100], [1000, 1000]], rocks: 400, houses: 100, towers: 20, outcrops: 60 });
  const f = new MapField(big);
  const t = performance.now();
  for (let i = 0; i < 2000; i++) f.blockedAt(300 + (i % 50), 300 + Math.floor(i / 50));
  assert.ok(performance.now() - t < 800, 'blockedAt too slow');
});

// ---- scale ---------------------------------------------------------------------------------------------------------------------------------------
test('a 1300x1300 map with 3000 castles: window queries and a long route stay fast', () => {
  const w = new World({ size: 1286 });
  const cells = new Set();
  let n = 0;
  w.addKingdom({ id: 'k', castles: [] });
  while (n < 3000) {
    const x = 4 + Math.floor(Math.random() * 1270) , y = 4 + Math.floor(Math.random() * 1270);
    const key = `${Math.floor(x / 5)},${Math.floor(y / 5)}`;
    if (cells.has(key)) continue;
    cells.add(key); w.addCastle('k', [Math.floor(x / 5) * 5 + 2, Math.floor(y / 5) * 5 + 2]); n++;
  }
  for (let i = 0; i < 200; i++) w.terrain.add({ kind: 'boulder', x: Math.floor(Math.random() * 1200), y: Math.floor(Math.random() * 1200), w: 3, h: 3 });
  let t = performance.now();
  for (let i = 0; i < 1000; i++) w.queryWindow({ x0: 500, y0: 500, x1: 560, y1: 560 });
  const q = (performance.now() - t) / 1000;
  const router = new Router(w);
  t = performance.now(); router.walkGrid; const build = performance.now() - t;
  const [a, b] = [...w.castles.values()].slice(0, 2);
  t = performance.now(); const r = router.route(a, b); const route = performance.now() - t;
  console.log(`      window query ${q.toFixed(3)} ms, grid build ${build.toFixed(0)} ms, route ${route.toFixed(0)} ms (${r.approximate ? 'approximate' : 'found'})`);
  assert.ok(q < 2);
});

console.log(`${passed} tests passed`);
