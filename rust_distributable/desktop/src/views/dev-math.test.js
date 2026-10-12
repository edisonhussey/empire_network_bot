import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_VIEW, MAX_VIEW, MIN_VIEW, endpoints, logPosition, makeViewport, movementState,
  PACE_MAX, PACE_MIN, panBy, paceToPosition, pointOnPath, positionToPace, progressOf, shareInBand, shortDuration,
  signalAt, stepProgress, visible, waitAt, waveValue, zoomAbout,
} from "./dev-math.js";

const close = (a, b, epsilon = 1e-9) => assert.ok(Math.abs(a - b) < epsilon, `${a} vs ${b}`);

test("a log axis spreads short waits and still holds the long tail", () => {
  close(logPosition(0.5, 0.5, 120), 0);
  close(logPosition(120, 0.5, 120), 1);
  close(logPosition(Math.sqrt(0.5 * 120), 0.5, 120), 0.5);
  assert.equal(logPosition(0.01, 0.5, 120), 0, "clamped below");
  assert.equal(logPosition(999, 0.5, 120), 1, "clamped above");
  // 1 s to 5 s takes real room on the axis, not a sliver.
  assert.ok(logPosition(5, 0.5, 120) - logPosition(1, 0.5, 120) > 0.25);
});

test("the share of waits in the 1 to 5 second band", () => {
  assert.equal(shareInBand([], 1, 5), null);
  assert.equal(shareInBand([0.8, 1, 2, 5, 5.01, 30], 1, 5), 3 / 6);
  assert.equal(shareInBand([2, 3, 4], 1, 5), 1);
});

test("progress is the clamped fraction and unknown stays unknown", () => {
  assert.equal(progressOf(1000, 3000, 1000), 0);
  assert.equal(progressOf(1000, 3000, 2000), 0.5);
  assert.equal(progressOf(1000, 3000, 9999), 1);
  assert.equal(progressOf(1000, 3000, 0), 0);
  assert.equal(progressOf(1000, null, 2000), null);
  assert.equal(progressOf(2000, 2000, 2500), null);
});

test("movements report remaining time, direction and an honest unknown state", () => {
  const castle = { x: 10, y: 10 };
  const out = { phase: "outbound", x: 50, y: 30, start_ms: 0, end_ms: 100_000 };
  assert.deepEqual(endpoints(out, castle), { from: castle, to: { x: 50, y: 30 } });
  const state = movementState(out, 25_000);
  assert.equal(state.progress, 0.25);
  assert.equal(state.remainingMs, 75_000);
  assert.equal(state.status, "active");

  const back = { phase: "returning", x: 50, y: 30, start_ms: 0, end_ms: 100_000 };
  assert.deepEqual(endpoints(back, castle), { from: { x: 50, y: 30 }, to: castle });
  assert.equal(movementState(back, 100_001).status, "completed");

  const unknown = { phase: "returning", x: 50, y: 30, start_ms: 0, end_ms: null };
  const unknownState = movementState(unknown, 5000);
  assert.equal(unknownState.status, "unknown");
  assert.equal(unknownState.progress, null);
  assert.equal(unknownState.remainingMs, null);

  assert.equal(movementState({ phase: "arrived", x: 1, y: 1, start_ms: 0, end_ms: null }, 5).status, "arrived");
});

test("a point on the path is linear in the fraction", () => {
  assert.deepEqual(pointOnPath({ x: 0, y: 0 }, { x: 10, y: 20 }, 0.5), { x: 5, y: 10 });
  assert.deepEqual(pointOnPath({ x: 4, y: 4 }, { x: 4, y: 4 }, 0.3), { x: 4, y: 4 });
});

test("the viewport keeps true proportions on any canvas shape", () => {
  const wide = makeViewport(500, 500, DEFAULT_VIEW, 800, 400);
  close(wide.scale, 4, 1e-9); // 100 coordinates across the shorter (400px) side
  // One coordinate step is the same length on both axes.
  const a = wide.toScreen(500, 500);
  const dx = wide.toScreen(501, 500).x - a.x;
  const dy = wide.toScreen(500, 501).y - a.y;
  close(dx, dy);
  assert.deepEqual(a, { x: 400, y: 200 });
  // The wider axis simply shows more map.
  assert.ok(wide.bounds.right - wide.bounds.left > wide.bounds.bottom - wide.bounds.top);
  close(wide.bounds.bottom - wide.bounds.top, 100);
  // And the transform round-trips.
  const back = wide.toMap(...Object.values(wide.toScreen(523.5, 481.25)));
  close(back.x, 523.5); close(back.y, 481.25);
});

test("only objects inside the window are drawn", () => {
  const view = makeViewport(500, 500, 100, 400, 400);
  assert.equal(visible(view, 500, 500), true);
  assert.equal(visible(view, 549, 451), true);
  assert.equal(visible(view, 551, 500), false);
  assert.equal(visible(view, 551, 500, 2), true, "a margin keeps edge markers whole");
});

test("zoom is clamped, anchored on the cursor, and pan moves the right way", () => {
  let view = makeViewport(500, 500, 100, 400, 400);
  const anchor = view.toMap(100, 100);
  const zoomed = zoomAbout(view, 2, anchor, 400, 400);
  assert.equal(zoomed.size, 200);
  const after = zoomed.toScreen(anchor.x, anchor.y);
  close(after.x, 100, 1e-6); close(after.y, 100, 1e-6);
  for (let i = 0; i < 30; i += 1) view = zoomAbout(view, 2, { x: 500, y: 500 }, 400, 400);
  assert.equal(view.size, MAX_VIEW);
  for (let i = 0; i < 60; i += 1) view = zoomAbout(view, 0.5, { x: 500, y: 500 }, 400, 400);
  assert.equal(view.size, MIN_VIEW);
  // Dragging the map right shows what is to the left.
  const panned = panBy(makeViewport(500, 500, 100, 400, 400), 40, -20, 400, 400);
  close(panned.cx, 490); close(panned.cy, 505);
});

test("durations read compactly", () => {
  assert.equal(shortDuration(42_000), "42s");
  assert.equal(shortDuration(252_000), "4m 12s");
  assert.equal(shortDuration(3_780_000), "1h 03m");
  assert.equal(shortDuration(null), "?");
  assert.equal(shortDuration(-5), "0s");
});

const timing = (extra = {}) => ({
  waves: [{ kind: "sin", amplitude: 1, omega: 1, phase: 0 }, { kind: "cos", amplitude: 0.5, omega: 2, phase: 0 }],
  floor_s: 0.75, log_offset: 0.5, max_interval_s: 120, pace: 1, ...extra,
});

test("each wave kind is the generator's own function", () => {
  close(waveValue({ kind: "sin", amplitude: 2, b: null }, Math.PI / 2), 2);
  close(waveValue({ kind: "cos", amplitude: 2 }, 0), 2);
  close(waveValue({ kind: "fold", amplitude: 3, b: 0.8 }, Math.PI / 2), 3, 1e-9);
  assert.ok(Math.abs(waveValue({ kind: "fold", amplitude: 1, b: 0.8 }, Math.PI / 4)) < 1, "bounded by the amplitude");
});

test("the signal is continuous: whole steps are the generator's, between them it carries on smoothly", () => {
  const { waves } = timing();
  close(signalAt(waves, 0), 0.5);
  close(signalAt(waves, Math.PI / 2), 1 + 0.5 * Math.cos(Math.PI));
  const steps = Array.from({ length: 100 }, (_, i) => signalAt(waves, i / 10));
  const largest = Math.max(...steps.slice(1).map((value, i) => Math.abs(value - steps[i])));
  assert.ok(largest < 0.2, `no jumps between samples: ${largest}`);
  assert.ok(signalAt(waves, -1) !== signalAt(waves, 1), "looking back is defined too");
});

test("a wait is the floor plus exp(offset + signal), times the pace, capped", () => {
  const t = timing();
  close(waitAt(t, 0), 0.75 + Math.exp(0.5 + 0.5));
  close(waitAt(t, 0, 2), 2 * waitAt(t, 0, 1));
  const huge = timing({ waves: [{ kind: "sin", amplitude: 50, omega: 1, phase: Math.PI / 2 }] });
  close(waitAt(huge, 0), 0.75 + (120 - 0.75), 1e-6);
  assert.ok(waitAt(timing({ waves: [{ kind: "sin", amplitude: 50, omega: 1, phase: -Math.PI / 2 }] }), 0) > 0.75, "never below the floor");
});

test("the chart's now slides through the wait in progress and sits still when nothing is scheduled", () => {
  assert.equal(stepProgress(1_000, 5_000, 1_000), 0);
  assert.equal(stepProgress(1_000, 5_000, 3_000), 0.5);
  assert.equal(stepProgress(1_000, 5_000, 9_000), 1, "overdue stays at the end");
  assert.equal(stepProgress(null, 5_000, 3_000), 0);
  assert.equal(stepProgress(1_000, null, 3_000), 0);
  assert.equal(stepProgress(5_000, 5_000, 6_000), 0);
});

test("the slider is log-scaled, bounded from below and lands exactly on 1", () => {
  close(paceToPosition(PACE_MIN), 0);
  close(paceToPosition(PACE_MAX), 1);
  close(paceToPosition(1) - paceToPosition(PACE_MIN), paceToPosition(PACE_MAX) - paceToPosition(2) , 0.2);
  assert.equal(positionToPace(-5), PACE_MIN);
  assert.equal(positionToPace(5), PACE_MAX);
  assert.equal(positionToPace(paceToPosition(1)), 1);
  assert.equal(positionToPace(paceToPosition(1.02)), 1, "snaps to the design pace");
  assert.equal(positionToPace(paceToPosition(2)), 2);
  assert.equal(paceToPosition(0.01), 0, "below the bound reads as the bound");
});
