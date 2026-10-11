import assert from "node:assert/strict";
import test from "node:test";

import { bandShare, histogramBars, jobStatus, seconds, supportMarker, timeline } from "./station-model.js";

const close = (a, b, epsilon = 1e-9) => assert.ok(Math.abs(a - b) < epsilon, `${a} vs ${b}`);

const job = (overrides = {}) => ({
  phase: "away", landing_ms: 100_000, cds_ack_ms: 94_000, support_duration_s: 4509,
  recall_due_ms: 102_000, mcm_ack_ms: null, return_eta_ms: null, outcome: null, doing: "", ...overrides,
});

test("nothing is drawn as moving before the support has been acknowledged", () => {
  assert.deepEqual(supportMarker(job({ cds_ack_ms: null }), 90_000), { phase: "preparing", fraction: 0, remainingMs: null });
});

test("out, the march covers a sliver of a long journey and counts down to the recall", () => {
  const marker = supportMarker(job(), 99_000);
  assert.equal(marker.phase, "out");
  close(marker.fraction, 5_000 / (4509 * 1000));
  assert.equal(marker.remainingMs, 3_000);
  assert.equal(supportMarker(job(), 120_000).remainingMs, 0, "never negative");
});

test("after the recall it walks back the same sliver, then is home", () => {
  const recalled = job({ mcm_ack_ms: 102_000, return_eta_ms: 112_000 });
  const turned = 8_000 / (4509 * 1000);
  const back = supportMarker(recalled, 107_000);
  assert.equal(back.phase, "back");
  close(back.fraction, turned * 0.5);
  assert.equal(back.remainingMs, 5_000);
  assert.equal(supportMarker(recalled, 112_000).phase, "home");
  assert.equal(supportMarker(recalled, 500_000).phase, "home");
});

test("a missing duration places the troops at the castle instead of guessing", () => {
  assert.equal(supportMarker(job({ support_duration_s: null }), 99_000).fraction, 0);
});

test("a job reads as saved, late, cancelled or in trouble at a glance", () => {
  assert.deepEqual(jobStatus(job({ phase: "done", outcome: "saved" })), { text: "saved", tone: "ok" });
  assert.equal(jobStatus(job({ phase: "done", outcome: "saved:recall_uncertain" })).tone, "warn");
  assert.equal(jobStatus(job({ phase: "done", outcome: "late" })).tone, "bad");
  assert.equal(jobStatus(job({ phase: "done", outcome: "cancelled:retreat" })).tone, "quiet");
  assert.equal(jobStatus(job({ phase: "done", outcome: "blocked:no_destination" })).tone, "warn");
  assert.equal(jobStatus(job({ phase: "done", outcome: "refused:cds:104" })).tone, "bad");
  assert.equal(jobStatus(job({ phase: "uncertain" })).tone, "bad");
  assert.equal(jobStatus(job({ phase: "waiting", doing: "next: reading castle stock (dcl)" })).text, "next: reading castle stock (dcl)");
});

test("times read as milliseconds under a second and seconds above", () => {
  assert.equal(seconds(310), "310 ms");
  assert.equal(seconds(4375), "4.4 s");
  assert.equal(seconds(null), "–");
  assert.equal(seconds(2000, true), "+2.0 s");
});

test("the timeline lists only the steps that happened, as time before the landing", () => {
  const steps = timeline(job({ dcl_sent_ms: 92_000, sdi_sent_ms: 94_000, cds_sent_ms: null, cds_ack_ms: 95_000 }));
  assert.deepEqual(steps, [["dcl", 8_000], ["sdi", 6_000], ["cds acknowledged", 5_000]]);
});

const histogram = {
  edges_s: [0.5, 1, 2, 5, 10],
  counts: [1, 2, 4, 6, 3, 4],
  total: 20,
};

test("bars give each bin's range and share, the last being open-ended", () => {
  const bars = histogramBars(histogram);
  assert.equal(bars.length, 6);
  assert.deepEqual([bars[0].low, bars[0].high], [0, 0.5]);
  assert.equal(bars[5].high, Infinity);
  close(bars[3].share, 0.3);
  close(bars.reduce((sum, bar) => sum + bar.share, 0), 1);
});

test("the share in a band counts whole bins and takes a straddling bin in proportion", () => {
  close(bandShare(histogram, 1, 5), (4 + 6) / 20, 1e-9);
  const partial = bandShare(histogram, 1.5, 5);
  assert.ok(partial > 6 / 20 && partial < (4 + 6) / 20, `${partial}`);
  assert.equal(bandShare({ edges_s: [1], counts: [0, 0], total: 0 }, 1, 5), null);
});
