/// Pure geometry and timing for the Development tab. No DOM, no network, no clock:
/// everything takes the time it needs as an argument, so each function can be
/// checked on its own (see dev-math.test.js).

const TAU = Math.PI * 2;

/// The deterministic wave sum at `atMs`, evolved from the snapshot taken at
/// `snapshotMs` with the frequencies as they were then. Past the snapshot this is
/// a projection, not a promise: frequencies drift slowly on the backend.
export function signalAt(waves, snapshotMs, atMs) {
  const seconds = (atMs - snapshotMs) / 1000;
  return waves.reduce((sum, wave) => sum + wave.amplitude * Math.sin(wave.phase + TAU * wave.frequency_hz * seconds), 0);
}

/// The stochastic part of an interval for a wave value (noise excluded):
/// `D_min + D0 * exp(signal)`.
export function intervalFromSignal(signal, minInterval, baseline) {
  return minInterval + baseline * Math.exp(signal);
}

/// Fraction of a journey done at `now`, clamped to 0..1. `null` when the end is
/// unknown or the interval is empty: an unknown return is shown as unknown, never
/// estimated.
export function progressOf(startMs, endMs, nowMs) {
  if (endMs == null || endMs <= startMs) return null;
  return Math.min(1, Math.max(0, (nowMs - startMs) / (endMs - startMs)));
}

/// What to draw for one movement right now.
export function movementState(movement, nowMs) {
  const progress = progressOf(movement.start_ms, movement.end_ms, nowMs);
  const remainingMs = movement.end_ms == null ? null : Math.max(0, movement.end_ms - nowMs);
  let status = "active";
  if (movement.phase === "arrived") status = "arrived";
  else if (progress == null) status = "unknown";
  else if (progress >= 1) status = "completed";
  return { phase: movement.phase, progress, remainingMs, status };
}

/// The point `fraction` of the way from `from` to `to`.
export function pointOnPath(from, to, fraction) {
  return { x: from.x + (to.x - from.x) * fraction, y: from.y + (to.y - from.y) * fraction };
}

/// Which end a movement starts from and heads to, as map points.
export function endpoints(movement, castle) {
  const tower = { x: movement.x, y: movement.y };
  return movement.phase === "returning" ? { from: tower, to: castle } : { from: castle, to: tower };
}

/// A square window onto the map: `size` logical coordinates across the *shorter*
/// canvas side, centred on (`cx`, `cy`). One scale for both axes, so the map is
/// never stretched. Screen y grows downwards while map y is plotted the same way
/// the game does (down), so no flip is needed.
export function makeViewport(cx, cy, size, width, height) {
  const scale = Math.min(width, height) / size;
  const halfW = width / (2 * scale);
  const halfH = height / (2 * scale);
  return {
    cx, cy, size, scale,
    bounds: { left: cx - halfW, right: cx + halfW, top: cy - halfH, bottom: cy + halfH },
    toScreen: (x, y) => ({ x: width / 2 + (x - cx) * scale, y: height / 2 + (y - cy) * scale }),
    toMap: (sx, sy) => ({ x: cx + (sx - width / 2) / scale, y: cy + (sy - height / 2) / scale }),
  };
}

export const MIN_VIEW = 20;
export const MAX_VIEW = 1200;
export const DEFAULT_VIEW = 100;

export function clampSize(size) {
  return Math.min(MAX_VIEW, Math.max(MIN_VIEW, size));
}

/// Whether a point (padded by `margin` coordinates) is inside the window.
export function visible(viewport, x, y, margin = 0) {
  const b = viewport.bounds;
  return x >= b.left - margin && x <= b.right + margin && y >= b.top - margin && y <= b.bottom + margin;
}

/// Zoom by `factor` (>1 shows more map) keeping the map point under the cursor
/// fixed, which is what makes wheel zoom feel anchored.
export function zoomAbout(viewport, factor, anchor, width, height) {
  const size = clampSize(viewport.size * factor);
  const next = makeViewport(viewport.cx, viewport.cy, size, width, height);
  // Move the centre so `anchor` keeps its position relative to the window.
  const ratio = size / viewport.size;
  return makeViewport(anchor.x - (anchor.x - viewport.cx) * ratio, anchor.y - (anchor.y - viewport.cy) * ratio, next.size, width, height);
}

/// Pan by a screen-space drag.
export function panBy(viewport, dxPx, dyPx, width, height) {
  return makeViewport(viewport.cx - dxPx / viewport.scale, viewport.cy - dyPx / viewport.scale, viewport.size, width, height);
}

/// A compact duration for labels: "42s", "4m 12s", "1h 03m".
export function shortDuration(ms) {
  if (ms == null) return "?";
  const total = Math.max(0, Math.round(ms / 1000));
  if (total < 60) return `${total}s`;
  if (total < 3600) return `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, "0")}s`;
  return `${Math.floor(total / 3600)}h ${String(Math.floor((total % 3600) / 60)).padStart(2, "0")}m`;
}
