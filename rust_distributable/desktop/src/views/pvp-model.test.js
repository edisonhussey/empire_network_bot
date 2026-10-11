import assert from "node:assert/strict";
import test from "node:test";

import { arrowFor, countOf, liveState, mergeRows, partyLabel } from "./pvp-model.js";

const row = (overrides = {}) => ({
  mid: 1, direction: "incoming", state: "active", detected_at_ms: 1_000_000, land_at_ms: 1_057_000,
  duration_s: 57, sender_x: 543, sender_y: 520, target_x: 546, target_y: 520,
  sender_name: "great emperor 9", sender_alliance: "top secret", target_name: "WIZRAD007", target_alliance: "Electi",
  ...overrides,
});

test("a movement lands on the clock without a new packet, and a retreat is never undone", () => {
  assert.equal(liveState(row(), 1_056_999), "active");
  assert.equal(liveState(row(), 1_057_000), "landed");
  assert.equal(liveState(row({ state: "retreated" }), 0), "retreated");
  assert.equal(liveState(row({ state: "retreated" }), 9_999_999), "retreated");
});

test("merging keeps one row per MID, newest first, and takes the fresher copy", () => {
  const held = [row({ mid: 1, detected_at_ms: 100 }), row({ mid: 2, detected_at_ms: 200 })];
  const merged = mergeRows(held, [row({ mid: 2, detected_at_ms: 200, state: "retreated" }), row({ mid: 3, detected_at_ms: 200 })]);
  assert.deepEqual(merged.map((item) => item.mid), [3, 2, 1], "same instant: higher MID first");
  assert.equal(merged[1].state, "retreated");
});

test("counts add up over directions and states, and over nothing", () => {
  const counts = [
    { direction: "incoming", state: "active", count: 2 },
    { direction: "incoming", state: "landed", count: 5 },
    { direction: "outgoing", state: "landed", count: 3 },
  ];
  assert.equal(countOf(counts), 10);
  assert.equal(countOf(counts, "incoming"), 7);
  assert.equal(countOf(counts, "incoming", "active"), 2);
  assert.equal(countOf(counts, null, "landed"), 8);
  assert.equal(countOf(undefined), 0);
});

test("labels leave out what is not known instead of printing placeholders", () => {
  assert.equal(partyLabel("Ann", "Electi"), "Ann | Electi");
  assert.equal(partyLabel("Ann", null), "Ann");
  assert.equal(partyLabel(null, null), "unknown");
});

test("an active incoming movement is a red arrow part-way along its journey", () => {
  const arrow = arrowFor(row(), 1_028_500);
  assert.equal(arrow.colour, "in");
  assert.equal(arrow.progress, 0.5);
  assert.equal(arrow.remainingMs, 28_500);
  assert.deepEqual(arrow.from, { x: 543, y: 520 });
  assert.deepEqual(arrow.to, { x: 546, y: 520 });
  assert.equal(arrow.senderLabel, "great emperor 9 | top secret");
});

test("an alliance attack going out is green", () => {
  assert.equal(arrowFor(row({ direction: "outgoing" }), 1_010_000).colour, "out");
});

test("the colour follows whose attack it is: an ally's is green, anyone else's red", () => {
  assert.equal(arrowFor(row({ direction: "internal" }), 1_010_000).colour, "out", "ally to ally");
  assert.equal(arrowFor(row({ direction: "other" }), 1_010_000).colour, "in", "neither end is ours");
});

test("nothing is drawn once a movement has landed or was retreated", () => {
  assert.equal(arrowFor(row(), 1_057_000), null);
  assert.equal(arrowFor(row({ state: "retreated" }), 1_010_000), null);
});

test("progress is clamped before the journey starts and survives a zero duration", () => {
  assert.equal(arrowFor(row(), 990_000).progress, 0);
  assert.equal(arrowFor(row({ duration_s: 0, land_at_ms: 2_000_000 }), 1_500_000).progress, 1);
});
