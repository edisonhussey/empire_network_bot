import assert from "node:assert/strict";
import test from "node:test";

import { describeBot, destinationsFrom } from "./support-bots.js";

const row = (kingdom_id, enabled, x = "", y = "") => ({ kingdom_id, enabled, x, y });

test("only ticked kingdoms get a destination; the rest are left out, meaning none", () => {
  const result = destinationsFrom([row(0, true, "468", "507"), row(1, false, "600", "600"), row(2, true, "0", "1300")]);
  assert.deepEqual(result, { kingdoms: [{ kingdom_id: 0, x: 468, y: 507 }, { kingdom_id: 2, x: 0, y: 1300 }] });
});

test("a ticked kingdom needs both coordinates as whole numbers inside the map", () => {
  assert.match(destinationsFrom([row(0, true, "468", "")]).error, /both coordinates/);
  assert.match(destinationsFrom([row(0, true, "4.5", "7")]).error, /whole numbers/);
  assert.match(destinationsFrom([row(1, true, "-1", "7")]).error, /Burning Sands.*between 0 and 1300/);
  assert.match(destinationsFrom([row(1, true, "7", "1301")]).error, /between 0 and 1300/);
});

test("a bot with no destination at all is refused before it is sent", () => {
  assert.match(destinationsFrom([row(0, false), row(1, false)]).error, /at least one kingdom/);
});

test("a bot reads back as its kingdoms and coordinates", () => {
  assert.equal(describeBot({ kingdoms: [{ kingdom_id: 0, x: 468, y: 507 }, { kingdom_id: 4, x: 5, y: 6 }] }), "Green 468:507 · Storm 5:6");
});
