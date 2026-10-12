import assert from "node:assert/strict";
import test from "node:test";

import { PALETTE, assignColours, botsRunning, chooseViewed, stateLabel, statusFor, worldMates, worldOf } from "./account-context.js";

const accounts = [
  { account_id: "pingpoko", endpoint: "wss://world2" },
  { account_id: "ventrilo", endpoint: "wss://us1" },
  { account_id: "mate", endpoint: "wss://us1" },
];
const live = (account_id, extra = {}) => ({ account_id, connected: true, phase: "sands_ready", ...extra });

test("the remembered account is shown when it still exists, else a signed-in one, else the first", () => {
  assert.equal(chooseViewed("Ventrilo", accounts, []), "ventrilo");
  assert.equal(chooseViewed("gone", accounts, [live("mate")]), "mate");
  assert.equal(chooseViewed(null, accounts, []), "pingpoko");
  assert.equal(chooseViewed(null, [], []), null);
});

test("colours are stable: an account keeps its colour and a new one takes the lowest free", () => {
  const first = assignColours({}, ["ventrilo", "pingpoko"]);
  assert.deepEqual(first, { ventrilo: 0, pingpoko: 1 });
  const later = assignColours(first, ["Ventrilo", "pingpoko", "mate"]);
  assert.deepEqual(later, { ventrilo: 0, pingpoko: 1, mate: 2 });
  const gap = assignColours({ a: 0, c: 2 }, ["b"]);
  assert.equal(gap.b, 1, "fills a gap rather than recolouring anyone");
});

test("more accounts than colours reuse them instead of failing", () => {
  const ids = Array.from({ length: PALETTE.length + 2 }, (_, index) => `acct${index}`);
  const colours = assignColours({}, ids);
  assert.ok(Object.values(colours).every((index) => index >= 0 && index < PALETTE.length));
});

test("an account with no session still reads as itself, disconnected", () => {
  assert.deepEqual(statusFor([live("mate")], "ventrilo"), { connected: false, phase: "disconnected", account_id: "ventrilo", bot_state: "stopped" });
  assert.equal(statusFor([live("mate")], "MATE").connected, true);
});

test("accounts share a world when they share an endpoint, and only signed-in ones are mates", () => {
  assert.equal(worldOf(accounts, "ventrilo"), "wss://us1");
  assert.deepEqual(worldMates(accounts, [live("ventrilo"), live("mate"), live("pingpoko")], "ventrilo").map((a) => a.account_id), ["ventrilo", "mate"]);
  assert.deepEqual(worldMates(accounts, [live("ventrilo")], "ventrilo").map((a) => a.account_id), ["ventrilo"]);
  assert.deepEqual(worldMates(accounts, [live("mate")], "ventrilo").map((a) => a.account_id), ["mate"], "a signed-out viewed account is not in the set");
  assert.deepEqual(worldMates(accounts, [live("pingpoko")], "nobody"), []);
});

test("the switcher says plainly what each account is doing", () => {
  assert.deepEqual(stateLabel(live("a"), true), { text: "bot running", tone: "ok" });
  assert.deepEqual(stateLabel(live("a"), false), { text: "connected", tone: "live" });
  assert.equal(stateLabel({ connected: true, phase: "loading_sands" }, false).tone, "warn");
  assert.equal(stateLabel({ connected: false, phase: "failed" }, false).tone, "bad");
  assert.deepEqual(stateLabel({ connected: false, phase: "disconnected" }, false), { text: "offline", tone: "off" });
});

test("an account counts as running when any of its bots is, and not for another account's bot", () => {
  const library = { account_modes: [{ account_id: "a", running: false }], account_recruit_bots: [{ account_id: "b", running: true }], account_station_bots: [{ account_id: "a", running: true }] };
  assert.equal(botsRunning(library, "A"), true, "the station alone counts");
  assert.equal(botsRunning(library, "b"), true);
  assert.equal(botsRunning(library, "c"), false);
  assert.equal(botsRunning({}, "a"), false);
});
