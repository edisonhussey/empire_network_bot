import assert from "node:assert/strict";
import test from "node:test";

import { barSegments, commanderRanges, followPrevious, moveBlock, needed, problems, toRequest, withRange } from "./commander-ranges.js";

const entry = (task_id, commander_count, commander_start = null) => ({ task_id, commander_count, commander_start });
const spans = (entries) => commanderRanges(entries).map((range) => [range.first, range.last]);

test("ranges follow each other, so three tasks of ten read 1-10, 11-20, 21-30", () => {
  assert.deepEqual(spans([entry("a", 10), entry("b", 10), entry("c", 10)]), [[1, 10], [11, 20], [21, 30]]);
});

test("a task placed at a number sits there, leaves the gap unused, and the next one follows it", () => {
  assert.deepEqual(spans([entry("a", 10), entry("b", 10, 21), entry("c", 5)]), [[1, 10], [21, 30], [31, 35]]);
  assert.equal(needed(commanderRanges([entry("a", 10), entry("b", 10, 21)])), 30);
  assert.equal(needed([]), 0);
});

test("order on the list need not be numeric order", () => {
  assert.deepEqual(spans([entry("a", 3, 5), entry("b", 2, 1)]), [[5, 7], [1, 2]]);
});

test("editing from and to sets an explicit start and the span, never below one", () => {
  assert.deepEqual(withRange(entry("a", 1), 11, 20), { task_id: "a", commander_start: 11, commander_count: 10 });
  assert.deepEqual(withRange(entry("a", 5), 7, 3), { task_id: "a", commander_start: 7, commander_count: 1 }, "a to below from is a span of one");
  assert.equal(withRange(entry("a", 5), 0, 0).commander_start, 1);
  assert.equal(withRange(entry("a", 5), "abc", "").commander_count, 1);
  assert.equal(followPrevious(entry("a", 4, 9)).commander_start, null);
});

test("two tasks on the same commander are reported, as are starts below one", () => {
  assert.deepEqual(problems(commanderRanges([entry("a", 10), entry("b", 5, 8)])), [{ kind: "overlap", index: 0, with: 1 }]);
  assert.deepEqual(problems(commanderRanges([entry("a", 10), entry("b", 10, 11)])), []);
  assert.deepEqual(problems(commanderRanges([entry("a", 1, 0)])), [{ kind: "start", index: 0 }]);
  assert.deepEqual(problems(commanderRanges([entry("a", 1, 3), entry("b", 1, 3)])), [{ kind: "overlap", index: 0, with: 1 }]);
});

test("the request carries a start only when one was chosen", () => {
  assert.deepEqual(toRequest([entry("a", 10), entry("b", 10, 21), entry("c", 0)]), [
    { task_id: "a", commander_count: 10 },
    { task_id: "b", commander_count: 10, commander_start: 21 },
    { task_id: "c", commander_count: 1 },
  ]);
});

test("the bar lists tasks and the unused numbers between them, in number order", () => {
  const segments = barSegments(commanderRanges([entry("a", 10), entry("b", 10, 21), entry("c", 2)]));
  assert.deepEqual(segments.map((s) => [s.kind, s.first, s.last]), [["task", 1, 10], ["gap", 11, 20], ["task", 21, 30], ["task", 31, 32]]);
  assert.deepEqual(barSegments(commanderRanges([entry("a", 2, 5)])).map((s) => [s.kind, s.first, s.last]), [["gap", 1, 4], ["task", 5, 6]]);
});

const sup = (commander_count, commander_start = null) => ({ kind: "support", task_id: "", commander_count, commander_start });

test("support slots sit among the tasks: support 1-9, a task at 10, support 11-20", () => {
  const ranges = commanderRanges([sup(9), entry("a", 1), sup(10)]);
  assert.deepEqual(ranges.map((r) => [r.kind, r.first, r.last]), [["support", 1, 9], ["task", 10, 10], ["support", 11, 20]]);
  assert.equal(needed(ranges), 20);
  assert.deepEqual(problems(ranges), []);
});

test("a support slot is sent with no task and the support flag", () => {
  assert.deepEqual(toRequest([sup(9), entry("a", 1, 10)]), [
    { task_id: "", commander_count: 9, support: true },
    { task_id: "a", commander_count: 1, commander_start: 10 },
  ]);
});

test("moving a block right trades places with its neighbour and keeps sizes and gaps", () => {
  const moved = moveBlock([entry("a", 3), entry("b", 2)], 0, 1);
  assert.deepEqual(moved.map((e) => [e.task_id, e.commander_start, e.commander_count]), [["b", 1, 2], ["a", 3, 3]]);
  const gap = moveBlock([entry("a", 3), entry("b", 2, 8)], 1, -1);
  assert.deepEqual(gap.map((e) => [e.task_id, e.commander_start]), [["b", 1], ["a", 7]], "b takes 1-2, the gap of four stays, a follows at 7-9");
});

test("moving at either end changes nothing, and moving works on support blocks too", () => {
  const list = [entry("a", 2), entry("b", 2)];
  assert.equal(moveBlock(list, 0, -1), list);
  assert.equal(moveBlock(list, 1, 1), list);
  const moved = moveBlock([sup(2), entry("a", 1)], 1, -1);
  assert.deepEqual(moved.map((e) => [e.kind || "task", e.commander_start, e.commander_count]), [["task", 1, 1], ["support", 2, 2]]);
});
