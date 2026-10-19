// A polyline of {x, y} points with arc-length lookup: where a marching squad is, and which way it faces.
export class Polyline {
  constructor(points) {
    if (points.length === 1) points = [points[0], points[0]];   // two castles whose rings touch: a zero-length route
    this.points = points;
    this.cum = [0];
    for (let i = 1; i < points.length; i++) this.cum.push(this.cum[i - 1] + Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y));
    this.length = this.cum[this.cum.length - 1];
  }

  /** Position at arc length `d` (clamped) and heading (radians, atan2(dx, dy): 0 faces +y, i.e. world +z). */
  pointAt(d) {
    d = Math.min(Math.max(d, 0), this.length);
    let i = 1;
    while (i < this.cum.length - 1 && this.cum[i] < d) i++;
    const a = this.points[i - 1], b = this.points[i];
    const seg = this.cum[i] - this.cum[i - 1] || 1;
    const t = (d - this.cum[i - 1]) / seg;
    return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, heading: Math.atan2(b.x - a.x, b.y - a.y) };
  }

  /** The points from arc length `d` to the end (first point interpolated): the part of the route still to walk. */
  remaining(d) {
    if (d <= 0) return this.points.slice();
    if (d >= this.length) return [this.points[this.points.length - 1]];
    const p = this.pointAt(d);
    const out = [{ x: p.x, y: p.y }];
    for (let i = 1; i < this.points.length; i++) if (this.cum[i] > d) out.push(this.points[i]);
    return out;
  }
}
