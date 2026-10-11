import assert from "node:assert/strict";
import test from "node:test";

import { ageLabel, needsRefresh, readable } from "./overview.js";

test("ages read in the unit people think in", () => {
  const now = 10_000_000;
  assert.equal(ageLabel(null, now), "never read");
  assert.equal(ageLabel(now - 2_000, now), "just now");
  assert.equal(ageLabel(now - 40_000, now), "40 s ago");
  assert.equal(ageLabel(now - 5 * 60_000, now), "5 min ago");
  assert.equal(ageLabel(now - 3 * 3_600_000, now), "3 h ago");
  assert.equal(ageLabel(now + 5_000, now), "just now", "never negative");
});

test("a reading is asked for when there is none or the newest is over two minutes old", () => {
  const now = 10_000_000;
  assert.equal(needsRefresh([], now), true);
  assert.equal(needsRefresh([{ observed_at_ms: null }], now), true);
  assert.equal(needsRefresh([{ observed_at_ms: now - 30_000 }, { observed_at_ms: null }], now), false);
  assert.equal(needsRefresh([{ observed_at_ms: now - 150_000 }], now), true);
});

test("counts keep their digits when small and compact when large", () => {
  assert.equal(readable(637.14), "637");
  assert.equal(readable(0), "0");
  assert.match(readable(2_500_000), /M|2/);
});
