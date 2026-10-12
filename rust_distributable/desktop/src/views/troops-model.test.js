import assert from "node:assert/strict";
import test from "node:test";

import { armyText, liveState, outcomeLabel, sortRows, summarise, until } from "./troops-model.js";

const row = (number, state, extra = {}) => ({ number, state, free_at_ms: null, lands_at_ms: null, troop_total: 0, coin_loot: null, ruby_loot: null, ...extra });

test("rows stay in commander-number order whatever their state", () => {
  const rows = [row(4, "idle"), row(2, "returning", { free_at_ms: 900 }), row(3, "outbound", { free_at_ms: 500 }), row(1, "idle")];
  assert.deepEqual(sortRows(rows).map((item) => item.number), [1, 2, 3, 4]);
});

test("a hold that has run out reads as inactive before the service says so", () => {
  assert.equal(liveState(row(1, "returning", { free_at_ms: 1000 }), 1000), "idle");
  assert.equal(liveState(row(1, "returning", { free_at_ms: 1001 }), 1000), "returning");
  assert.equal(liveState(row(1, "idle"), 5), "idle");
});

test("the totals count troops and loot only for commanders still out", () => {
  const rows = [
    row(1, "outbound", { free_at_ms: 9_000, troop_total: 50 }),
    row(2, "returning", { free_at_ms: 9_000, troop_total: 40, coin_loot: 1_200, ruby_loot: 3 }),
    row(3, "returning", { free_at_ms: 500, troop_total: 99, coin_loot: 7, ruby_loot: 7 }),
    row(4, "station", { free_at_ms: 9_000, troop_total: 20 }),
    row(5, "idle"),
  ];
  assert.deepEqual(summarise(rows, 1_000), { outbound: 1, returning: 1, sending: 0, station: 1, idle: 2, troops_out: 90, coins: 1_200, rubies: 3 });
});

test("a countdown reads in the unit people think in, and never negative", () => {
  assert.equal(until(null, 0), "–");
  assert.equal(until(0, 5_000), "now");
  assert.equal(until(42_000, 0), "in 42s");
  assert.equal(until(252_000, 0), "in 4m 12s");
  assert.equal(until(3 * 3_600_000 + 5 * 60_000, 0), "in 3h 05m");
  assert.equal(until(252_000, 0, true), "≤ 4m 12s", "an estimate is a bound");
  assert.equal(until(0, 5_000, true), "now");
  assert.equal(until(null, 0, true), "–");
});

test("an army reads largest first and says how many more there are", () => {
  assert.equal(armyText([]), "–");
  assert.equal(armyText([{ count: 50, name: "crossbowman_602" }]), "50 × crossbowman 602");
  assert.equal(armyText([{ count: 5, name: "a" }, { count: 4, name: "b" }, { count: 3, name: "c" }, { count: 2, name: "d" }]), "5 × a, 4 × b, 3 × c +1 more");
});

test("finished marches are named by what happened", () => {
  assert.equal(outcomeLabel({ status: "returning" }), "returned");
  assert.equal(outcomeLabel({ status: "sent" }), "out");
  assert.equal(outcomeLabel({ status: "lost_army" }), "lost army");
});
