// Polylines for rivers and roads: Catmull-Rom smoothing, arc length, and a bucketed segment index for nearest-distance queries.
export function smoothPolyline(points, step = 0.5) {
  const P = points.map((p) => (Array.isArray(p) ? { x: p[0], y: p[1] } : p));
  if (P.length < 3) return densify(P, step);
  const out = [];
  for (let i = 0; i < P.length - 1; i++) {
    const p0 = P[Math.max(0, i - 1)], p1 = P[i], p2 = P[i + 1], p3 = P[Math.min(P.length - 1, i + 2)];
    const n = Math.max(2, Math.ceil(Math.hypot(p2.x - p1.x, p2.y - p1.y) / step));
    for (let s = 0; s < n; s++) {
      const t = s / n, t2 = t * t, t3 = t2 * t;
      const f = (a, b, c, d) => 0.5 * (2 * b + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t2 + (-a + 3 * b - 3 * c + d) * t3);
      out.push({ x: f(p0.x, p1.x, p2.x, p3.x), y: f(p0.y, p1.y, p2.y, p3.y) });
    }
  }
  out.push({ ...P[P.length - 1] });
  return out;
}

function densify(P, step) {
  const out = [];
  for (let i = 0; i < P.length - 1; i++) {
    const n = Math.max(1, Math.ceil(Math.hypot(P[i + 1].x - P[i].x, P[i + 1].y - P[i].y) / step));
    for (let s = 0; s < n; s++) out.push({ x: P[i].x + ((P[i + 1].x - P[i].x) * s) / n, y: P[i].y + ((P[i + 1].y - P[i].y) * s) / n });
  }
  out.push({ ...P[P.length - 1] });
  return out;
}

/** A smoothed polyline with cumulative arc length and nearest-point queries (bucketed, so cost is local). */
export class Path {
  constructor(points, { step = 0.5, reach = 6, bucket = 8 } = {}) {
    this.pts = smoothPolyline(points, step);
    this.cum = [0];
    for (let i = 1; i < this.pts.length; i++) this.cum.push(this.cum[i - 1] + Math.hypot(this.pts[i].x - this.pts[i - 1].x, this.pts[i].y - this.pts[i - 1].y));
    this.length = this.cum[this.cum.length - 1];
    this.reach = reach;
    this.bucket = bucket;
    this.cells = new Map();
    for (let i = 0; i < this.pts.length - 1; i++) {
      const a = this.pts[i], b = this.pts[i + 1];
      const x0 = Math.floor((Math.min(a.x, b.x) - reach) / bucket), x1 = Math.floor((Math.max(a.x, b.x) + reach) / bucket);
      const y0 = Math.floor((Math.min(a.y, b.y) - reach) / bucket), y1 = Math.floor((Math.max(a.y, b.y) + reach) / bucket);
      for (let by = y0; by <= y1; by++) for (let bx = x0; bx <= x1; bx++) {
        const k = bx * 100003 + by;
        (this.cells.get(k) ?? this.cells.set(k, []).get(k)).push(i);
      }
    }
  }

  /** Nearest point on the path: { d, s (arc length), x, y, dx, dy (unit direction) }, or null when farther than `reach`. */
  nearest(x, y) {
    const list = this.cells.get(Math.floor(x / this.bucket) * 100003 + Math.floor(y / this.bucket));
    if (!list) return null;
    let best = null;
    for (const i of list) {
      const a = this.pts[i], b = this.pts[i + 1];
      const vx = b.x - a.x, vy = b.y - a.y, l2 = vx * vx + vy * vy || 1e-9;
      const t = Math.min(1, Math.max(0, ((x - a.x) * vx + (y - a.y) * vy) / l2));
      const px = a.x + vx * t, py = a.y + vy * t, d = Math.hypot(x - px, y - py);
      if (!best || d < best.d) { const l = Math.sqrt(l2); best = { d, s: this.cum[i] + t * l, x: px, y: py, dx: vx / l, dy: vy / l }; }
    }
    return best && best.d <= this.reach ? best : null;
  }

  pointAt(s) {
    s = Math.min(Math.max(s, 0), this.length);
    let i = 1;
    while (i < this.cum.length - 1 && this.cum[i] < s) i++;
    const a = this.pts[i - 1], b = this.pts[i], seg = this.cum[i] - this.cum[i - 1] || 1, t = (s - this.cum[i - 1]) / seg;
    return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, dx: (b.x - a.x) / seg, dy: (b.y - a.y) / seg };
  }
}
