// The game world as data and rules only: kingdoms (each owning castles and NPCs at x,y), terrain features, and marches.
// Logic is bounded to coordinates: a march is "from x,y to x2,y2 over `duration` seconds starting at `startedAt`". Progress is simply
// (now - startedAt) / duration, so the arrival time never depends on terrain; routes (routing.js) only decide how a march is DRAWN.
//
//   const world = new World({ size: 50 });
//   world.addKingdom({ id: 'blue', name: 'Blue', team: 'blue', castles: [[3, 3], [9, 6]], npc: [[18, 12]] });
//   world.terrain.add({ kind: 'boulder', x: 8, y: 20, w: 6, h: 2 });
//   world.sendMarch({ from: [3, 3], to: [18, 12], duration: 40, kingdomId: 'blue' });
//   world.setNow(t) | world.advance(dt);   world.queryWindow(rect);   world.progress(march);
import { MAP, FOOTPRINT_HALF, WINDOW } from './config.js';
import { SpatialIndex } from './spatial.js';
import { Terrain } from './terrain.js';
import { npcSpec } from './npc_kinds.js';
import { rectOf, intersects, grow } from './rect.js';
import { MapField } from './mapfield/field.js';

const P = (p) => (Array.isArray(p) ? { x: p[0], y: p[1] } : { x: p.x, y: p.y });

export class World {
  constructor({ size = MAP.size, pad = MAP.pad, field = null } = {}) {
    this.size = size;
    this.pad = pad;
    this.field = null;
    this.kingdoms = new Map();
    this.castles = new Map();       // key "c:x,y" -> { key, kind: 'castle', x, y, kingdomId, name }
    this.npcs = new Map();          // key "n:x,y" -> { key, kind: 'npc', npcKind, x, y, kingdomId, name }
    this.index = new SpatialIndex(WINDOW.bucket);
    this.terrain = new Terrain();
    this.marches = new Map();
    this.queue = [];                // marches sorted by arrival time
    this.now = 0;                   // seconds; drive it with advance(dt) or set it from a server clock with setNow(t)
    this.structure = 0;
    this.listeners = {};
    this.nextId = 1;
    if (field) this.setField(field);
  }

  /** Attach the land (a MapField, or a spec object to build one from). Castles and NPCs level the ground they stand on. */
  setField(field) {
    this.field = field instanceof MapField ? field : new MapField({ size: this.size, pad: this.pad, ...field });
    for (const e of this.allEntities()) this.field.addEntity(e, this.footprintHalf(e));
    this.structure++;
    this.emit('field', this.field);
    return this.field;
  }

  /** Ground height at a node position (0 on a flat map). */
  heightAt(x, y) { return this.field ? this.field.heightAt(x, y) : 0; }

  /** Height a squad stands at: the bridge deck on a bridge, else the ground. */
  walkHeightAt(x, y) { return this.field ? this.field.walkHeightAt(x, y) : 0; }

  /** Bumps whenever castles, NPCs or terrain change (routes and grids are rebuilt lazily when it moves). */
  get version() {
    return this.structure + this.terrain.version;
  }

  on(event, fn) {
    (this.listeners[event] ??= new Set()).add(fn);
    return () => this.listeners[event].delete(fn);
  }

  emit(event, payload) {
    for (const fn of this.listeners[event] ?? []) fn(payload);
  }

  // ---- kingdoms, castles, NPCs ------------------------------------------------------------------------------------------------------------------
  /** castles: [[x, y], ...] (or {x, y, name}); npc: [[x, y], ...] (or {x, y, kind, name}). Replaces a kingdom with the same id. */
  addKingdom({ id, name = id, team = 'blue', castles = [], npc = [] }) {
    if (this.kingdoms.has(id)) this.removeKingdom(id);
    const kingdom = { id, name, team, castles: [], npc: [] };
    this.kingdoms.set(id, kingdom);
    for (const c of castles) this.addCastle(id, c);
    for (const n of npc) this.addNpc(id, n);
    this.emit('kingdom', kingdom);
    return kingdom;
  }

  removeKingdom(id) {
    const k = this.kingdoms.get(id);
    if (!k) return;
    for (const c of k.castles) { this.castles.delete(c.key); this.index.remove(c); }
    for (const n of k.npc) { this.npcs.delete(n.key); this.index.remove(n); }
    this.kingdoms.delete(id);
    this.structure++;
    this.emit('kingdom', null);
  }

  addCastle(kingdomId, pos) {
    const k = this.kingdoms.get(kingdomId);
    const { x, y } = P(pos);
    const key = `c:${x},${y}`;
    if (this.castles.has(key)) throw new Error(`castle already at ${x},${y}`);
    const castle = { key, kind: 'castle', x, y, kingdomId, name: pos.name ?? `${k.name} ${k.castles.length + 1}` };
    this.castles.set(key, castle);
    k.castles.push(castle);
    this.index.insert(castle);
    this.field?.addEntity(castle, this.footprintHalf(castle));
    this.structure++;
    return castle;
  }

  addNpc(kingdomId, pos) {
    const k = this.kingdoms.get(kingdomId);
    const { x, y } = P(pos);
    const key = `n:${x},${y}`;
    if (this.npcs.has(key)) throw new Error(`npc already at ${x},${y}`);
    const npcKind = pos.kind ?? 'dungeon';
    const npc = { key, kind: 'npc', npcKind, x, y, kingdomId, name: pos.name ?? npcSpec(npcKind).label };
    this.npcs.set(key, npc);
    k.npc.push(npc);
    this.index.insert(npc);
    this.field?.addEntity(npc, this.footprintHalf(npc));
    this.structure++;
    return npc;
  }

  setTeam(kingdomId, team) {
    const k = this.kingdoms.get(kingdomId);
    k.team = team;
    this.emit('team', k);
  }

  teamOf(thing) {
    return this.kingdoms.get(thing.kingdomId)?.team;
  }

  allEntities() {
    return [...this.castles.values(), ...this.npcs.values()];
  }

  footprintHalf(e) {
    return e.kind === 'castle' ? FOOTPRINT_HALF.castle : npcSpec(e.npcKind).footprintHalf;
  }

  /** The castle or NPC whose footprint covers (x, y), or null. */
  entityAt(x, y) {
    let best = null, bd = Infinity;
    for (const e of this.index.query({ x0: x - 3, y0: y - 3, x1: x + 3, y1: y + 3 })) {
      const h = this.footprintHalf(e);
      if (Math.abs(e.x - x) <= h + 0.5 && Math.abs(e.y - y) <= h + 0.5) {
        const d = (e.x - x) ** 2 + (e.y - y) ** 2;
        if (d < bd) { bd = d; best = e; }
      }
    }
    return best;
  }

  /** Bounding rect (nodes) of everything a kingdom owns, or null when it owns nothing. */
  kingdomBounds(id) {
    const k = this.kingdoms.get(id);
    const pts = k ? [...k.castles, ...k.npc] : [];
    return pts.length ? rectOf(pts) : null;
  }

  /** What the renderer should draw: only entities inside the window (nodes). */
  queryWindow(rect) {
    const castles = [], npcs = [];
    for (const e of this.index.query(rect)) (e.kind === 'castle' ? castles : npcs).push(e);
    return { castles, npcs };
  }

  // ---- marches ----------------------------------------------------------------------------------------------------------------------------------
  /**
   * An attack (or any march) from one point to another. `duration` is seconds of travel and fully decides when it arrives;
   * `startedAt` (world seconds, default now) may be in the past to show a march already under way.
   */
  sendMarch({ from, to, duration, startedAt = this.now, kingdomId, kind = 'attack', units = 13 }) {
    if (!(duration > 0)) throw new Error('march needs a positive duration (seconds)');
    const march = { id: `march${this.nextId++}`, from: P(from), to: P(to), startedAt, duration, endAt: startedAt + duration, kingdomId, kind, units };
    this.marches.set(march.id, march);
    let lo = 0, hi = this.queue.length;
    while (lo < hi) { const mid = (lo + hi) >> 1; if (this.queue[mid].endAt <= march.endAt) lo = mid + 1; else hi = mid; }
    this.queue.splice(lo, 0, march);
    this.emit('march', march);
    return march;
  }

  /** Move the world clock; marches whose time is up are removed and announced with 'arrive'. */
  setNow(t) {
    this.now = t;
    while (this.queue.length && this.queue[0].endAt <= t) {
      const m = this.queue.shift();
      this.marches.delete(m.id);
      this.emit('arrive', m);
    }
  }

  advance(dt) {
    this.setNow(this.now + dt);
  }

  /** 0 before it starts, 1 on arrival: linear in time, so speed along any route is constant. */
  progress(m) {
    return Math.min(1, Math.max(0, (this.now - m.startedAt) / m.duration));
  }

  /** Straight-line position and heading (logic level; the drawn route may bend, the timing does not). */
  positionOf(m) {
    const p = this.progress(m);
    return { x: m.from.x + (m.to.x - m.from.x) * p, y: m.from.y + (m.to.y - m.from.y) * p, heading: Math.atan2(m.to.x - m.from.x, m.to.y - m.from.y) };
  }

  /** Marches that have started and whose straight-line box touches the window. */
  marchesInWindow(rect) {
    const out = [];
    for (const m of this.marches.values()) {
      if (this.now < m.startedAt) continue;
      if (intersects(grow(rectOf([m.from, m.to]), 2), rect)) out.push(m);
    }
    return out;
  }
}
