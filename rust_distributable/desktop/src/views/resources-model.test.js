import { test } from "node:test";
import assert from "node:assert/strict";
import { describeRecipes, nextVisitLabel, presetDraft } from "./resources-model.js";

test("a preset needs a name and at least one recipe", () => {
  assert.equal(presetDraft(" ", { refined_wood: true }).error, "Give the preset a name.");
  assert.match(presetDraft("x", {}).error, /Tick/);
  assert.deepEqual(presetDraft(" both ", { refined_wood: true, refined_stone: true }).draft, { name: "both", building: "refinery", recipes: ["refined_wood", "refined_stone"] });
});

test("one recipe fills both places, two alternate", () => {
  assert.equal(describeRecipes(["refined_wood"]), "2 lots of refined wood per visit");
  assert.equal(describeRecipes(["refined_wood", "refined_stone"]), "refined wood + refined stone, one lot each per visit");
});

test("the next visit reads in hours and minutes", () => {
  assert.equal(nextVisitLabel(0, 5), "due now");
  assert.equal(nextVisitLabel(112 * 60_000, 0), "next visit in 1 h 52 min");
});
