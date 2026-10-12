/// Pure geometry and timing for the Development tab. No DOM, no network, no clock:
/// everything takes the time it needs as an argument, so each function can be
/// checked on its own (see dev-math.test.js).

/// Position of `seconds` on a log axis running from `low` to `high`, 0 at the
/// bottom and 1 at the top, clamped. A log axis is the right one for waits that are
/// mostly short with a long tail: it keeps both the bulk and the tail readable.
export function logPosition(seconds, low, high) {
  const span = Math.log(high) - Math.log(low);
  return Math.min(1, Math.max(0, (Math.log(Math.max(seconds, low)) - Math.log(low)) / span));
}

/// Share of `values` inside [low, high], as a fraction. `null` with no values.
export function shareInBand(values, low, high) {
  if (!values.length) return null;
  return values.filter((value) => value >= low && value <= high).length / values.length;
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

// ---- the wave chart: the generator's signal as a continuous curve ----------------

/// The test pace's bounds, as the service keeps them. 1 is the design.
export const PACE_MIN = 0.5;
export const PACE_MAX = 4;

/// One wave at phase `x`, as the generator computes it (`timing.rs`).
export function waveValue(wave, x) {
  if (wave.kind === "cos") return wave.amplitude * Math.cos(x);
  if (wave.kind === "fold") return wave.amplitude * Math.asin(wave.b * Math.sin(x)) / Math.asin(wave.b);
  return wave.amplitude * Math.sin(x);
}

/// The wave sum `offset` steps from the next wait to be generated, for any real
/// `offset`: whole numbers are the generator's own steps, between them the same waves
/// carried on smoothly. Negative offsets look back with the frequencies as they stand now.
export function signalAt(waves, offset) {
  let total = 0;
  for (const wave of waves) total += waveValue(wave, wave.phase + wave.omega * offset);
  return total;
}

/// The wait the generator would give at `offset`, seconds, without noise: the floor plus
/// `exp(offset + signal)`, capped, times the pace. The same arithmetic as `WaveTimer`.
export function waitAt(timing, offset, pace = timing.pace ?? 1) {
  const ceiling = Math.log(Math.max(1, (timing.max_interval_s ?? 120) - timing.floor_s));
  const exponent = Math.min(ceiling, Math.max(-6, timing.log_offset + signalAt(timing.waves, offset)));
  return (timing.floor_s + Math.exp(exponent)) * pace;
}

/// How far through the wait in progress it is, 0 to 1: the chart's "now" sits that far
/// past the last step, so it slides continuously between actions instead of jumping at
/// each one. 0 when nothing is scheduled.
export function stepProgress(startedAtMs, dueAtMs, nowMs) {
  if (startedAtMs == null || dueAtMs == null || dueAtMs <= startedAtMs) return 0;
  return Math.min(1, Math.max(0, (nowMs - startedAtMs) / (dueAtMs - startedAtMs)));
}

/// The slider, on a log scale so 0.5 and 2 are equally far from 1. `position` is 0 at the
/// bottom (shortest waits) and 1 at the top.
export function paceToPosition(pace) {
  const clamped = Math.min(PACE_MAX, Math.max(PACE_MIN, pace));
  return (Math.log(clamped) - Math.log(PACE_MIN)) / (Math.log(PACE_MAX) - Math.log(PACE_MIN));
}

/// The pace at a slider position, kept within the bounds, and snapped to exactly 1 when
/// close to it so the design pace is easy to land on again.
export function positionToPace(position) {
  const clamped = Math.min(1, Math.max(0, position));
  const pace = Math.exp(Math.log(PACE_MIN) + clamped * (Math.log(PACE_MAX) - Math.log(PACE_MIN)));
  return Math.abs(pace - 1) < 0.04 ? 1 : Math.round(pace * 100) / 100;
}
