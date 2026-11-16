import { test } from "node:test";
import assert from "node:assert/strict";
import { apiBase } from "./platform.js";

const at = (url) => new URL(url);

test("the Vite dev server talks to the daemon on its fixed port", () => {
  assert.equal(apiBase(at("http://localhost:1420/"), {}), "http://127.0.0.1:47821/v1");
});

test("a page the daemon served uses its own origin", () => {
  assert.equal(apiBase(at("http://127.0.0.1:47821/"), {}), "http://127.0.0.1:47821/v1");
  assert.equal(apiBase(at("http://127.0.0.1:47900/"), {}), "http://127.0.0.1:47900/v1");
});

test("the Tauri origin falls back to the fixed port", () => {
  assert.equal(apiBase(at("tauri://localhost/"), {}), "http://127.0.0.1:47821/v1");
});

test("VITE_EMPIRE_API overrides everything", () => {
  assert.equal(apiBase(at("http://localhost:1420/"), { VITE_EMPIRE_API: "http://127.0.0.1:5000/v1/" }), "http://127.0.0.1:5000/v1");
});
