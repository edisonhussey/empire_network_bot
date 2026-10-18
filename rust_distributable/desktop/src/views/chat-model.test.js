import assert from "node:assert/strict";
import test from "node:test";

import { changed, senderName, stickToBottom, timeLabel } from "./chat-model.js";

const at = (iso) => Date.parse(iso);

test("a message from today shows the time, from another day the date too", () => {
  const now = at("2026-10-17T12:00:00Z");
  assert.equal(timeLabel(at("2026-10-17T09:13:18Z"), now, "UTC"), "09:13");
  assert.equal(timeLabel(at("2026-10-17T00:05:00Z"), now, "UTC"), "00:05");
  assert.equal(timeLabel(at("2026-10-16T23:59:00Z"), now, "UTC"), "Oct 16 · 23:59");
});

test("the day boundary follows the reader's time zone, not UTC", () => {
  const now = at("2026-10-17T12:00:00Z");
  // In Auckland (UTC+13 in October) "now" is 01:00 on the 18th, so 12:30 on the 17th is yesterday...
  assert.equal(timeLabel(at("2026-10-16T23:30:00Z"), now, "Pacific/Auckland"), "Oct 17 · 12:30");
  // ...and 00:00 on the 18th is today, though it is still the 17th in UTC.
  assert.equal(timeLabel(at("2026-10-17T11:00:00Z"), now, "Pacific/Auckland"), "00:00");
});

test("senders are named, with a fallback and the game for system lines", () => {
  assert.equal(senderName({ player_name: "Dawn Reaver", player_id: 4616599 }), "Dawn Reaver");
  assert.equal(senderName({ player_name: "", player_id: 7 }), "Player 7");
  assert.equal(senderName({ system: true, player_name: "", player_id: -1 }), "Game");
});

test("a refresh redraws only when something new arrived", () => {
  const first = [{ id: 1 }, { id: 2 }];
  assert.equal(changed(null, first), true);
  assert.equal(changed(first, [{ id: 1 }, { id: 2 }]), false);
  assert.equal(changed(first, [...first, { id: 3 }]), true);
  assert.equal(changed(first, [{ id: 2 }, { id: 3 }]), true, "same count, newer last message");
  assert.equal(changed([], []), false);
});

test("a reader at the bottom follows new messages; one who scrolled up is left alone", () => {
  assert.equal(stickToBottom(560, 400, 1000), true);
  assert.equal(stickToBottom(565, 400, 1000), true, "within the slack");
  assert.equal(stickToBottom(520, 400, 1000), false, "just past it");
  assert.equal(stickToBottom(100, 400, 1000), false);
  assert.equal(stickToBottom(0, 400, 300), true, "a short list is always at the bottom");
});
