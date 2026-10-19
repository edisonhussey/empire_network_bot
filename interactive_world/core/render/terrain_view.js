// Layer 4, terrain rendering: turns the typed arrays of each chunk (terrain/chunks.js) into three.js meshes. One group per chunk holding
// tops, connector walls, the map-edge skirt and the cell outlines, so a cell edit rebuilds only the chunks it touches (rebuild(keys)).
// Greyscale on purpose: tops are shaded by height (low = darker), connectors a darker grey, the skirt darker still.
import * as THREE from 'three';
import { chunkLayout, buildChunk } from '../terrain/chunks.js';

const GREY_LOW = 0.26, GREY_HIGH = 0.56;   // top-surface albedo (linear) from the lowest to the highest ground

export function createTerrainView(scene, grid) {
  const root = new THREE.Group();
  scene.add(root);
  const { min, max } = grid.range();
  const base = Math.floor(min) - 4;   // the skirt's bottom
  const mats = {
    top: new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95, metalness: 0 }),
    connector: new THREE.MeshStandardMaterial({ color: '#5c5d60', roughness: 0.95, metalness: 0 }),
    skirt: new THREE.MeshStandardMaterial({ color: '#3f4043', roughness: 1, metalness: 0 }),
    outline: new THREE.LineBasicMaterial({ color: '#2b2c2e', transparent: true, opacity: 0.35, depthWrite: false }),
  };
  const CONNECTOR_GREY = new THREE.Color('#5c5d60'), CONNECTOR_HIGHLIGHT = new THREE.Color('#e3a54a');
  const chunks = new Map();   // key -> { group, top }
  const stats = { top: 0, connectors: 0, skirt: 0, edges: 0, chunks: 0, ms: 0 };

  const geometry = (part, colorByHeight = false) => {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(part.positions, 3));
    g.setAttribute('normal', new THREE.BufferAttribute(part.normals, 3));
    if (colorByHeight) {
      const p = part.positions, c = new Float32Array(p.length);
      for (let i = 0; i < p.length; i += 9) {   // one value per triangle (its mean height), so each plane stays one flat tone
        const t = ((p[i + 1] + p[i + 4] + p[i + 7]) / 3 - min) / Math.max(1e-6, max - min);
        const v = GREY_LOW + (GREY_HIGH - GREY_LOW) * t;
        for (let k = 0; k < 9; k++) c[i + k] = v;
      }
      g.setAttribute('color', new THREE.BufferAttribute(c, 3));
    }
    g.computeBoundingSphere();
    return g;
  };

  function buildOne(c) {
    const old = chunks.get(c.key);
    if (old) { root.remove(old.group); old.group.traverse((o) => o.geometry?.dispose()); stats.top -= old.n.top; stats.connectors -= old.n.connectors; stats.skirt -= old.n.skirt; stats.edges -= old.n.edges; }
    const data = buildChunk(grid, c.range, { base });
    const group = new THREE.Group();
    group.name = `chunk ${c.key}`;
    const top = new THREE.Mesh(geometry(data.top, true), mats.top);
    top.receiveShadow = top.castShadow = true;
    top.userData.range = c.range;   // the cell of triangle i is cell i >> 1 of this range, row by row (picking)
    group.add(top);
    if (data.connectors.triangles) { const m = new THREE.Mesh(geometry(data.connectors), mats.connector); m.castShadow = m.receiveShadow = true; group.add(m); }
    if (data.skirt.triangles) { const m = new THREE.Mesh(geometry(data.skirt), mats.skirt); m.receiveShadow = true; group.add(m); }
    const lg = new THREE.BufferGeometry();
    lg.setAttribute('position', new THREE.BufferAttribute(data.outlines, 3));
    const lines = new THREE.LineSegments(lg, mats.outline);
    lines.name = 'outlines';
    group.add(lines);
    root.add(group);
    const n = { top: data.top.triangles, connectors: data.connectors.triangles, skirt: data.skirt.triangles, edges: data.connectors.edges };
    stats.top += n.top; stats.connectors += n.connectors; stats.skirt += n.skirt; stats.edges += n.edges;
    chunks.set(c.key, { group, top, n });
  }

  const layout = chunkLayout(grid);
  /** Build (or rebuild) chunks: all of them, or only `keys` (see chunksTouchedBy). */
  function rebuild(keys = null) {
    const t = performance.now();
    for (const c of layout) if (!keys || keys.includes(c.key)) buildOne(c);
    stats.chunks = chunks.size;
    stats.ms = performance.now() - t;
  }
  rebuild();

  return {
    root, stats,
    topMeshes: () => [...chunks.values()].map((c) => c.top),
    rebuild,
    setOutlines(on) { for (const c of chunks.values()) c.group.getObjectByName('outlines').visible = on; },
    setOutlineOpacity(o) { mats.outline.opacity = o; },
    setHighlightConnectors(on) { mats.connector.color.copy(on ? CONNECTOR_HIGHLIGHT : CONNECTOR_GREY); },
    /** Vertical exaggeration for inspection (1 = true heights). Data and picking are unaffected. */
    setRelief(k) { root.scale.y = k; },
    setWireframe(on) { for (const m of [mats.top, mats.connector, mats.skirt]) m.wireframe = on; },
  };
}
