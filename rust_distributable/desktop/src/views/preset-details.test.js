import assert from "node:assert/strict";
import test from "node:test";

import { attackDetails, modeDetails, recruitBotDetails, recruitmentDetails, stationBotDetails, taskDetails } from "./preset-details.js";

const itemName = (id) => ({ 602: "Crossbowman", 614: "Scaling ladder" })[id] || `Item ${id}`;
const kingdom = (id) => ({ 0: "Green", 1: "Burning Sands" })[id] || `Kingdom ${id}`;

const wave = (left = [], middle = [], right = [], tools = []) => ({
  left: { troops: left, tools: [] }, middle: { troops: middle, tools }, right: { troops: right, tools: [] },
});

test("an attack reads wave by wave and flank by flank, with totals, skipping what is empty", () => {
  const draft = { name: "a", waves: [wave([{ item_id: 602, amount: 50 }], [{ item_id: 602, amount: 20 }], [], [{ item_id: 614, amount: 40 }]), wave(), wave(), wave()] };
  const sections = attackDetails(draft, itemName);
  assert.deepEqual(sections[0], { title: "Total", rows: [{ label: "Troops", value: "70" }, { label: "Tools", value: "40" }] });
  assert.equal(sections[1].title, "Wave 1");
  assert.deepEqual(sections[1].rows, [
    { label: "Left flank · troops", value: "50 × Crossbowman" },
    { label: "Centre · troops", value: "20 × Crossbowman" },
    { label: "Centre · tools", value: "40 × Scaling ladder" },
  ]);
  assert.deepEqual(sections[2].rows, [{ label: "", value: "Empty" }]);
  assert.equal(sections.length, 5);
});

const task = (destination, extra = {}) => ({ name: "t", attack_profile_id: "p1", source: { kingdom_id: 1, x: 593, y: 613 }, source_kind: "coordinate", destination, algorithm: "advanced", travel: "coin", priority: "medium", ...extra });

test("a task says what it attacks, from where, with which attack and how", () => {
  const rows = taskDetails(task({ kind: "rbc_level_range", kingdom_id: 1, minimum: 35, maximum: 60 }), () => "crossbow rbc", kingdom)[0].rows;
  assert.deepEqual(rows.map((row) => row.value), ["crossbow rbc", "Burning Sands 593:613", "Robber barons of level 35–60 in Burning Sands", "Advanced (spotlight, nearest first)", "Coin horse", "medium"]);
});

test("each kind of destination and a main-castle source are described", () => {
  const sections = (destination, extra) => taskDetails(task(destination, extra), () => "a", kingdom)[0].rows;
  assert.equal(sections({ kind: "fortress", kingdom_id: 1 })[2].value, "Any fortress in Burning Sands");
  assert.equal(sections({ kind: "fortress_level_range", kingdom_id: 1, minimum: 40, maximum: 55 })[2].value, "Fortresses of level 40–55 in Burning Sands");
  assert.equal(sections({ kind: "coordinate", kingdom_id: 0, x: 5, y: 6 })[2].value, "The fixed target Green 5:6");
  assert.equal(sections({ kind: "berimond_camp", kingdom_id: 10 })[2].value, "A Berimond camp");
  assert.equal(sections({ kind: "fortress", kingdom_id: 1 }, { source_kind: "main_castle" })[1].value, "The main castle in Burning Sands");
});

test("an attack bot lists the commanders each task owns and how many it needs in all", () => {
  const rows = modeDetails([{ task_id: "a", commander_count: 17 }, { task_id: "b", commander_count: 1, commander_start: 20 }], (id) => `task ${id}`)[0].rows;
  assert.deepEqual(rows, [{ label: "task a", value: "1–17 (17)" }, { label: "task b", value: "20 (1)" }, { label: "Needs", value: "20 commanders" }]);
});

test("commanders kept for support are listed by number between the tasks, under the support's name", () => {
  const rows = modeDetails([{ task_id: "", support: true, commander_count: 9 }, { task_id: "a", commander_count: 1, commander_start: 10 }, { task_id: "", support: true, commander_count: 10, commander_start: 11 }], (id) => `task ${id}`, "Pingpoko")[0].rows;
  assert.deepEqual(rows, [{ label: "Pingpoko (kept free)", value: "1–9 (9)" }, { label: "task a", value: "10 (1)" }, { label: "Pingpoko (kept free)", value: "11–20 (10)" }, { label: "Needs", value: "20 commanders" }]);
});

test("recruitments, recruit bots and support bots read in words", () => {
  assert.deepEqual(recruitmentDetails({ troop_id: 602, quantity: 100, slot_count: 3, ask_alliance_help: true }, itemName)[0].rows.map((r) => r.value), ["Crossbowman", "100 troops", "3", "Asked for after the last order"]);
  const bot = recruitBotDetails({ algorithm: "greedy", castles: [{ castle_id: 1, recruitment_id: "r1" }] }, () => "100 × crossbow", (id) => `Castle ${id}`)[0];
  assert.equal(bot.title, "Recruit bot · Greedy — as fast as the floor allows");
  assert.deepEqual(bot.rows, [{ label: "Castle 1", value: "100 × crossbow" }]);
  assert.deepEqual(stationBotDetails({ kingdoms: [{ kingdom_id: 0, x: 468, y: 507 }] }, kingdom)[0].rows, [{ label: "Green", value: "468:507" }]);
});
