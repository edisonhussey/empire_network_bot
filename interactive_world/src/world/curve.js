// Corner rounding for drawn routes: each interior corner becomes a small quadratic curve, so squads swing round obstacles instead of
// pivoting on a point. The cut never reaches further than `radius` nodes from the corner (the router keeps spare clearance for that).
export function roundCorners(points, radius = 1, steps = 5) {
  if (points.length < 3) return points.slice();
  const out = [points[0]];
  for (let i = 1; i < points.length - 1; i++) {
    const a = points[i - 1], b = points[i], c = points[i + 1];
    const la = Math.hypot(b.x - a.x, b.y - a.y), lc = Math.hypot(c.x - b.x, c.y - b.y);
    const d = Math.min(radius, la / 2, lc / 2);
    if (d < 1e-6) { out.push(b); continue; }
    const p0 = { x: b.x + ((a.x - b.x) / la) * d, y: b.y + ((a.y - b.y) / la) * d };
    const p2 = { x: b.x + ((c.x - b.x) / lc) * d, y: b.y + ((c.y - b.y) / lc) * d };
    for (let s = 0; s <= steps; s++) {
      const t = s / steps, u = 1 - t;
      out.push({ x: u * u * p0.x + 2 * u * t * b.x + t * t * p2.x, y: u * u * p0.y + 2 * u * t * b.y + t * t * p2.y });
    }
  }
  out.push(points[points.length - 1]);
  return out;
}
