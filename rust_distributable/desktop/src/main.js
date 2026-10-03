const API = "http://127.0.0.1:47821/v1";
const status = document.querySelector("#status");
const gateway = document.querySelector("#gateway-label");
const phase = document.querySelector("#phase");
const rows = document.querySelector("#messages");
const result = document.querySelector("#result");
const directResult = document.querySelector("#direct-result");

function label(value) {
  return value.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

async function refresh() {
  try {
    const [healthResponse, directResponse, messagesResponse] = await Promise.all([
      fetch(`${API}/health`), fetch(`${API}/direct`), fetch(`${API}/messages?limit=50`),
    ]);
    if (!healthResponse.ok || !directResponse.ok || !messagesResponse.ok) throw new Error("local API error");
    const health = await healthResponse.json();
    const direct = await directResponse.json();
    const messages = await messagesResponse.json();
    status.textContent = direct.connected ? "Game socket connected" : "Local core ready";
    status.className = direct.connected ? "status connected" : "status";
    gateway.textContent = health.transport_connected ? "Transport connected" : "Local core connected";
    phase.textContent = label(direct.phase);
    if (direct.error) directResult.textContent = direct.error;
    rows.replaceChildren(...messages.map((message) => {
      const row = document.createElement("tr");
      const values = [message.sequence, label(message.direction), message.command ?? "system", new Date(message.observed_at_ms).toLocaleTimeString()];
      row.append(...values.map((value) => { const cell = document.createElement("td"); cell.textContent = value; return cell; }));
      return row;
    }));
  } catch (error) {
    status.textContent = "Local core offline";
    status.className = "status error";
    gateway.textContent = "Gateway unavailable";
  }
}

document.querySelector("#direct-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  directResult.textContent = "Opening direct game session…";
  const body = {
    endpoint: document.querySelector("#endpoint").value,
    credentials: {
      player_name: document.querySelector("#player-name").value,
      portal_account_id: document.querySelector("#portal-id").value,
      password: document.querySelector("#password").value || null,
      login_token: document.querySelector("#login-token").value || null,
      registration_token: document.querySelector("#registration-token").value || null,
    },
    settings: {
      server_header: "EmpireEx_21", client_version: "1169011", language: "en", platform_id: 1,
      connection_time: 676, round_trip_time: 118,
      map: { kingdom_id: 1, left: Number(document.querySelector("#map-x").value), top: Number(document.querySelector("#map-y").value), columns: 3, rows: 2 },
    },
  };
  const response = await fetch(`${API}/direct`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  const responseBody = await response.json();
  directResult.textContent = response.ok ? "Session accepted; waiting for the game server." : responseBody.error;
  if (response.ok) {
    document.querySelector("#password").value = "";
    document.querySelector("#login-token").value = "";
    document.querySelector("#registration-token").value = "";
  }
  refresh();
});

document.querySelector("#disconnect").addEventListener("click", async () => {
  await fetch(`${API}/direct`, { method: "DELETE" });
  directResult.textContent = "Disconnected.";
  refresh();
});

document.querySelector("#refresh").addEventListener("click", refresh);
document.querySelector("#inject-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  result.textContent = "Queueing…";
  const response = await fetch(`${API}/injections`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ packet: document.querySelector("#packet").value }) });
  const body = await response.json();
  result.textContent = response.ok ? `Queued ${body.id}` : body.error;
});

refresh();
setInterval(refresh, 2000);
