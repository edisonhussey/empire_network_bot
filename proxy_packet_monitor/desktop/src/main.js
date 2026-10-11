import './style.css';
const API = 'http://127.0.0.1:8798';
const app = document.querySelector('#app');
const state = { sessions: [], selected: null, rows: [], counts: {}, total: 0, offset: 0, types: new Set(), query: '', event: null, raw: false, status: null, busy: false, settings: null };
const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const date = value => value ? new Date(value * 1000).toLocaleString() : '—';
const time = value => value ? new Date(value * 1000).toLocaleTimeString(undefined,{hour12:false, fractionalSecondDigits:3}) : '—';
const duration = item => item.ended_at ? `${Math.round(item.ended_at - item.started_at)}s` : 'Recoverable';
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
  return `<header><div class="brand"><span class="brand-mark">◇</span><div><strong>Proxy Packet Monitor</strong><small>Recorded session explorer</small></div></div><div class="toolbar"><span class="status ${status.connected?'connected':''}"><i></i>${status.connected ? 'Game connected' : status.proxy_running ? 'Proxy waiting for game' : 'Proxy stopped'}</span><span class="endpoint">${esc(status.endpoint || `127.0.0.1:${status.proxy_port || 8080}`)}</span><button id="proxy-button" class="secondary">${status.proxy_running ? 'Stop proxy' : 'Start proxy'}</button><button id="record-button" class="primary ${status.recording?'recording':''}" ${status.proxy_running?'':'disabled'}>${status.recording ? '■ Stop recording' : '● Start recording'}</button><button id="settings-button" class="icon" title="Settings">⚙</button></div></header><div class="subbar">${status.recording ? '<span class="live-dot"></span> Recording incoming messages' : 'Ready to inspect recorded sessions'}${status.shortcut_error ? `<span class="shortcut-warning" title="${esc(status.shortcut_error)}"> · Shortcut unavailable</span>` : ''} <span class="shortcut">Global shortcut: ${esc(status.shortcut || 'Ctrl+Alt+X')}</span></div>`;
}
function sessionPanel() {
  return `<aside class="sessions"><div class="panel-title"><div><span class="eyebrow">LIBRARY</span><h2>Recorded sessions</h2></div><span class="counter">${state.sessions.length}</span></div><div id="session-list" class="session-list">${state.sessions.length ? state.sessions.map(s => `<button class="session ${state.selected===s.id?'selected':''}" data-id="${s.id}"><span class="session-icon">▣</span><span class="session-copy"><strong>${esc(date(s.started_at))}</strong><small>${Number(s.event_count).toLocaleString()} events · ${duration(s)}</small></span><span class="chevron">›</span></button>`).join('') : '<div class="empty">No sessions yet. Start the proxy, then record while the game is connected.</div>'}</div><div class="session-actions"><button id="folder-button">▱ &nbsp;Open folder</button><button id="export-button" ${state.selected?'':'disabled'}>↥ &nbsp;Export JSON</button><button id="delete-button" class="danger" ${state.selected?'':'disabled'}>Delete session</button></div></aside>`;
}
function eventsPanel() {
  const selected = state.sessions.find(s => s.id === state.selected);
  const count = selected?.event_count || 0;
  return `<section class="events"><div class="panel-title event-title"><div><span class="eyebrow">TIMELINE</span><h2>${selected ? esc(date(selected.started_at)) : 'Select a session'}</h2><p>${selected ? `${count.toLocaleString()} incoming events · ${duration(selected)}` : 'Choose a recording from the left'}</p></div><label class="search"><span>⌕</span><input id="search" placeholder="Search events and payloads" value="${esc(state.query)}" ${selected?'':'disabled'}></label></div><div class="filters" id="filters">${selected ? `<button data-type="all" class="filter ${state.types.size===0?'active':''}">All <b>${count.toLocaleString()}</b></button>` + Object.entries(state.counts).map(([type,n]) => `<button data-type="${esc(type)}" class="filter ${state.types.has(type)?'active':''}">${esc(type)} <b>${n.toLocaleString()}</b></button>`).join('') : ''}</div><div class="table-head"><span>#</span><span>TIME</span><span>TYPE</span><span>SUMMARY</span></div><div id="event-scroll" class="event-scroll"><div id="event-rows">${state.rows.length ? state.rows.map(row => `<button class="event-row ${state.event?.seq===row.seq?'selected':''}" data-seq="${row.seq}"><span>${row.seq}</span><span class="mono">${esc(time(row.received_at))}</span><span><b class="tag">${esc(row.type)}</b></span><span class="summary">${esc(row.summary)}</span></button>`).join('') : `<div class="empty">${selected ? 'No events match these filters.' : 'Your captured events will appear here.'}</div>`}</div><div class="load-more">${state.rows.length < state.total ? '<button id="more-button">Load more events</button>' : state.rows.length ? `Showing ${state.rows.length.toLocaleString()} of ${state.total.toLocaleString()}` : ''}</div></div></section>`;
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
  const packet = e.decoded;
  const payload = packet?.payload;
  const sections = packet ? `<div class="detail-section"><h3>Frame</h3>${nodeView('Sequence',e.seq)}${nodeView('Received',date(e.received_at))}${nodeView('Command',packet.command?.toUpperCase())}${nodeView('Request ID',packet.request_id)}${nodeView('Status',packet.status)}${nodeView('Server header',packet.server_header)}</div><div class="detail-section"><h3>${esc(packet.command?.toUpperCase() || 'Message')} payload</h3>${payloadView(packet)}</div>` : `<div class="detail-section"><h3>Unparsed message</h3><p>The server frame did not match the XT format.</p></div>`;
  return `<aside class="details"><div class="panel-title detail-title"><div><span class="eyebrow">INSPECTOR</span><h2>Event #${e.seq}</h2></div><div class="navigation"><button id="prev-event">‹</button><button id="next-event">›</button></div></div><div class="detail-intro"><b class="tag large">${esc(e.type)}</b><span>${esc(e.summary)}</span></div><div class="detail-tabs"><button data-view="formatted" class="${state.raw?'':'active'}">Formatted</button><button data-view="raw" class="${state.raw?'active':''}">Raw</button></div><div class="detail-scroll">${state.raw ? `<pre>${esc(e.raw)}</pre>` : sections}</div></aside>`;
}
function settingsDialog() {
  const c=state.settings;
  if (!c) return '';
  return `<div class="modal-backdrop"><form id="settings-form" class="modal"><div class="modal-heading"><span class="eyebrow">CONFIGURATION</span><h2>Monitor settings</h2><p>Changes apply after restarting the monitor.</p></div><label>Recording folder<input name="data_dir" value="${esc(c.data_dir)}"></label><div class="modal-grid"><label>Proxy host<input name="proxy_host" value="${esc(c.proxy_host)}"></label><label>Proxy port<input name="proxy_port" type="number" min="1" max="65535" value="${esc(c.proxy_port)}"></label></div><label>Global shortcut<input name="shortcut" value="${esc(c.shortcut)}"></label><label>mitmdump executable<input name="mitmdump" value="${esc(c.mitmdump)}"></label><div class="modal-actions"><button type="button" id="settings-cancel">Cancel</button><button type="submit" class="primary">Save settings</button></div></form></div>`;
}
function render() {
  const focused = document.activeElement?.id;
  const selection = focused === 'search' ? [document.activeElement.selectionStart, document.activeElement.selectionEnd] : null;
  app.innerHTML = `${header()}<main>${sessionPanel()}${eventsPanel()}${detailPanel()}</main>${settingsDialog()}<div id="notice" hidden></div>`;
  bind();
  if (selection) { const input = document.querySelector('#search'); input.focus(); input.setSelectionRange(...selection); }
}
async function refreshStatus() { try { const previous=state.status; state.status=await api('/api/status'); if (JSON.stringify(previous)!==JSON.stringify(state.status)) render(); } catch {} }
async function refreshSessions() { try { const previous = JSON.stringify(state.sessions); state.sessions=await api('/api/sessions'); if (!state.selected && state.sessions.length) { state.selected=state.sessions[0].id; await loadEvents(); } else if (previous!==JSON.stringify(state.sessions)) render(); } catch(e) { notice(e.message); } }
async function loadEvents(append=false) {
  if (!state.selected) return;
  const offset=append?state.rows.length:0;
  const query=new URLSearchParams({limit:'100',offset:String(offset),q:state.query});
  for (const type of state.types) query.append('type',type);
  try { const data=await api(`/api/sessions/${state.selected}/events?${query}`); state.rows=append?[...state.rows,...data.events]:data.events; state.counts=data.types; state.total=data.total; render(); } catch(e) { notice(e.message); }
}
async function selectEvent(seq) { try { state.event=await api(`/api/sessions/${state.selected}/events/${seq}`); state.raw=false; render(); } catch(e) { notice(e.message); } }
function bind() {
  document.querySelector('#proxy-button').onclick=async () => { try { await api(`/api/proxy/${state.status?.proxy_running?'stop':'start'}`,'POST'); await refreshStatus(); } catch(e) { notice(e.message); } };
  document.querySelector('#record-button').onclick=async () => { try { await api('/api/recording/toggle','POST'); setTimeout(refreshStatus,250); } catch(e) { notice(e.message); } };
  document.querySelector('#settings-button').onclick=async () => { try { state.settings=await api('/api/config'); render(); } catch(e) { notice(e.message); } };
  const settingsForm=document.querySelector('#settings-form');
  if (settingsForm) {
    document.querySelector('#settings-cancel').onclick=() => { state.settings=null; render(); };
    settingsForm.onsubmit=async event => { event.preventDefault(); const values=Object.fromEntries(new FormData(settingsForm)); values.proxy_port=Number(values.proxy_port); try { await api('/api/config','POST',values); state.settings=null; render(); notice('Settings saved. Restart the monitor to apply.'); } catch(e) { notice(e.message); } };
  }
  document.querySelector('#folder-button').onclick=async () => { try { await api('/api/folder','POST'); } catch(e) { notice(e.message); } };
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
refreshStatus(); refreshSessions();
setInterval(async () => { await refreshStatus(); await refreshSessions(); }, 2500);
