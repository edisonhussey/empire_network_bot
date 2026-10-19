// A* on the node grid (8 directions, no corner cutting) with several start and goal nodes, plus line-of-sight path smoothing.
// Pure functions of a Grid: swap the algorithm here without touching the world or the renderer.
const SQRT2 = Math.SQRT2;
const DIRS = [[1, 0, 1], [-1, 0, 1], [0, 1, 1], [0, -1, 1], [1, 1, SQRT2], [1, -1, SQRT2], [-1, 1, SQRT2], [-1, -1, SQRT2]];
const octile = (dx, dy) => { dx = Math.abs(dx); dy = Math.abs(dy); return Math.max(dx, dy) + (SQRT2 - 1) * Math.min(dx, dy); };

class MinHeap {
  constructor() { this.keys = []; this.items = []; }
  get size() { return this.items.length; }
  push(key, item) {
    const { keys, items } = this;
    let i = items.length;
    keys.push(key); items.push(item);
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (keys[p] <= key) break;
      keys[i] = keys[p]; items[i] = items[p]; i = p;
    }
    keys[i] = key; items[i] = item;
  }
  pop() {
    const { keys, items } = this;
    const top = items[0], lastKey = keys.pop(), lastItem = items.pop();
    const n = items.length;
    if (n) {
      let i = 0;
      for (;;) {
        let c = 2 * i + 1;
        if (c >= n) break;
        if (c + 1 < n && keys[c + 1] < keys[c]) c++;
        if (keys[c] >= lastKey) break;
        keys[i] = keys[c]; items[i] = items[c]; i = c;
      }
      keys[i] = lastKey; items[i] = lastItem;
    }
    return top;
  }
}

/**
 * True when the straight segment between two nodes crosses no blocked node square (sampled every quarter node). Each node owns the unit
 * square around it, so a line touching a corner of a blocked square counts as blocked: no squeezing diagonally between two obstacles.
 */
export function lineOfSight(grid, a, b) {
  const EPS = 1e-6;
  const steps = Math.ceil(Math.hypot(b.x - a.x, b.y - a.y) / 0.25);
  for (let i = 0; i <= steps; i++) {
    const t = steps ? i / steps : 0;
    const px = a.x + (b.x - a.x) * t, py = a.y + (b.y - a.y) * t;
    for (const x of new Set([Math.floor(px + 0.5 - EPS), Math.floor(px + 0.5 + EPS)])) {
      for (const y of new Set([Math.floor(py + 0.5 - EPS), Math.floor(py + 0.5 + EPS)])) if (grid.isBlocked(x, y)) return false;
    }
  }
  return true;
}

/** Cost of walking the straight segment a -> b over the grid's terrain costs (1 per node where the grid has none). */
function lineCost(grid, a, b) {
  const len = Math.hypot(b.x - a.x, b.y - a.y), steps = Math.max(1, Math.ceil(len / 0.5));
  let sum = 0;
  for (let i = 0; i < steps; i++) { const t = (i + 0.5) / steps; sum += (len / steps) * grid.costAt(Math.round(a.x + (b.x - a.x) * t), Math.round(a.y + (b.y - a.y) * t)); }
  return sum;
}

/**
 * Greedy string pulling: drop every node you can skip while keeping a clear straight line. On terrain with costs (roads) a shortcut is only
 * taken when it is not dearer than the path it replaces, so routes keep following roads instead of cutting across country.
 */
export function smoothPath(grid, nodes) {
  if (nodes.length < 3) return nodes;
  let prefix = null;
  if (grid.costAt) {
    prefix = [0];
    for (let k = 1; k < nodes.length; k++) prefix.push(prefix[k - 1] + lineCost(grid, nodes[k - 1], nodes[k]));
  }
  const out = [nodes[0]];
  let i = 0;
  while (i < nodes.length - 1) {
    let j = nodes.length - 1;
    while (j > i + 1 && (!lineOfSight(grid, nodes[i], nodes[j]) || (prefix && lineCost(grid, nodes[i], nodes[j]) > (prefix[j] - prefix[i]) * 1.04))) j--;
    out.push(nodes[j]);
    i = j;
  }
  return out;
}

// Scratch buffers reused between calls (a 1300x1300 map would otherwise allocate ~20 MB per route). `stamp` marks which entries belong to this call.
const scratch = { size: 0, g: null, from: null, stamp: null, closed: null, goal: null, gen: 0 };
function prepare(n) {
  if (scratch.size < n) {
    scratch.size = n;
    scratch.g = new Float32Array(n); scratch.from = new Int32Array(n);
    scratch.stamp = new Uint32Array(n); scratch.closed = new Uint32Array(n); scratch.goal = new Uint32Array(n);
    scratch.gen = 0;
  }
  return ++scratch.gen;
}

/**
 * Cheapest route from any of `starts` to any of `goals` (arrays of {x, y}). `aim` = { x, y, slack } steers the search: the heuristic is the
 * octile distance to `aim` minus `slack` (how far the goal nodes can be from it), which keeps it admissible.
 * Returns { nodes, cost } (grid-node path, then smoothed when `smooth`), null when there is no route, or { exceeded: true } when more than
 * `maxExpansions` nodes were needed (the caller decides what to draw instead).
 */
export function findPath(grid, { starts, goals, aim, smooth = true, maxExpansions = Infinity }) {
  const gen = prepare(grid.cells.length);
  const { g, from, stamp, closed, goal } = scratch;
  for (const n of goals) if (!grid.isBlocked(n.x, n.y)) goal[grid.index(n.x, n.y)] = gen;
  const minCost = grid.minCost ?? 1;   // cheapest terrain multiplier (roads): keeps the heuristic admissible
  const h = (x, y) => (aim ? minCost * Math.max(0, octile(x - aim.x, y - aim.y) - (aim.slack ?? 0)) : 0);
  const open = new MinHeap();
  for (const s of starts) {
    if (grid.isBlocked(s.x, s.y)) continue;
    const i = grid.index(s.x, s.y);
    stamp[i] = gen; g[i] = 0; from[i] = -1;
    open.push(h(s.x, s.y), i);
  }
  let expanded = 0;
  while (open.size) {
    const cur = open.pop();
    if (closed[cur] === gen) continue;
    closed[cur] = gen;
    if (++expanded > maxExpansions) return { exceeded: true };
    const cx = (cur % grid.n) + grid.min, cy = Math.floor(cur / grid.n) + grid.min;
    if (goal[cur] === gen) {
      const nodes = [];
      for (let i = cur; i !== -1; i = from[i]) nodes.push({ x: (i % grid.n) + grid.min, y: Math.floor(i / grid.n) + grid.min });
      nodes.reverse();
      return { nodes: smooth ? smoothPath(grid, nodes) : nodes, cost: g[cur] };
    }
    for (const [dx, dy, step] of DIRS) {
      const nx = cx + dx, ny = cy + dy;
      if (grid.isBlocked(nx, ny)) continue;
      if (dx && dy && (grid.isBlocked(cx + dx, cy) || grid.isBlocked(cx, cy + dy))) continue; // no squeezing past a corner
      const ni = grid.index(nx, ny);
      const cost = g[cur] + step * (grid.costAt ? grid.costAt(nx, ny) : 1);
      if (stamp[ni] !== gen || cost < g[ni]) {
        stamp[ni] = gen; g[ni] = cost; from[ni] = cur;
        open.push(cost + h(nx, ny), ni);
      }
    }
  }
  return null;
}
