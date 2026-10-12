import { element } from "../dom.js";
import { commanderRanges, needed } from "./commander-ranges.js";

/// What a saved preset is made of, in words, for Manage presets.
///
/// Every function here takes the same draft the preset is copied as (so what is shown
/// is exactly what would be exported) and returns plain sections:
/// `[{ title, rows: [{ label, value }] }]`. No DOM and no state, so each is tested on its
/// own (preset-details.test.js); `renderSections` turns them into elements.

const FLANKS = [["left", "Left flank"], ["middle", "Centre"], ["right", "Right flank"]];

const sum = (items) => items.reduce((total, item) => total + item.amount, 0);

/// "50 × Crossbowman, 20 × Archer".
function itemsText(items, itemName) {
  return items.map((item) => `${item.amount.toLocaleString()} × ${itemName(item.item_id)}`).join(", ");
}

/// An attack: wave by wave, flank by flank, troops and tools, with the totals.
export function attackDetails(draft, itemName) {
  const sections = [];
  let troops = 0;
  let tools = 0;
  draft.waves.forEach((wave, index) => {
    const rows = [];
    for (const [key, label] of FLANKS) {
      const side = wave[key] || { troops: [], tools: [] };
      troops += sum(side.troops);
      tools += sum(side.tools);
      if (side.troops.length) rows.push({ label: `${label} · troops`, value: itemsText(side.troops, itemName) });
      if (side.tools.length) rows.push({ label: `${label} · tools`, value: itemsText(side.tools, itemName) });
    }
    sections.push({ title: `Wave ${index + 1}`, rows: rows.length ? rows : [{ label: "", value: "Empty" }] });
  });
  sections.unshift({ title: "Total", rows: [{ label: "Troops", value: troops.toLocaleString() }, { label: "Tools", value: tools.toLocaleString() }] });
  return sections;
}

const TRAVEL = { coin: "Coin horse", ruby1: "Ruby horse (first)", ruby2: "Ruby horse (second)", feather: "Feather" };
const ALGORITHM = { advanced: "Advanced (spotlight, nearest first)", closest: "Closest first", random: "Random" };

function where(point, kingdomLabel) {
  return `${kingdomLabel(point.kingdom_id)} ${point.x}:${point.y}`;
}

/// What a task attacks, from where, with which attack and how.
export function taskDetails(draft, attackName, kingdomLabel) {
  const destination = draft.destination;
  let target;
  switch (destination.kind) {
    case "rbc_level_range": target = `Robber barons of level ${destination.minimum}–${destination.maximum} in ${kingdomLabel(destination.kingdom_id)}`; break;
    case "fortress_level_range": target = `Fortresses of level ${destination.minimum}–${destination.maximum} in ${kingdomLabel(destination.kingdom_id)}`; break;
    case "fortress": target = `Any fortress in ${kingdomLabel(destination.kingdom_id)}`; break;
    case "berimond_camp": target = "A Berimond camp"; break;
    default: target = `The fixed target ${where(destination, kingdomLabel)}`;
  }
  const source = draft.source_kind === "main_castle"
    ? `The main castle in ${kingdomLabel(draft.source.kingdom_id)}`
    : where(draft.source, kingdomLabel);
  return [{
    title: "Task",
    rows: [
      { label: "Attack", value: attackName(draft.attack_profile_id) },
      { label: "Sends from", value: source },
      { label: "Attacks", value: target },
      { label: "Chooses targets", value: ALGORITHM[draft.algorithm] || draft.algorithm },
      { label: "Travel", value: TRAVEL[draft.travel] || draft.travel },
      { label: "Priority", value: String(draft.priority).replaceAll("_", " ") },
    ],
  }];
}

/// An attack bot: which task owns which commanders.
/// Ranges kept for the bot's support read "Support", with the support's own name when known.
export function modeDetails(allocations, taskName, supportName = "Support") {
  const ranges = commanderRanges(allocations);
  const total = needed(ranges);
  return [{
    title: "Commanders",
    rows: [
      ...ranges.map((range) => ({ label: range.kind === "support" ? `${supportName} (kept free)` : taskName(range.task_id), value: `${range.first === range.last ? range.first : `${range.first}–${range.last}`} (${range.count})` })),
      { label: "Needs", value: `${total} commander${total === 1 ? "" : "s"}` },
    ],
  }];
}

export function recruitmentDetails(draft, itemName) {
  return [{
    title: "Recruitment",
    rows: [
      { label: "Troop", value: itemName(draft.troop_id) },
      { label: "Per order", value: `${draft.quantity.toLocaleString()} troops` },
      { label: "Orders in a row", value: String(draft.slot_count) },
      { label: "Alliance help", value: draft.ask_alliance_help ? "Asked for after the last order" : "Not asked for" },
    ],
  }];
}

const TEMPO = { advanced: "Advanced — randomised gaps", sporadic: "Sporadic — flat random gaps", greedy: "Greedy — as fast as the floor allows" };

export function recruitBotDetails(bot, recruitmentName, castleName) {
  return [{
    title: `Recruit bot · ${TEMPO[bot.algorithm] || bot.algorithm}`,
    rows: bot.castles.map((castle) => ({ label: castleName(castle.castle_id), value: recruitmentName(castle.recruitment_id) })),
  }];
}

export function stationBotDetails(bot, kingdomLabel) {
  return [{
    title: "Where support waits",
    rows: bot.kingdoms.map((kingdom) => ({ label: kingdomLabel(kingdom.kingdom_id), value: `${kingdom.x}:${kingdom.y}` })),
  }];
}

/// Sections as elements.
export function renderSections(sections) {
  const body = element("div", "preset-body");
  for (const section of sections) {
    const block = element("div", "preset-section");
    block.append(element("p", "list-label", section.title));
    const list = element("dl", "preset-rows");
    for (const row of section.rows) {
      const item = element("div");
      item.append(element("dt", "", row.label), element("dd", "", row.value));
      list.append(item);
    }
    block.append(list);
    body.append(block);
  }
  return body;
}
