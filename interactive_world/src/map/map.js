// Page bootstrap: mounts the map app full-window and applies scripted URL options (used for screenshots and demos).
//   world.html?kingdom=red&send=3,3>46,46:40;46,3>3,46:60&advance=15&view=Top&hover=25,25&dev&shot
import { createMapApp } from './app.js';
import { generated } from '../world/scenario.js';
import { World } from '../world/world.js';
import { TEAMS } from '../world/teams.js';

const params = new URLSearchParams(location.search);
const container = document.getElementById('app');
if (params.has('shot')) container.classList.add('shot');
// ?stress=3000 builds a GGE-sized map (1286 x 1286 nodes) with that many castles, to check the window / level-of-detail behaviour
function stressWorld(count) {
  const w = new World({ size: 1286 });
  const teamIds = Object.keys(TEAMS);
  let seed = 12345;
  const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
  teamIds.forEach((t, i) => w.addKingdom({ id: t, name: `${TEAMS[t].name} Kingdom`, team: t }));
  const used = new Set();
  for (let n = 0; n < count;) {
    const gx = Math.floor(rnd() * 250), gy = Math.floor(rnd() * 250), key = `${gx},${gy}`;
    if (used.has(key)) continue;
    used.add(key);
    w.addCastle(teamIds[(gx + gy) % teamIds.length], [gx * 5 + 3, gy * 5 + 3]);
    n++;
  }
  for (let i = 0; i < count / 4; i++) w.terrain.add({ kind: 'boulder', x: Math.floor(rnd() * 1270), y: Math.floor(rnd() * 1270), w: 2 + Math.floor(rnd() * 3), h: 2 });
  w.terrain.add({ kind: 'river', x: 640, y: -4, w: 3, h: 1300 });
  w.terrain.add({ kind: 'bridge', x: 640, y: 600, w: 3, h: 6 });
  return w;
}
const app = await createMapApp({ container, world: params.has('stress') ? stressWorld(+params.get('stress') || 3000) : null });
window.mapApi = app;

if (params.has('kingdom')) app.selectKingdom(params.get('kingdom'));
for (const spec of (params.get('send') || '').split(';').filter(Boolean)) {
  const [route, duration] = spec.split(':');
  const [from, to] = route.split('>').map((p) => p.split(',').map(Number));
  app.sendAttack({ from, to, duration: +duration || 30 });
}
if (params.has('view')) app.stage.view(params.get('view'), 0);
if (params.has('gen')) {
  const seed = +params.get('gen') || 1;
  app.setMapSpec(generated(seed).map);
  container.querySelector('[data-map]').value = 'gen';   // keep the panel in step with the map
  const box = container.querySelector('[data-seed]'); box.value = seed; box.hidden = container.querySelector('[data-roll]').hidden = false;
}
if (params.has('frame')) { const [x0, y0, x1, y1] = params.get('frame').split(',').map(Number); app.frame({ x0, y0, x1, y1 }, 0); }
if (params.has('advance')) app.simulate(+params.get('advance'));
if (params.has('zoom')) { const t = app.stage.controls.target, c = app.stage.camera; c.position.sub(t).multiplyScalar(+params.get('zoom')).add(t); }
app.refresh();
if (params.has('blocked')) container.querySelector('[data-t="blocked"]').click();
if (params.has('dev')) container.querySelector('[data-t="dev"]').click();
if (params.has('light')) { app.stage.lighting.preset(params.get('light')); if (params.has('panel')) container.querySelector('[data-t="light"]').click(); }
window.__ready = true;
