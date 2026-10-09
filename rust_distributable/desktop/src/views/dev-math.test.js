import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_VIEW, MAX_VIEW, MIN_VIEW, endpoints, intervalFromSignal, makeViewport, movementState,
  panBy, pointOnPath, progressOf, shortDuration, signalAt, visible, zoomAbout,
} from "./dev-math.js";

const close = (a, b, epsilon = 1e-9) => assert.ok(Math.abs(a - b) < epsilon, `${a} vs ${b}`);

test("the wave sum matches the backend formula and stays inside the amplitude budget", () => {
  const waves = [
    { frequency_hz: 0.01, amplitude: 0.3, phase: 0 },
    { frequency_hz: 0.05, amplitude: 0.2, phase: 1 },
  ];
  close(signalAt(waves, 1_000, 1_000), 0.2 * Math.sin(1));
  // 25 s later the first wave is a quarter of the way round.
  close(signalAt([waves[0]], 0, 25_000), 0.3 * Math.sin(2 * Math.PI * 0.01 * 25));
  for (let t = 0; t < 600_000; t += 500) assert.ok(Math.abs(signalAt(waves, 0, t)) <= 0.5 + 1e-9);
});

test("the interval is the floor plus the baseline scaled by exp(signal)", () => {
  close(intervalFromSignal(0, 0.4, 1), 1.4);
  close(intervalFromSignal(Math.log(2), 0.4, 1.5), 3.4);
  assert.ok(intervalFromSignal(-5, 0.4, 1) > 0.4, "never below the floor");
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
