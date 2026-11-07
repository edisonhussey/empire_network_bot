import test from "node:test";
import assert from "node:assert/strict";
import { licenceState, planLabel, tokenFrom } from "./licence-model.js";

const now = 1_800_000_000_000;
const valid = (seconds) => ({ licence: { valid: true, stage: "activated", expires_at: now / 1000 + seconds } });

test("the plan line describes the account in view, never a countdown past expiry", () => {
  assert.equal(planLabel(valid(4 * 86400 + 3 * 3600), now), "4d 3h remaining");
  assert.equal(planLabel(valid(2 * 3600), now), "2h remaining");
  assert.equal(planLabel({ licence: { valid: false, reason: "licence has expired", expires_at: 1 } }, now), "Expired");
  assert.equal(planLabel({ licence: null }, now), "No licence");
  assert.equal(planLabel(null, now), "No licence");
});

test("the state line says what to drop when there is no licence", () => {
  assert.match(licenceState({ server: "US1", main_castle: [509, 405], licence: null }), /US1 with its main castle at 509:405/);
  assert.match(licenceState({ licence: { valid: false, reason: "licence has expired" } }), /expired/);
});

test("a token is found in a file's text or a paste", () => {
  assert.equal(tokenFrom("  OA2.abc.def\n"), "OA2.abc.def");
  assert.equal(tokenFrom("token: OA2.a.b"), "OA2.a.b");
  assert.equal(tokenFrom("nothing"), "");
});
