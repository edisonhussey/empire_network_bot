/// Pure helpers for the Resources tab (no DOM). The rules themselves live in the daemon
/// (`empire-core/src/refinery.rs`); this only labels and checks what the form sends.

/// The refinery's recipes, by the names the API uses. Each craft: 54,400 raw to 100 refined, one hour.
export const REFINERY_RECIPES = [
  { id: "refined_wood", label: "Refined wood", raw: "wood" },
  { id: "refined_stone", label: "Refined stone", raw: "stone" },
];

export function recipeLabel(id) {
  return REFINERY_RECIPES.find((recipe) => recipe.id === id)?.label ?? id;
}

/// What one visit to the refinery does with these recipes (two places: one producing, one waiting).
export function describeRecipes(recipes) {
  if (!recipes.length) return "nothing";
  if (recipes.length === 1) return `2 lots of ${recipeLabel(recipes[0]).toLowerCase()} per visit`;
  return `${recipes.map((id) => recipeLabel(id).toLowerCase()).join(" + ")}, one lot each per visit`;
}

/// The create-resource request body, or what is wrong with the form.
export function presetDraft(name, checked) {
  if (!name.trim()) return { error: "Give the preset a name." };
  const recipes = REFINERY_RECIPES.filter((recipe) => checked[recipe.id]).map((recipe) => recipe.id);
  if (!recipes.length) return { error: "Tick refined wood, refined stone or both." };
  return { draft: { name: name.trim(), building: "refinery", recipes } };
}

/// "next visit in 1 h 52 min" / "due now".
export function nextVisitLabel(nextDueMs, nowMs = Date.now()) {
  const left = nextDueMs - nowMs;
  if (!nextDueMs || left <= 0) return "due now";
  const minutes = Math.ceil(left / 60_000);
  const hours = Math.floor(minutes / 60);
  return `next visit in ${hours ? `${hours} h ` : ""}${minutes % 60} min`;
}
