// OpenAuto lab — control panel logic.
//
// Two rules shape this file:
//
//   * it never talks to the daemon directly, only through /api/... on the lab
//     server, which owns the allowlist and the rate gate;
//   * it shows what came back rather than summarising it. When a probe is sent
//     the raw response is kept and the numbers on screen are computed from it,
//     so a wrong verdict is visible instead of inferred.

const $ = (selector) => document.querySelector(selector);

const state = {
  messages: [],
  commands: new Set(),
  lastProbe: null, // { kid, asked, sentAtMs }
  daemon: { api_version: null, reachable: false },
};

// --- transport --------------------------------------------------------------

async function call(path, options) {
  const response = await fetch(`/api/${path}`, options);
  const text = await response.text();
  let body;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = { raw: text };
  }
  return { status: response.status, ok: response.ok, body };
}

// The gate lives in the lab server, so asking about it costs no daemon budget.
async function refreshGate() {
  const { body } = await call("gate");
  if (!body || typeof body.used !== "number") return;
  const free = body.used >= body.limit ? ` · free in ${body.free_in_seconds}s` : "";
  const node = $("#gate");
  node.textContent = `gate ${body.used}/${body.limit} per ${body.window_seconds}s${free}`;
  node.classList.toggle("busy", body.used >= body.limit);
}

// --- the packet log ---------------------------------------------------------

function directionLabel(value) {
  if (value === "server_to_client") return "S→C";
  if (value === "client_to_server") return "C→S";
  return value || "—";
}

function clock(ms) {
  const date = new Date(ms);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
}

function payloadText(payload) {
  if (payload === null || payload === undefined) return "";
  const text = JSON.stringify(payload);
  return text.length > 240 ? `${text.slice(0, 240)}…` : text;
}

function renderLog() {
  const body = $("#log tbody");
  body.replaceChildren();
  const rows = state.messages.slice(0, 400);
  for (const message of rows) {
    const row = document.createElement("tr");
    const cells = [
      String(message.sequence),
      clock(message.observed_at_ms),
      directionLabel(message.direction),
      message.command || "—",
    ];
    for (const text of cells) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    const payload = document.createElement("td");
    // The daemon already parsed the frame, so this is the JSON it saw, not a
    // raw string that needs un-escaping to be readable.
    payload.className = "payload";
    payload.textContent = payloadText(message.payload);
    payload.title = JSON.stringify(message.payload);
    row.append(payload);
    body.append(row);
  }
  const shown = rows.length;
  $("#log-meta").textContent = `${shown} of ${state.messages.length} recorded packets shown`;
}

function renderFilterOptions() {
  const select = $("#filter");
  const current = select.value;
  const options = ["", ...state.commands];
  if (select.dataset.built === options.join("|")) return;
  select.replaceChildren();
  for (const value of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = value || "all";
    select.append(option);
  }
  select.value = state.commands.has(current) ? current : "";
  select.dataset.built = options.join("|");
}

async function loadMessages() {
  const limit = Math.max(10, Math.min(2000, Number($("#limit").value) || 300));
  const query = new URLSearchParams({ limit: String(limit) });
  const { ok, status, body } = await call(`messages?${query}`);
  if (!ok || !Array.isArray(body)) {
    $("#log-meta").textContent = `read failed (${status}): ${JSON.stringify(body)}`;
    return;
  }
  state.messages = body;
  for (const message of body) {
    if (message.command) state.commands.add(message.command);
  }
  renderFilterOptions();
  renderLog();
  describeLastProbe();
}

// --- the window ladder ------------------------------------------------------

function readProbeInput() {
  return {
    kid: Number($("#kid").value),
    ax1: Number($("#ax1").value),
    ay1: Number($("#ay1").value),
  };
}

function gaaPacket(kid, ax1, ay1, span) {
  const payload = {
    KID: kid,
    AX1: ax1,
    AY1: ay1,
    AX2: ax1 + span - 1,
    AY2: ay1 + span - 1,
  };
  return `%xt%EmpireEx_21%gaa%1%${JSON.stringify(payload)}%`;
}

async function inject(packet, ttl_ms) {
  return call("injections", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ packet, ttl_ms }),
  });
}

async function sendWindow(span) {
  const { kid, ax1, ay1 } = readProbeInput();
  const packet = gaaPacket(kid, ax1, ay1, span);
  state.lastProbe = { kid, span, ax1, ay1, sentAtMs: Date.now() };
  $("#probe-result").textContent = `sent span ${span} for KID ${kid} — waiting for the reply…`;
  const { ok, status, body } = await inject(packet, 15000);
  if (!ok) {
    $("#probe-result").textContent = `refused (${status}): ${JSON.stringify(body)}`;
    state.lastProbe = null;
    return;
  }
  $("#probe-result").textContent = `queued (id ${body.id}) — reading the reply…`;
  // Give the socket a moment, then read. The gate paces this for us.
  window.setTimeout(() => { loadMessages().catch(() => {}); }, 1200);
}

// Measures the reply to the most recent probe against what was asked for. The
// numbers are computed from the stored payload, so nothing here is assumed.
function describeLastProbe() {
  const probe = state.lastProbe;
  if (!probe) return;
  const match = state.messages.find((message) =>
    message.direction === "server_to_client"
    && message.command === "gaa"
    && message.payload?.KID === probe.kid
    && message.observed_at_ms >= probe.sentAtMs - 2000);
  if (!match) {
    $("#probe-result").textContent =
      `span ${probe.span}: no reply recorded yet. Injected packets only reach a live socket.`;
    return;
  }
  const rows = Array.isArray(match.payload?.AI) ? match.payload.AI : [];
  const xs = rows.map((row) => row?.[1]).filter((value) => typeof value === "number");
  const ys = rows.map((row) => row?.[2]).filter((value) => typeof value === "number");
  const fortresses = rows.filter((row) => row?.[0] === 11).length;
  if (!xs.length) {
    $("#probe-result").textContent =
      `span ${probe.span}: the reply carried no objects (${rows.length} rows).`;
    state.lastProbe = null;
    return;
  }
  const spanX = Math.max(...xs) - Math.min(...xs) + 1;
  const spanY = Math.max(...ys) - Math.min(...ys) + 1;
  const verdict = spanX >= probe.span && spanY >= probe.span
    ? "honoured in full"
    : `clipped to ${spanX}×${spanY}`;
  $("#probe-result").textContent =
    `asked ${probe.span}×${probe.span} at ${probe.ax1},${probe.ay1} → ` +
    `returned ${spanX}×${spanY} spanning x ${Math.min(...xs)}..${Math.max(...xs)}, ` +
    `${rows.length} objects (${fortresses} type-11) — ${verdict}`;
  state.lastProbe = null;
}

// --- wiring -----------------------------------------------------------------

function buildLadder() {
  const spans = [42, 62, 101, 140, 218];
  const node = $("#ladder");
  for (const span of spans) {
    const button = document.createElement("button");
    button.textContent = `${span}`;
    button.title = `ask for a ${span}×${span} window`;
    button.addEventListener("click", () => sendWindow(span).catch((error) => {
      $("#probe-result").textContent = String(error);
    }));
    node.append(button);
  }
}

function wire() {
  $("#refresh").addEventListener("click", () => loadMessages().catch(() => {}));
  $("#filter").addEventListener("change", () => applyFilter());
  $("#direction").addEventListener("change", () => applyFilter());
  $("#send-custom").addEventListener("click", () => {
    sendWindow(Number($("#span").value) || 42).catch((error) => {
      $("#probe-result").textContent = String(error);
    });
  });
  $("#send-packet").addEventListener("click", async () => {
    const packet = $("#packet").value.trim();
    // Injections reach a live game socket; make that explicit rather than a
    // stray click.
    if (!window.confirm(`Inject this packet into the live session?\n\n${packet}`)) return;
    const { ok, status, body } = await inject(packet, Number($("#ttl").value) || 15000);
    $("#packet-result").textContent = ok
      ? `queued: ${JSON.stringify(body)}`
      : `refused (${status}): ${JSON.stringify(body)}`;
  });
  for (const button of document.querySelectorAll("[data-read]")) {
    button.addEventListener("click", async () => {
      const path = button.dataset.read;
      const { status, body } = await call(path);
      $("#state").textContent = `GET /v1/${path} -> ${status}\n\n${JSON.stringify(body, null, 2)}`;
      if (path === "health") {
        state.daemon = { api_version: body?.api_version ?? null, reachable: true };
        paintDaemon();
      }
    });
  }
}

// The log filter is applied by re-reading a filtered view rather than hiding
// rows client-side, so what is on screen matches what the daemon recorded.
function applyFilter() {
  const command = $("#filter").value;
  const direction = $("#direction").value;
  const rows = state.messages.filter((message) =>
    (!command || message.command === command)
    && (!direction || message.direction === direction));
  const body = $("#log tbody");
  body.replaceChildren();
  for (const message of rows.slice(0, 400)) {
    const row = document.createElement("tr");
    for (const text of [
      String(message.sequence),
      clock(message.observed_at_ms),
      directionLabel(message.direction),
      message.command || "—",
    ]) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    const payload = document.createElement("td");
    payload.className = "payload";
    payload.textContent = payloadText(message.payload);
    payload.title = JSON.stringify(message.payload);
    row.append(payload);
    body.append(row);
  }
  $("#log-meta").textContent =
    `${rows.length} of ${state.messages.length} packets match ` +
    `${command || "any command"} / ${direction ? directionLabel(direction) : "both directions"}`;
}

function paintDaemon() {
  const node = $("#daemon");
  if (!state.daemon.reachable) {
    node.textContent = "daemon: not read";
    return;
  }
  node.textContent = `api_version ${state.daemon.api_version}`;
}

async function loop() {
  await refreshGate();
  if ($("#follow").checked) {
    await loadMessages().catch(() => {});
  }
  window.setTimeout(loop, 2000);
}

buildLadder();
wire();
paintDaemon();
call("health").then(({ body }) => {
  state.daemon = { api_version: body?.api_version ?? null, reachable: true };
  paintDaemon();
}).catch(() => {});
loop();
