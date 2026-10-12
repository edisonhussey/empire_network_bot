import assert from "node:assert/strict";
import test from "node:test";

import { accountsToDraw, involvement, mergedThreats } from "./shared-layers.js";

const accounts = [
  { account_id: "ventrilo", player_id: 1, viewed: true, pvp: [] },
  { account_id: "pingpoko", player_id: 2, pvp: [] },
];

test("a threat belongs to the account it targets, else the one that sent it, else its recorder", () => {
  assert.deepEqual(involvement({ target_id: 2, sender_id: 1 }, "ventrilo", accounts), { owner: "pingpoko", rank: 2 });
  assert.deepEqual(involvement({ target_id: 99, sender_id: 1 }, "pingpoko", accounts), { owner: "ventrilo", rank: 1 });
  assert.deepEqual(involvement({ target_id: 99, sender_id: 98 }, "pingpoko", accounts), { owner: "pingpoko", rank: 0 });
  assert.deepEqual(involvement({ target_id: null, sender_id: null }, "ventrilo", accounts), { owner: "ventrilo", rank: 0 });
});

test("a movement recorded by two allied accounts is drawn once, for the account it concerns", () => {
  const row = { mid: 5, target_id: 2, sender_id: 99 };
  const shared = [
    { ...accounts[0], pvp: [row] },
    { ...accounts[1], pvp: [row] },
  ];
  const merged = mergedThreats(shared);
  assert.equal(merged.length, 1);
  assert.equal(merged[0].owner, "pingpoko");
  assert.equal(merged[0].rank, 2);
});

test("distinct movements are all kept and an alliance-wide sighting stays with its recorder", () => {
  const shared = [
    { ...accounts[0], pvp: [{ mid: 1, target_id: 77, sender_id: 88 }, { mid: 2, target_id: 1, sender_id: 3 }] },
    accounts[1],
  ];
  const merged = mergedThreats(shared);
  assert.deepEqual(merged.map((item) => [item.row.mid, item.owner, item.rank]), [[1, "ventrilo", 0], [2, "ventrilo", 2]]);
});

test("a world-mate with nothing in the kingdom is left off its map; the viewed account never is", () => {
  const list = [
    { account_id: "a", viewed: true },
    { account_id: "b", castle: null, movements: [], pvp: [], station: [] },
    { account_id: "c", castle: [1, 2] },
    { account_id: "d", movements: [{}] },
  ];
  assert.deepEqual(accountsToDraw(list).map((account) => account.account_id), ["a", "c", "d"]);
});
