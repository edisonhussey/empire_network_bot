// The map's UI, built inside the container it is given (so it can live in any page or app tab): a clean top bar with the kingdom picker,
// view and overlay toggles, plus a collapsible Dev panel to fire test attacks and edit team colours. It talks to the world and to callbacks only.
import { TEAMS } from '../world/teams.js';

const fmt = (s) => (s >= 60 ? `${Math.floor(s / 60)}m ${Math.round(s % 60)}s` : `${Math.round(s)}s`);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

export function createUI({ container, world, teams, focus, stage, gridView, actions }) {
  container.classList.add('mw');
  container.insertAdjacentHTML('beforeend', `
    <div class="mw-bar">
      <span class="mw-title">MAP</span>
      <label class="mw-field">Kingdom <select data-k></select></label>
      <span class="mw-chip" data-chip></span>
      <span class="mw-gap"></span>
      <label class="mw-field">Map <select data-map><option value="sheet">Sheet example</option><option value="gen">Generated</option></select></label>
      <input data-seed type="number" min="1" step="1" value="1" title="seed" style="width:56px" hidden><button data-roll hidden>New</button>
      <span class="mw-gap"></span>
      <span class="mw-seg"><button data-view="Iso">Iso</button><button data-view="Top">Top</button><button data-view="Side">Side</button></span>
      <button data-t="grid" class="on">Grid</button><button data-t="blocked">Blocked</button><button data-t="light">Light</button><button data-t="dev">Dev</button>
    </div>
    <div class="mw-hint" data-hint></div>
    <div class="mw-light" data-light hidden>
      <div class="mw-sec">Lighting</div>
      <div class="mw-row" data-presets></div>
      <div class="mw-sliders" data-sliders></div>
      <label class="mw-check"><input type="checkbox" data-shadows> Shadows</label>
    </div>
    <div class="mw-dev" data-dev hidden>
      <div class="mw-sec">Send attack</div>
      <div class="mw-grid2"><span>From</span><select data-from></select><span>To</span><select data-to></select><span>Seconds</span><input data-secs type="number" min="1" step="1" value="30"></div>
      <div class="mw-row"><button data-send>Send</button><button data-demo title="six attacks between random castles">Random attacks</button><span data-info class="mw-dim"></span></div>
      <div class="mw-sec">Team of selected kingdom</div>
      <div class="mw-row" data-teams></div>
      <div class="mw-grid2"><span>Primary</span><input data-ca type="color"><span>Secondary</span><input data-cb type="color"></div>
      <div class="mw-sec">Marches</div>
      <div data-marches></div>
    </div>
    <div class="mw-stats" data-stats></div>`);
  const $ = (sel) => container.querySelector(sel);
  const $$ = (sel) => [...container.querySelectorAll(sel)];
  let source = null;
  let listTimer = 0;

  // ---- kingdom picker (also driven from outside through focus) ------------------------------------------------------------------------------------
  const fillKingdoms = () => {
    $('[data-k]').innerHTML = '<option value="">— none —</option>' + [...world.kingdoms.values()].map((k) => `<option value="${esc(k.id)}">${esc(k.name)} (${k.castles.length})</option>`).join('');
    $('[data-k]').value = focus.kingdomId ?? '';
  };
  const syncChip = () => {
    const k = world.kingdoms.get(focus.kingdomId), t = k && teams[k.team];
    $('[data-chip]').innerHTML = t ? `<i style="background:${t.primary}"></i><i style="background:${t.secondary}"></i>` : '';
    $('[data-ca]').disabled = $('[data-cb]').disabled = !t;   // nothing to edit until a kingdom is selected
    if (t) { $('[data-ca]').value = t.primary; $('[data-cb]').value = t.secondary; }
    for (const b of $$('[data-teams] button')) b.classList.toggle('on', !!k && b.dataset.team === k.team);
  };
  $('[data-k]').onchange = (e) => focus.selectKingdom(e.target.value || null);
  focus.on(() => { $('[data-k]').value = focus.kingdomId ?? ''; syncChip(); fillFrom(); });
  world.on('kingdom', () => { fillKingdoms(); fillFrom(); fillTo(); });

  // ---- map choice: the hand-authored sheet map, or a generated one from a seed ----------------------------------------------------------------------
  const applyMap = () => actions.setMap($('[data-map]').value, +$('[data-seed]').value || 1);
  $('[data-map]').onchange = () => { const gen = $('[data-map]').value === 'gen'; $('[data-seed]').hidden = $('[data-roll]').hidden = !gen; applyMap(); };
  $('[data-seed]').onchange = applyMap;
  $('[data-roll]').onclick = () => { $('[data-seed]').value = 1 + Math.floor(Math.random() * 9999); applyMap(); };

  // ---- dev: attack form ------------------------------------------------------------------------------------------------------------------------------
  const opt = (e) => `<option value="${e.x},${e.y}">${esc(e.name)} (${e.x}, ${e.y})</option>`;
  function fillFrom() {
    const mine = focus.kingdomId ? world.kingdoms.get(focus.kingdomId).castles : [...world.castles.values()];
    const prev = $('[data-from]').value;
    $('[data-from]').innerHTML = mine.slice(0, 300).map(opt).join('');
    if ([...$('[data-from]').options].some((o) => o.value === prev)) $('[data-from]').value = prev;
  }
  function fillTo() {
    const prev = $('[data-to]').value;
    $('[data-to]').innerHTML = world.allEntities().slice(0, 400).map(opt).join('');
    if ([...$('[data-to]').options].some((o) => o.value === prev)) $('[data-to]').value = prev;
    else if ($('[data-to]').options.length > 1) $('[data-to]').selectedIndex = $('[data-to]').options.length - 1;
  }
  const pt = (v) => { const [x, y] = v.split(',').map(Number); return { x, y }; };
  const seconds = () => Math.max(1, +$('[data-secs]').value || 30);
  $('[data-send]').onclick = () => { if ($('[data-from]').value && $('[data-to]').value && $('[data-from]').value !== $('[data-to]').value) actions.sendAttack(pt($('[data-from]').value), pt($('[data-to]').value), seconds()); };

  $('[data-demo]').onclick = () => {
    const all = [...world.castles.values()];
    if (all.length < 2) return;
    for (let i = 0; i < 6; i++) {
      const a = all[Math.floor(Math.random() * all.length)];
      let b = a;
      while (b === a) b = all[Math.floor(Math.random() * all.length)];
      actions.sendAttack({ x: a.x, y: a.y }, { x: b.x, y: b.y }, 25 + Math.round(Math.random() * 40));
    }
  };

  // ---- dev: teams -----------------------------------------------------------------------------------------------------------------------------------
  for (const t of Object.values(TEAMS)) {
    const b = document.createElement('button');
    b.textContent = t.name; b.dataset.team = t.id;
    b.onclick = () => { if (focus.kingdomId) { world.setTeam(focus.kingdomId, t.id); syncChip(); } };
    $('[data-teams]').append(b);
  }
  const edit = (slot) => (e) => {
    const k = world.kingdoms.get(focus.kingdomId);
    if (!k) return;
    teams[k.team][slot] = e.target.value;
    actions.teamsChanged(teams[k.team]);
    syncChip();
  };
  $('[data-ca]').oninput = edit('primary');
  $('[data-cb]').oninput = edit('secondary');

  // ---- lighting panel: presets (Default / Ambient / Warm) and the knobs behind them ---------------------------------------------------------------------
  const light = stage.lighting;
  const SLIDERS = [['ambient', 'Ambient', 0, 3, 0.05], ['env', 'Sky fill', 0, 1.5, 0.05], ['sun', 'Sun', 0, 5, 0.1], ['warmth', 'Warmth', -1, 1, 0.05], ['exposure', 'Exposure', 0.3, 1.8, 0.05], ['azimuth', 'Sun angle', -180, 180, 1], ['elevation', 'Sun height', 5, 90, 1]];
  for (const name of light.presets) {
    const b = document.createElement('button');
    b.textContent = name; b.dataset.preset = name;
    b.onclick = () => light.preset(name);
    $('[data-presets]').append(b);
  }
  $('[data-sliders]').innerHTML = SLIDERS.map(([key, label, min, max, step]) => `<span>${label}</span><input type="range" data-l="${key}" min="${min}" max="${max}" step="${step}"><output data-o="${key}"></output>`).join('');
  for (const input of $$('[data-l]')) input.oninput = () => light.set({ [input.dataset.l]: +input.value });
  $('[data-shadows]').onchange = (e) => light.set({ shadows: e.target.checked });
  light.onChange((p) => {
    for (const input of $$('[data-l]')) { input.value = p[input.dataset.l]; $(`[data-o="${input.dataset.l}"]`).textContent = (+p[input.dataset.l]).toFixed(input.step < 1 ? 2 : 0); }
    $('[data-shadows]').checked = p.shadows;
    for (const b of $$('[data-preset]')) b.classList.toggle('on', Object.entries(light.presetValues(b.dataset.preset)).every(([k, v]) => p[k] === v));
  });
  light.set({});   // paint the panel from the current values

  // ---- view / overlay toggles --------------------------------------------------------------------------------------------------------------------
  for (const b of $$('[data-view]')) b.onclick = () => stage.view(b.dataset.view);
  const toggle = (name, fn) => { const b = $(`[data-t="${name}"]`); b.onclick = () => { b.classList.toggle('on'); fn(b.classList.contains('on')); }; };
  toggle('grid', (v) => gridView.setGrid(v));
  toggle('blocked', (v) => gridView.setBlocked(v));
  toggle('dev', (v) => { $('[data-dev]').hidden = !v; });
  toggle('light', (v) => { $('[data-light]').hidden = !v; });

  // ---- click flow: pick a castle (source), then any castle or NPC (target) ----------------------------------------------------------------------
  const hint = (t) => { $('[data-hint]').textContent = t; $('[data-hint]').style.opacity = t ? 1 : 0; };
  hint('Click a castle, then a target, to send an attack');
  const ui = {
    get source() { return source; },
    click(e) {
      if (!e) { source = null; actions.markSource(null); hint(''); return; }
      if (!source) {
        if (e.kind !== 'castle') { hint('Pick one of your castles first'); return; }
        source = e; actions.markSource(e); actions.preview(null);
        hint(`${e.name}: now click a target`);
      } else if (e === source) { source = null; actions.markSource(null); actions.preview(null); hint(''); }
      else { actions.sendAttack({ x: source.x, y: source.y }, { x: e.x, y: e.y }, seconds()); source = null; actions.markSource(null); actions.preview(null); hint(''); }
    },
    hover(e) {
      if (source && e && e !== source) { actions.preview(source, e); hint(`${source.name} → ${e.name}, ${fmt(seconds())}`); }
      else { actions.preview(null); if (source) hint(`${source.name}: now click a target`); }
    },
    setStats(text) { $('[data-stats]').textContent = text; },
    update(dt) {
      listTimer += dt;
      if (listTimer < 0.25 || $('[data-dev]').hidden) return;
      listTimer = 0;
      const rows = [...world.marches.values()].slice(0, 12).map((m) => {
        const p = world.progress(m);
        return `<div class="mw-march"><span>${esc(world.kingdoms.get(m.kingdomId)?.name ?? '?')} → (${m.to.x}, ${m.to.y})</span><span>${p > 0 ? fmt(m.endAt - world.now) : 'queued'}</span></div><div class="mw-bar2"><i style="width:${(p * 100).toFixed(0)}%"></i></div>`;
      });
      $('[data-marches]').innerHTML = rows.length ? rows.join('') + (world.marches.size > 12 ? `<small class="mw-dim">+${world.marches.size - 12} more</small>` : '') : '<small class="mw-dim">none on the road</small>';
    },
  };
  fillKingdoms(); fillFrom(); fillTo(); syncChip();
  return ui;
}
