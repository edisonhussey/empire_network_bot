const API = '';
const app = document.querySelector('#app');
const state = { sessions: [], selected: null, rows: [], counts: {}, total: 0, offset: 0, types: new Set(), query: '', event: null, raw: false, status: null, busy: false, settings: null, tab: 'sessions', incoming: null, autoSupport: null, injectText: '', injectToClient: false };
const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const date = value => value ? new Date(value * 1000).toLocaleString() : '—';
const time = value => value ? new Date(value * 1000).toLocaleTimeString(undefined,{hour12:false, fractionalSecondDigits:3}) : '—';
const duration = item => item.ended_at ? `${Math.round(item.ended_at - item.started_at)}s` : 'Recoverable';
function decodeUnparsed(raw) {
  if (!raw) return null;
  if (raw.trimStart().startsWith('<')) {
    const xml = new DOMParser().parseFromString(raw, 'application/xml');
    if (xml.querySelector('parsererror')) return null;
    const root = xml.documentElement;
    const body = Array.from(root.children).find(child => child.tagName === 'body');
    const node = element => {
      const result = {tag: element.tagName};
      if (element.attributes.length) result.attributes = Object.fromEntries(Array.from(element.attributes, attribute => [attribute.name, attribute.value]));
      if (element.children.length) result.children = Array.from(element.children, node);
      else if (element.textContent.trim()) result.text = element.textContent.trim();
      return result;
    };
    return {format: 'xml', command: body?.getAttribute('action') || root.tagName, request_id: body?.getAttribute('r') ?? null, status: null, server_header: null, payload: node(root)};
  }
  const parts = raw.trim().replace(/^%|%$/g, '').split('%');
  if (parts.length === 4 && parts[0] === 'xt' && parts[1].startsWith('EmpireEx_'))
    return {command: parts[2], request_id: parts[3], status: null, server_header: parts[1], payload: null};
  return null;
}
async function api(path, method='GET', body) {
  const response = await fetch(API + path, {method, headers: {'Content-Type':'application/json'}, body: body === undefined ? undefined : JSON.stringify(body)});
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || response.statusText);
  return result;
}
function notice(message) { const bar = document.querySelector('#notice'); if (bar) { bar.textContent = message; bar.hidden = false; setTimeout(() => bar.hidden = true, 4500); } }
function element(tag, className, html) { const node = document.createElement(tag); if (className) node.className = className; node.innerHTML = html; return node; }
function header() {
  const status = state.status || {};
  return `<header><div class="brand"><span class="brand-mark">◇</span><div><strong>Proxy Packet Monitor</strong><small>Recorded session explorer</small></div></div><div class="toolbar"><span class="status ${status.connected?'connected':''}"><i></i>${status.connected ? 'Game connected' : status.proxy_running ? 'Proxy waiting for game' : 'Proxy stopped'}</span><span class="endpoint">${esc(status.endpoint || `127.0.0.1:${status.proxy_port || 8080}`)}</span><label class="capture" title="While on, the game's login downloads (items, config, language) are saved to ${esc(status.capture?.dir || 'web_data/login_data')}"><input type="checkbox" id="capture-toggle" ${status.capture_enabled?'checked':''}>Download login data${status.capture_enabled && status.proxy_running ? `<small>${status.capture?.saved ?? 0} saved · ${status.capture?.logins ?? 0} logins</small>` : ''}</label><label class="capture" title="Also keep the pictures the game loads (sprites and icons). Needs Download login data on; hard-reload the game after ticking. Audio and fonts are never kept."><input type="checkbox" id="images-toggle" ${status.capture_images?'checked':''}>Include images${status.capture_enabled && status.capture_images && status.proxy_running ? `<small>${status.capture?.megabytes ?? 0} MB so far</small>` : ''}</label><button id="proxy-button" class="secondary">${status.proxy_running ? 'Stop proxy' : 'Start proxy'}</button><button id="record-button" class="primary ${status.recording?'recording':''}" ${status.proxy_running?'':'disabled'}>${status.recording ? '■ Stop recording' : '● Start recording'}</button><button id="settings-button" class="icon" title="Settings">⚙</button></div></header><div class="subbar">${status.recording ? '<span class="live-dot"></span> Recording sent and received messages' : 'Ready to inspect recorded sessions'}${status.shortcut_error ? `<span class="shortcut-warning" title="${esc(status.shortcut_error)}"> · Shortcut unavailable</span>` : ''} <span class="shortcut" title="One key, any application, only while the proxy is running. Set it in Settings (space, f9, ctrl+alt+r ...). The key still types where you are, so a letter or the space bar also toggles every time you press it.">Global shortcut: ${esc(status.shortcut || 'off')}${status.shortcut ? (status.shortcut_active ? ' · listening' : ' · starts with the proxy') : ''}</span></div>`;
}
function sessionPanel() {
  return `<aside class="sessions"><div class="panel-title"><div><span class="eyebrow">LIBRARY</span><h2>Recorded sessions</h2></div><span class="counter">${state.sessions.length}</span></div><div id="session-list" class="session-list">${state.sessions.length ? state.sessions.map(s => `<button class="session ${state.selected===s.id?'selected':''}" data-id="${s.id}"><span class="session-icon">▣</span><span class="session-copy"><strong>${esc(date(s.started_at))}</strong><small>${Number(s.event_count).toLocaleString()} events · ${duration(s)}</small></span><span class="chevron">›</span></button>`).join('') : '<div class="empty">No sessions yet. Start the proxy, then record while the game is connected.</div>'}</div><div class="session-actions"><button id="folder-button">▱ &nbsp;Open folder</button><button id="export-button" ${state.selected?'':'disabled'}>↥ &nbsp;Export JSON</button><button id="export-raw-button" ${state.selected?'':'disabled'} title="The events the timeline shows now (your type filters and search), raw, one per line, saved to the exports folder">↥ &nbsp;Export visible (raw)</button><button id="delete-button" class="danger" ${state.selected?'':'disabled'}>Delete session</button></div></aside>`;
}
function eventsPanel() {
  const selected = state.sessions.find(s => s.id === state.selected);
  const count = selected?.event_count || 0;
  return `<section class="events"><div class="panel-title event-title"><div><span class="eyebrow">TIMELINE</span><h2>${selected ? esc(date(selected.started_at)) : 'Select a session'}</h2><p>${selected ? `${count.toLocaleString()} sent and received events · ${duration(selected)}` : 'Choose a recording from the left'}</p></div><label class="search"><span>⌕</span><input id="search" placeholder="Search events and payloads" value="${esc(state.query)}" ${selected?'':'disabled'}></label></div><div class="filters" id="filters">${selected ? `<button data-type="all" class="filter ${state.types.size===0?'active':''}">All <b>${count.toLocaleString()}</b></button>` + Object.entries(state.counts).map(([type,n]) => `<button data-type="${esc(type)}" class="filter ${state.types.has(type)?'active':''}">${esc(type)} <b>${n.toLocaleString()}</b></button>`).join('') : ''}</div><div class="table-head"><span>#</span><span>TIME</span><span>TYPE</span><span>DIRECTION</span><span>SUMMARY</span></div><div id="event-scroll" class="event-scroll"><div id="event-rows">${state.rows.length ? state.rows.map(row => `<button class="event-row ${state.event?.seq===row.seq?'selected':''}" data-seq="${row.seq}"><span>${row.seq}</span><span class="mono">${esc(time(row.received_at))}</span><span><b class="tag">${esc(row.type)}</b></span><span class="packet-direction ${row.direction === 'sent' ? 'sent' : 'received'}">${row.direction === 'sent' ? 'Sent →' : '← Received'}</span><span class="summary">${esc(row.summary)}</span></button>`).join('') : `<div class="empty">${selected ? 'No events match these filters.' : 'Your captured events will appear here.'}</div>`}</div><div class="load-more">${state.rows.length < state.total ? '<button id="more-button">Load more events</button>' : state.rows.length ? `Showing ${state.rows.length.toLocaleString()} of ${state.total.toLocaleString()}` : ''}</div></div></section>`;
}
function nodeView(key, value, depth=0) {
  if (depth > 25) return '<span class="muted">Depth limit</span>';
  if (value === null) return `<div class="field"><span>${esc(key)}</span><em>Null</em></div>`;
  if (Array.isArray(value)) return `<details class="tree" ${depth<1?'open':''}><summary><span>${esc(key)}</span><small>Array · ${value.length} items</small></summary><div class="children">${value.map((item,index) => nodeView(`[${index}]`, item, depth+1)).join('')}</div></details>`;
  if (typeof value === 'object') return `<details class="tree" ${depth<1?'open':''}><summary><span>${esc(key)}</span><small>Object · ${Object.keys(value).length} fields</small></summary><div class="children">${Object.entries(value).map(([name,item]) => nodeView(name,item,depth+1)).join('')}</div></details>`;
  return `<div class="field"><span>${esc(key)}</span><strong title="${esc(value)}">${typeof value === 'number' ? value.toLocaleString() : esc(value)}</strong></div>`;
}
function payloadView(packet) {
  const payload = packet.payload;
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return nodeView('Payload',payload);
  const command = String(packet.command || '').toLowerCase();
  const used = new Set();
  const parts = [];
  if (command === 'adi') {
    const gaa = payload.gaa;
    const ai = gaa?.AI;
    if (Array.isArray(ai) && ai.length >= 3) {
      parts.push('<h4>Target from gaa.AI</h4>' + nodeView('X coordinate',ai[1]) + nodeView('Y coordinate',ai[2]) + (ai.length > 4 ? nodeView('Observed GAA value',ai[4]) : ''));
    }
    if (Array.isArray(payload.AE)) {
      const row = payload.AE.find(row => Array.isArray(row) && row.length >= 3 && row[2] === 'TL');
      if (row) parts.push('<h4>Exact target level</h4>' + nodeView('Level',row[0]));
    }
    for (const key of ['KID','AE','gaa','gli']) if (key in payload) { parts.push(nodeView(key,payload[key])); used.add(key); }
  } else if (command === 'cra') {
    const movement = payload.AAM?.M;
    if (movement && typeof movement === 'object') {
      parts.push('<h4>Attack movement</h4>' + nodeView('March ID',movement.MID ?? null) + nodeView('Travel seconds',movement.TT ?? null) + nodeView('Kingdom ID',movement.KID ?? null));
    }
    if ('AAM' in payload) { parts.push(nodeView('AAM',payload.AAM)); used.add('AAM'); }
  }
  for (const [key,value] of Object.entries(payload)) if (!used.has(key)) parts.push(nodeView(key,value));
  return parts.join('');
}
function detailPanel() {
  const e = state.event;
  if (!e) return `<aside class="details"><div class="panel-title"><div><span class="eyebrow">INSPECTOR</span><h2>Event detail</h2></div></div><div class="empty detail-empty">Select an event to inspect its decoded structure.</div></aside>`;
  const packet = e.decoded || decodeUnparsed(e.raw);
  const shownType = packet?.command?.toUpperCase() || e.type;
  const shownSummary = e.type === 'UNKNOWN' && packet?.format === 'xml' ? `XML ${packet.payload.tag} · ${packet.command}` : e.summary;
  const payload = packet?.payload;
  const sections = packet ? `<div class="detail-section"><h3>Frame</h3>${nodeView('Sequence',e.seq)}${nodeView('Direction',e.direction === 'sent' ? 'Sent to server' : 'Received from server')}${nodeView('Captured',date(e.received_at))}${nodeView('Format',packet.format === 'xml' ? 'XML' : 'XT')}${nodeView('Command',packet.command?.toUpperCase())}${nodeView('Request ID',packet.request_id)}${nodeView('Status',packet.status)}${nodeView('Server header',packet.server_header)}</div><div class="detail-section"><h3>${esc(packet.command?.toUpperCase() || 'Message')} payload</h3>${payloadView(packet)}</div>` : `<div class="detail-section"><h3>Unparsed message</h3><p>The frame format is not recognized yet. Use Raw to inspect it.</p></div>`;
  return `<aside class="details"><div class="panel-title detail-title"><div><span class="eyebrow">INSPECTOR</span><h2>Event #${e.seq}</h2></div><div class="navigation"><button id="prev-event">‹</button><button id="next-event">›</button></div></div><div class="detail-intro"><b class="tag large">${esc(shownType)}</b><b class="packet-direction ${e.direction === 'sent' ? 'sent' : 'received'}">${e.direction === 'sent' ? 'Sent →' : '← Received'}</b><span>${esc(shownSummary)}</span></div><div class="detail-tabs"><button data-view="formatted" class="${state.raw?'':'active'}">Formatted</button><button data-view="raw" class="${state.raw?'active':''}">Raw</button></div><div class="detail-scroll">${state.raw ? `<pre>${esc(e.raw)}</pre>` : sections}</div></aside>`;
}
function settingsDialog() {
  const c=state.settings;
  if (!c) return '';
  return `<div class="modal-backdrop"><form id="settings-form" class="modal"><div class="modal-heading"><span class="eyebrow">CONFIGURATION</span><h2>Monitor settings</h2><p>Changes apply after restarting the monitor.</p></div><label>Recording folder<input name="data_dir" value="${esc(c.data_dir)}"></label><div class="modal-grid"><label>Proxy host<input name="proxy_host" value="${esc(c.proxy_host)}"></label><label>Proxy port<input name="proxy_port" type="number" min="1" max="65535" value="${esc(c.proxy_port)}"></label></div><label>Global shortcut<input name="shortcut" value="${esc(c.shortcut)}"></label><label>mitmdump executable<input name="mitmdump" value="${esc(c.mitmdump)}"></label><div class="modal-actions"><button type="button" id="settings-cancel">Cancel</button><button type="submit" class="primary">Save settings</button></div></form></div>`;
}
function injectBar() {
  const live = !!state.status?.connected;
  return `<form id="inject-form" class="inject-bar" ${state.tab==='sessions'?'':'hidden'}><label for="inject-text">Inject</label><textarea id="inject-text" rows="2" spellcheck="false" placeholder="%xt%EmpireEx_19%nch%1%  (one frame per line; Ctrl/Cmd+Enter sends)">${esc(state.injectText)}</textarea><select id="inject-direction" title="Direction"><option value="server" ${state.injectToClient?'':'selected'}>To server</option><option value="client" ${state.injectToClient?'selected':''}>To client</option></select><button class="primary" type="submit">Send</button><small class="${live?'ok':''}">${live?'Game WebSocket connected':'No game WebSocket'}</small></form>`;
}
function connectionHint(status) {
  if (!status?.proxy_running) return 'Start the proxy to listen for game traffic.';
  if (status.connected) return 'Game WebSocket connected.';
  if (status.last_websocket_url) return status.last_websocket_matches_game
    ? 'The game WebSocket was seen but has closed. Reconnect the game client.'
    : `Last WebSocket: ${esc(status.last_websocket_url)}. It does not match the game endpoint.`;
  return 'Proxy is listening, but no WebSocket has reached it. Check the game client proxy address and mitmproxy certificate.';
}
function incomingPanel() {
  const data = state.incoming || {location:null, castles:[], active:[], arrived:[], active_count:0};
  const loc = data.location;
  const castles = data.castles || [];
  const label = castles.length ? castles.map(c => `KID ${c.kid}: ${c.x}, ${c.y}`).join(' · ') : loc ? `Kingdom ${loc.own_kid} · ${loc.own_x}, ${loc.own_y}` : 'No location configured';
  const row = entry => {
    const composition = entry.composition;
    const groups = composition.unit_groups;
    const groupRows = groups ? nodeView('Left units',groups.left) + nodeView('Centre units',groups.middle) + nodeView('Right units',groups.right) + nodeView('RW units',groups.courtyard) : '';
    return `<details class="incoming-entry"><summary><span class="mono">${entry.mid}</span><strong>${esc(entry.sender.name || 'Unknown player')}</strong><span>${entry.source.x ?? '—'}, ${entry.source.y ?? '—'}</span><span>${esc(new Date(entry.arrival_at*1000).toLocaleTimeString())}</span><span class="countdown" data-arrival="${entry.arrival_at}" ${entry.retreated_at?'data-retreated="1"':''}></span><span>${composition.troops ?? 'Unknown'}</span><span>${composition.courtyard ?? 'Unknown'}</span></summary><div class="incoming-detail"><section><h3>Movement</h3>${nodeView('MID',entry.mid)}${nodeView('KID',entry.kid)}${nodeView('PT at first observation',entry.pt)}${nodeView('TT',entry.tt)}${nodeView('Estimated arrival',date(entry.arrival_at))}${nodeView('First observed',date(entry.first_observed_at))}${entry.retreated_at?nodeView('Retreated',date(entry.retreated_at)):''}</section><section><h3>Sender</h3>${personDetail(entry.sender,entry.source,entry.kid)}</section><section><h3>Receiver</h3>${personDetail(entry.receiver,entry.target,entry.kid)}</section><section><h3>Composition</h3>${nodeView('Total troops',composition.troops ?? 'Unknown')}${nodeView('Left flank',composition.left ?? 'Unknown')}${nodeView('Centre',composition.middle ?? 'Unknown')}${nodeView('Right flank',composition.right ?? 'Unknown')}${nodeView('RW / reserve-courtyard',composition.courtyard ?? 'Unknown')}${groupRows}${nodeView('Tools',composition.tools ?? 'Unknown')}${nodeView('Waves',composition.waves ?? 'Unknown')}</section></div></details>`;
  };
  return `<section class="incoming-view" ${state.tab==='incoming'?'':'hidden'}><div class="incoming-header"><div><span class="eyebrow">EXPERIMENTAL · GAM / MRM</span><h1>Incoming</h1><p>Watching ${esc(label)} · <b id="active-count">${data.active_count}</b> active incoming candidates · ${state.status?.connected ? "Game connected" : state.status?.proxy_running ? "Proxy waiting for game" : "Proxy stopped"}</p></div><form id="location-form" class="location-form" ${castles.length?'hidden':''}><label>Kingdom<input name="own_kid" type="number" min="0" required value="${loc?.own_kid ?? ''}"></label><label>X<input name="own_x" type="number" min="0" required value="${loc?.own_x ?? ''}"></label><label>Y<input name="own_y" type="number" min="0" required value="${loc?.own_y ?? ''}"></label><button class="primary" type="submit">Set location</button></form></div><div class="incoming-note"><strong>Connection:</strong> Set the game client HTTP proxy to <code>127.0.0.1:${state.status?.proxy_port || 8080}</code>. The dashboard runs separately on <code>127.0.0.1:8799</code>. ${connectionHint(state.status)}<br>${castles.length?'Watching the owned castles configured in Auto Support.':'Set a fallback location here, or add owned castles in Auto Support.'} Arrival is estimated from TT − PT. GA unit counts include L, M, R, and RW; the exact RW battle role needs further packet confirmation.</div><div class="incoming-list"><h2>Active incoming <span>${data.active.length}</span></h2><div class="incoming-table-head"><span>MID</span><span>SENDER</span><span>SOURCE</span><span>ARRIVAL</span><span>REMAINING</span><span>TROOPS</span><span>RW</span></div>${data.active.length ? data.active.map(row).join('') : '<div class="empty">No matching incoming movements yet.</div>'}<h2>Predicted arrival passed <span>${data.arrived.length}</span></h2>${data.arrived.length ? data.arrived.map(row).join('') : '<div class="empty">None yet.</div>'}<h2>Retreated <span>${data.retreated?.length || 0}</span></h2>${data.retreated?.length ? data.retreated.map(row).join('') : '<div class="empty">None yet.</div>'}</div></section>`;
}
function supportPanel() {
  const cfg = state.autoSupport?.config || {enabled:false, castles:[], destinations:{}, lid:0};
  const status = state.autoSupport?.status || {jobs:[], last_event:'Waiting for proxy'};
  const castleRows = cfg.castles.map((castle, index) => `<tr><td>${castle.kid}</td><td>${castle.x}, ${castle.y}</td><td><button type="button" data-remove-castle="${index}">Remove</button></td></tr>`).join('');
  const destinationRows = Object.entries(cfg.destinations).map(([kid, point]) => `<tr><td>${esc(kid)}</td><td>${point.x}, ${point.y}</td><td><button type="button" data-remove-destination="${esc(kid)}">Remove</button></td></tr>`).join('');
  const jobs = [...status.jobs].reverse().map(job => {
    const details = [job.sent_mid != null ? `Support MID ${job.sent_mid}` : null,
      job.recall_reason ? `Recall: ${job.recall_reason}` : null,
      job.return_at ? `Estimated return ${date(job.return_at)}` : null,
      job.error || (job.units ? job.units.map(row => row.join(' × ')).join(', ') : null)]
      .filter(Boolean).map(esc).join('<br>') || '—';
    return `<tr><td>${job.mid}</td><td>${job.kid}: ${job.sx}, ${job.sy}</td><td>${esc(date(job.landing))}</td><td>${esc(job.state)}</td><td>${details}</td></tr>`;
  }).join('');
  return `<section class="incoming-view support-view" ${state.tab==='support'?'':'hidden'}><div class="incoming-header"><div><span class="eyebrow">PROXY PROOF OF CONCEPT</span><h1>Auto Support</h1><p>${cfg.enabled?'On':'Off'} · ${esc(status.last_event)}</p></div><button id="support-toggle" class="${cfg.enabled?'secondary':'primary'}">${cfg.enabled?'Turn off':'Turn on'}</button></div>
    <div class="incoming-note">Auto Support triggers when a GAM targets a configured owned castle and its sender and receiver have different alliance IDs in the matched O[] player records. Missing alliance records or equal IDs are skipped. It requests DCL about 7 seconds before estimated landing, SDI at 5 seconds, then CDS. After dispatch, it recalls support at estimated landing or on an earlier confirmed attack retreat. Outstanding recalls continue when this toggle is off. LID availability is not shared with the Rust bot.</div>
    <div class="support-grid"><div class="support-card"><h2>Owned source castles</h2><table><thead><tr><th>KID</th><th>Coordinates</th><th></th></tr></thead><tbody>${castleRows || '<tr><td colspan="3">None configured</td></tr>'}</tbody></table><form id="source-form" class="location-form"><label>KID<input name="kid" type="number" min="0" required></label><label>X<input name="x" type="number" min="0" required></label><label>Y<input name="y" type="number" min="0" required></label><button type="submit">Add castle</button></form></div>
    <div class="support-card"><h2>Send support to</h2><table><thead><tr><th>KID</th><th>Destination X, Y</th><th></th></tr></thead><tbody>${destinationRows || '<tr><td colspan="3">None configured</td></tr>'}</tbody></table><form id="destination-form" class="location-form"><label>KID<input name="kid" type="number" min="0" required></label><label>X<input name="x" type="number" min="0" required></label><label>Y<input name="y" type="number" min="0" required></label><button type="submit">Set destination</button></form></div></div>
    <div class="support-card support-wide"><h2>Support LID</h2><form id="support-lid-form" class="location-form"><label>Server LID<input name="lid" type="number" min="0" required value="${cfg.lid}"></label><button type="submit">Save LID</button></form><p>The proxy does not yet reserve a commander against the Rust bot; choose an idle LID.</p></div>
    <div class="support-card support-wide"><h2>Recent attempts</h2><table><thead><tr><th>MID</th><th>Source</th><th>Estimated landing</th><th>State</th><th>Result or selected units</th></tr></thead><tbody>${jobs || '<tr><td colspan="5">No attempts yet</td></tr>'}</tbody></table></div></section>`;
}
function personDetail(person,location,kid) {
  return nodeView('Name',person.name ?? 'Unknown') + nodeView('Player ID',person.id ?? 'Unknown') + nodeView('Level',person.level ?? 'Unknown') + nodeView('Legendary level',person.legendary_level ?? 'Unknown') + nodeView('Might points',person.might_points ?? 'Unknown') + nodeView('Kingdom',kid) + nodeView('Coordinates',location.x == null || location.y == null ? 'Unknown' : `${location.x}, ${location.y}`);
}
function updateCountdowns() {
  const now=Date.now()/1000;
  document.querySelectorAll('.countdown').forEach(element => { if (element.dataset.retreated) { element.textContent='Retreated'; return; } const seconds=Math.max(0,Math.ceil(Number(element.dataset.arrival)-now)); element.textContent=seconds ? `${String(Math.floor(seconds/3600)).padStart(2,'0')}:${String(Math.floor(seconds%3600/60)).padStart(2,'0')}:${String(seconds%60).padStart(2,'0')}` : 'ETA passed'; });
  const count=document.querySelector('#active-count'); if(count && state.incoming) count.textContent=state.incoming.active.filter(item=>item.arrival_at>now).length;
}
async function refreshIncoming() {
  try { const next=await api('/api/incoming'); if(JSON.stringify(next)!==JSON.stringify(state.incoming)) { state.incoming=next; if(state.tab==='incoming') render(); } else updateCountdowns(); } catch(e) { notice(e.message); }
}
function render() {
  const focused = document.activeElement?.id;
  const selection = focused === 'search' || focused === 'inject-text' ? [document.activeElement.selectionStart, document.activeElement.selectionEnd] : null;
  app.innerHTML = `${header()}<nav class="tabs"><button data-tab="sessions" class="${state.tab==='sessions'?'active':''}">Recorded sessions</button><button data-tab="incoming" class="${state.tab==='incoming'?'active':''}">Incoming <span>${state.incoming?.active_count ?? 0}</span></button><button data-tab="support" class="${state.tab==='support'?'active':''}">Auto Support</button></nav>${injectBar()}<main ${state.tab==='sessions'?'':'hidden'}>${sessionPanel()}${eventsPanel()}${detailPanel()}</main>${incomingPanel()}${supportPanel()}${settingsDialog()}<div id="notice" hidden></div>`;
  bind();
  updateCountdowns();
  if (selection) { const input = document.querySelector('#' + focused); input.focus(); input.setSelectionRange(...selection); }
}
async function refreshStatus() { try { const previous=state.status; state.status=await api('/api/status'); if (JSON.stringify(previous)!==JSON.stringify(state.status)) render(); } catch {} }
async function refreshSupport() {
  try {
    const next=await api('/api/auto-support');
    if (JSON.stringify(next)!==JSON.stringify(state.autoSupport)) {
      state.autoSupport=next;
      if (state.tab==='support' && !document.activeElement?.closest('#source-form, #destination-form, #support-lid-form')) render();
    }
  } catch(e) { notice(e.message); }
}
async function saveSupport(config) {
  await api('/api/auto-support/config','POST',config);
  await refreshSupport();
  render();
}
async function refreshSessions() { try { const previous = JSON.stringify(state.sessions); state.sessions=await api('/api/sessions'); if (!state.selected && state.sessions.length) { state.selected=state.sessions[0].id; await loadEvents(); } else if (previous!==JSON.stringify(state.sessions)) render(); } catch(e) { notice(e.message); } }
async function loadEvents(append=false) {
  if (!state.selected) return;
  const offset=append?state.rows.length:0;
  const query=new URLSearchParams({limit:'100',offset:String(offset),q:state.query});
  for (const type of state.types) query.append('type',type);
  try { const data=await api(`/api/sessions/${state.selected}/events?${query}`); state.rows=append?[...state.rows,...data.events]:data.events; state.counts=data.types; state.total=data.total; render(); } catch(e) { notice(e.message); }
}
async function selectEvent(seq) {
  try {
    state.event = await api(`/api/sessions/${state.selected}/events/${seq}`);
    const parsed = state.event.decoded || decodeUnparsed(state.event.raw);
    if (state.event.type === 'UNKNOWN' && parsed?.command) {
      const row = state.rows.find(item => item.seq === seq);
      if (row) {
        row.type = parsed.command.toUpperCase();
        row.summary = parsed.format === 'xml' ? `XML ${parsed.payload.tag} · ${parsed.command}` : 'No payload';
      }
    }
    state.raw = false;
    render();
  } catch(e) { notice(e.message); }
}
function bind() {
  document.querySelectorAll('[data-tab]').forEach(button => button.onclick=() => { state.tab=button.dataset.tab; render(); });
  const injectText=document.querySelector('#inject-text');
  injectText.oninput=() => { state.injectText=injectText.value; };
  document.querySelector('#inject-direction').onchange=event => { state.injectToClient=event.target.value==='client'; };
  injectText.onkeydown=event => { if (event.key==='Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); document.querySelector('#inject-form').requestSubmit(); } };
  document.querySelector('#inject-form').onsubmit=async event => {
    event.preventDefault();
    try { const result=await api('/api/inject','POST',{text:state.injectText,to_client:state.injectToClient}); notice(`Sent ${result.sent} frame${result.sent===1?'':'s'} ${result.to_client?'to the client':'to the server'}.`); }
    catch(e) { notice(e.message); }
  };
  const locationForm=document.querySelector('#location-form');
  locationForm.onsubmit=async event => { event.preventDefault(); const value=Object.fromEntries(new FormData(locationForm)); try { await api('/api/incoming/config','POST',value); await refreshIncoming(); notice('Incoming location updated. Previous location ledger cleared.'); } catch(e) { notice(e.message); } };
  const supportConfig = () => JSON.parse(JSON.stringify(state.autoSupport?.config || {enabled:false,castles:[],destinations:{},lid:0}));
  document.querySelector('#support-toggle').onclick=async () => { const config=supportConfig(); config.enabled=!config.enabled; try { await saveSupport(config); notice(`Auto Support ${config.enabled?'on':'off'}`); } catch(e) { notice(e.message); } };
  document.querySelector('#source-form').onsubmit=async event => { event.preventDefault(); const config=supportConfig(); const row=Object.fromEntries(new FormData(event.target)); config.castles.push(Object.fromEntries(Object.entries(row).map(([key,value])=>[key,Number(value)]))); try { await saveSupport(config); } catch(e) { notice(e.message); } };
  document.querySelector('#destination-form').onsubmit=async event => { event.preventDefault(); const config=supportConfig(); const row=Object.fromEntries(new FormData(event.target)); config.destinations[String(Number(row.kid))]={x:Number(row.x),y:Number(row.y)}; try { await saveSupport(config); } catch(e) { notice(e.message); } };
  document.querySelector('#support-lid-form').onsubmit=async event => { event.preventDefault(); const config=supportConfig(); config.lid=Number(new FormData(event.target).get('lid')); try { await saveSupport(config); } catch(e) { notice(e.message); } };
  document.querySelectorAll('[data-remove-castle]').forEach(button => button.onclick=async () => { const config=supportConfig(); config.castles.splice(Number(button.dataset.removeCastle),1); try { await saveSupport(config); } catch(e) { notice(e.message); } });
  document.querySelectorAll('[data-remove-destination]').forEach(button => button.onclick=async () => { const config=supportConfig(); delete config.destinations[button.dataset.removeDestination]; try { await saveSupport(config); } catch(e) { notice(e.message); } });

  document.querySelector('#capture-toggle').onchange=async event => { try { await api('/api/capture/config','POST',{enabled:event.target.checked}); await refreshStatus(); } catch(e) { event.target.checked=!event.target.checked; notice(e.message); } };
  document.querySelector('#images-toggle').onchange=async event => { try { await api('/api/capture/config','POST',{images:event.target.checked}); await refreshStatus(); } catch(e) { event.target.checked=!event.target.checked; notice(e.message); } };
  document.querySelector('#proxy-button').onclick=async () => { try { await api(`/api/proxy/${state.status?.proxy_running?'stop':'start'}`,'POST'); await refreshStatus(); } catch(e) { notice(e.message); } };
  document.querySelector('#record-button').onclick=async () => { try { await api('/api/recording/toggle','POST'); setTimeout(refreshStatus,250); } catch(e) { notice(e.message); } };
  document.querySelector('#settings-button').onclick=async () => { try { state.settings=await api('/api/config'); render(); } catch(e) { notice(e.message); } };
  const settingsForm=document.querySelector('#settings-form');
  if (settingsForm) {
    document.querySelector('#settings-cancel').onclick=() => { state.settings=null; render(); };
    settingsForm.onsubmit=async event => { event.preventDefault(); const values=Object.fromEntries(new FormData(settingsForm)); values.proxy_port=Number(values.proxy_port); try { await api('/api/config','POST',values); state.settings=null; render(); notice('Settings saved. Restart the monitor to apply.'); } catch(e) { notice(e.message); } };
  }
  document.querySelector('#folder-button').onclick=async () => { try { await api('/api/folder','POST'); } catch(e) { notice(e.message); } };
  document.querySelector('#export-raw-button').onclick=async () => {
    try {
      const result=await api(`/api/sessions/${state.selected}/export-raw`,'POST',{types:[...state.types],q:state.query});
      try { await navigator.clipboard.writeText(result.latest); } catch {}
      notice(`${result.count.toLocaleString()} events written to ${result.path} (also ${result.latest}, path copied)`);
    } catch(e) { notice(e.message); }
  };
  document.querySelector('#export-button').onclick=async () => { try { const result=await api(`/api/sessions/${state.selected}/export`,'POST',{}); notice(`Exported to ${result.path}`); } catch(e) { notice(e.message); } };
  document.querySelector('#delete-button').onclick=async () => { if (!confirm('Delete this recorded session permanently?')) return; try { await api(`/api/sessions/${state.selected}`,'DELETE'); state.selected=null; state.event=null; state.rows=[]; state.types.clear(); await refreshSessions(); } catch(e) { notice(e.message); } };
  document.querySelectorAll('.session[data-id]').forEach(button => button.onclick=async () => { state.selected=button.dataset.id; state.event=null; state.types.clear(); state.query=''; await loadEvents(); });
  document.querySelectorAll('.filter').forEach(button => button.onclick=async () => { const type=button.dataset.type; if (type==='all') state.types.clear(); else state.types.has(type)?state.types.delete(type):state.types.add(type); await loadEvents(); });
  document.querySelectorAll('.event-row').forEach(button => button.onclick=() => selectEvent(Number(button.dataset.seq)));
  const input=document.querySelector('#search'); if (input) input.oninput=() => { state.query=input.value; clearTimeout(window.searchTimer); window.searchTimer=setTimeout(() => loadEvents(),250); };
  const more=document.querySelector('#more-button'); if (more) more.onclick=() => loadEvents(true);
  document.querySelectorAll('[data-view]').forEach(button => button.onclick=() => { state.raw=button.dataset.view==='raw'; render(); });
  for (const [id,step] of [['prev-event',-1],['next-event',1]]) { const button=document.querySelector('#'+id); if(button) button.onclick=() => { const index=state.rows.findIndex(row=>row.seq===state.event.seq); if(state.rows[index+step]) selectEvent(state.rows[index+step].seq); }; }
}
render();
refreshStatus(); refreshSessions(); refreshIncoming(); refreshSupport();
setInterval(async () => { await refreshStatus(); await refreshSessions(); }, 2500);

setInterval(refreshIncoming, 1500);
setInterval(refreshSupport, 1500);
setInterval(updateCountdowns, 1000);
