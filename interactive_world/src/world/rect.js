// Axis-aligned rectangles in nodes: { x0, y0, x1, y1 } (inclusive corners). Used for render windows, bounds and culling.
export const rectOf = (points) => {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of points) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  return { x0, y0, x1, y1 };
};
export const grow = (r, m) => ({ x0: r.x0 - m, y0: r.y0 - m, x1: r.x1 + m, y1: r.y1 + m });
export const intersects = (a, b) => a.x0 <= b.x1 && a.x1 >= b.x0 && a.y0 <= b.y1 && a.y1 >= b.y0;
export const contains = (r, x, y) => x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1;
export const center = (r) => ({ x: (r.x0 + r.x1) / 2, y: (r.y0 + r.y1) / 2 });
export const size = (r) => ({ w: r.x1 - r.x0, h: r.y1 - r.y0 });
/** Keep a rect within `radius` nodes of `c` (stops a horizon-wide camera window from asking for the whole map). */
export const limit = (r, c, radius) => ({ x0: Math.max(r.x0, c.x - radius), y0: Math.max(r.y0, c.y - radius), x1: Math.min(r.x1, c.x + radius), y1: Math.min(r.y1, c.y + radius) });
export const same = (a, b, eps = 0.5) => !!a && !!b && Math.abs(a.x0 - b.x0) < eps && Math.abs(a.y0 - b.y0) < eps && Math.abs(a.x1 - b.x1) < eps && Math.abs(a.y1 - b.y1) < eps;
