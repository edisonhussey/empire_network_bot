import assert from "node:assert/strict";
import test from "node:test";

import {
  buildDraft, canSend, clampCount, coinsLabel, durationLabel, jobTitle, phaseLabel, skipOptions, targetOptions,
} from "./transfer-model.js";

test("durations read the way a journey is thought of", () => {
  assert.equal(durationLabel(7200), "2 h");
  assert.equal(durationLabel(5400), "1 h 30 min");
  assert.equal(durationLabel(300), "5 min");
  assert.equal(durationLabel(45), "45 s");
  assert.equal(durationLabel(-3), "0 s");
  assert.equal(durationLabel(undefined), "0 s");
});

test("coins are grouped and counts never leave what the castle has", () => {
  assert.equal(coinsLabel(2500), "2,500");
  assert.equal(clampCount("20", 20), 20);
  assert.equal(clampCount("25", 20), 20);
  assert.equal(clampCount("-4", 20), 0);
  assert.equal(clampCount("abc", 20), 0);
  assert.equal(clampCount(3.9, 20), 3);
});

test("targets are the unlocked kingdoms other than the source", () => {
  const kingdoms = [
    { kingdom_id: 0, unlocked: true, tax_percent: 70, travel_s: 7200, in_flight: null },
    { kingdom_id: 3, unlocked: true, tax_percent: 50, travel_s: 7200, in_flight: { remaining_s: 100 } },
    { kingdom_id: 4, unlocked: false, tax_percent: 40, travel_s: 7200, in_flight: null },
  ];
  const options = targetOptions(kingdoms, 0);
  assert.deepEqual(options.map((option) => option.id), [3]);
  assert.match(options[0].label, /Fire Peaks \(50% tax, 2 h\)/);
  assert.equal(options[0].travellingNow, true);
});

test("the request keeps only units with a count and passes the skip choice through", () => {
  const draft = buildDraft({ castleId: 16011862, targetKingdomId: 3, counts: { 655: "20", 277: 0, 9: "" }, skip: "MS5" });
  assert.deepEqual(draft, { source_castle_id: 16011862, target_kingdom_id: 3, units: [[655, 20]], skip: "MS5" });
  assert.equal(buildDraft({ castleId: 1, targetKingdomId: 2, counts: {}, skip: "" }).skip, null);
});

test("skip choices say how many a journey uses and leave out hopeless ones", () => {
  const skips = [
    { key: "MS1", seconds: 60, stock: 3786 },
    { key: "MS5", seconds: 3600, stock: 996 },
    { key: "MS7", seconds: 86400, stock: null },
  ];
  const options = skipOptions(skips, 7200);
  assert.deepEqual(options.map((option) => option.key), ["", "MS5", "MS7"], "120 one-minute skips are not offered");
  assert.match(options[0].label, /Wait it out \(2 h\)/);
  assert.match(options[1].label, /uses 2; 996 in stock/);
  assert.match(options[2].label, /stock unknown/);
});

test("history lines and phases are readable", () => {
  const job = { units: [[655, 20]], source_kingdom_id: 0, target_kingdom_id: 3, phase: "travelling" };
  assert.equal(jobTitle(job, () => "Travelling Knights"), "20 Travelling Knights: Green to Fire Peaks");
  assert.equal(phaseLabel("travelling"), "On its way");
  assert.equal(phaseLabel("odd"), "odd");
});

test("sending needs a clean check and no transfer already open", () => {
  const clean = { problems: [] };
  assert.equal(canSend(clean, { jobs: [{ phase: "done" }] }), true);
  assert.equal(canSend(clean, { jobs: [{ phase: "queued" }] }), false);
  assert.equal(canSend({ problems: [{ message: "x" }] }, { jobs: [] }), false);
  assert.equal(canSend(null, { jobs: [] }), false);
});
