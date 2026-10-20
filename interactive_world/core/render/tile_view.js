// Layer 3, the tile terrain on screen: the kit GLB (five tile templates + pieces), merged tile tops with a painted ground shader, cliff and
// riser walls, water with ripples, waterfalls, and every dressing piece drawn with one InstancedMesh per piece primitive.
// Rules: architecture/TILE_TERRAIN_ARCHITECTURE.md. Data comes from core/tiles (grid, dress); this file only draws it.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { buildTileTops, buildTileWalls, buildWater } from '../tiles/tile_mesh.js';
import { GROUND, NO_WATER } from '../tiles/tile_grid.js';

const TEX = '../assets/development/textures/';
const CHUNK = 16;
const SHADOW_KINDS = /^(Tree|Rock|Cliff|Bush_(Round|Dark|Blossom)|Path_Stones)/;

/** Load the kit: tile templates (non-indexed positions per shape) and every piece's primitives. */
export async function loadKit(url) {
  const gltf = await new GLTFLoader().loadAsync(url);
  const pieces = new Map(), templates = {}, extras = {};
  for (const node of gltf.scene.children) {
    const prims = [];
    node.updateMatrixWorld(true);
    node.traverse((o) => { if (o.isMesh) prims.push({ geometry: o.geometry, material: o.material }); });
    pieces.set(node.name, prims);
    extras[node.name] = node.userData;
    const t = node.name.match(/^Tile_(.+)$/);
    if (t) templates[t[1]] = prims[0].geometry.toNonIndexed().attributes.position.array.slice();
  }
  return { pieces, templates, extras };
}

function texture(name, repeat = true) {
  const t = new THREE.TextureLoader().load(TEX + name + '.png');
  t.colorSpace = THREE.SRGBColorSpace;
  if (repeat) t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.anisotropy = 8;
  return t;
}

/** Per-tile ground weights as a texture (one texel per tile, linear filtered so edges blend): R path, G sand, B stream bed. */
function splatTexture(grid) {
  const data = new Uint8Array(grid.tileCount * 4);
  for (let i = 0; i < grid.tileCount; i++) {
    const g = grid.ground[i];
    data[i * 4] = g === GROUND.path ? 255 : 0;
    data[i * 4 + 1] = g === GROUND.sand ? 255 : 0;
    data[i * 4 + 2] = g === GROUND.bed ? 255 : 0;
    data[i * 4 + 3] = 255;
  }
  const t = new THREE.DataTexture(data, grid.width, grid.depth, THREE.RGBAFormat);
  t.magFilter = t.minFilter = THREE.LinearFilter;
  t.needsUpdate = true;
  return t;
}

function groundMaterial(grid, uniforms) {
  const m = new THREE.MeshStandardMaterial({ roughness: 0.95, metalness: 0 });
  m.onBeforeCompile = (sh) => {
    Object.assign(sh.uniforms, uniforms);
    sh.vertexShader = sh.vertexShader.replace('#include <common>', '#include <common>\nvarying vec3 vWorld;')
      .replace('#include <worldpos_vertex>', '#include <worldpos_vertex>\nvWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;');
    sh.fragmentShader = sh.fragmentShader.replace('#include <common>', `#include <common>
      varying vec3 vWorld;
      uniform sampler2D tGrass, tDirt, tSand, tSplat; uniform vec2 uSize; uniform float uGrid;`)
      .replace('#include <map_fragment>', `
      vec2 wuv = vWorld.xz / 4.0;
      vec3 grass = texture2D(tGrass, wuv).rgb;
      grass = mix(grass, texture2D(tGrass, wuv * 0.31 + vec2(0.37, 0.11)).rgb, 0.4);          // a second scale hides the repeat
      vec3 dirt = texture2D(tDirt, wuv).rgb;
      vec3 sand = texture2D(tSand, vWorld.xz / 3.0).rgb;
      vec4 sp = texture2D(tSplat, vWorld.xz / uSize);
      float nz = texture2D(tGrass, wuv * 2.7).g - 0.45;
      float path = smoothstep(0.32, 0.62, sp.r + nz * 0.55);
      float sandW = smoothstep(0.30, 0.70, sp.g + nz * 0.5);
      float bed = smoothstep(0.2, 0.6, sp.b);
      vec3 col = mix(grass, sand, sandW);
      col = mix(col, dirt, path);
      col = mix(col, sand * vec3(0.62, 0.66, 0.62), bed);
      col *= 0.94 + 0.08 * clamp(vWorld.y / 5.0, 0.0, 1.0);
      vec2 f = abs(fract(vWorld.xz) - 0.5);
      col = mix(col, vec3(1.0), uGrid * 0.32 * smoothstep(0.475, 0.497, max(f.x, f.y)));
      diffuseColor.rgb *= col;`);
  };
  return m;
}

function wallMaterial(uniforms) {
  const m = new THREE.MeshStandardMaterial({ roughness: 0.95, metalness: 0, side: THREE.FrontSide });
  m.onBeforeCompile = (sh) => {
    Object.assign(sh.uniforms, uniforms);
    sh.vertexShader = sh.vertexShader.replace('#include <common>', '#include <common>\nattribute float soil;\nvarying float vSoil;\nvarying vec3 vWorld;')
      .replace('#include <worldpos_vertex>', '#include <worldpos_vertex>\nvWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;\nvSoil = soil;');
    sh.fragmentShader = sh.fragmentShader.replace('#include <common>', '#include <common>\nvarying float vSoil;\nvarying vec3 vWorld;\nuniform sampler2D tRock, tSoil;')
      .replace('#include <map_fragment>', `
      vec2 wuv = vec2(vWorld.x + vWorld.z, vWorld.y) / 2.0;
      vec3 col = mix(texture2D(tRock, wuv).rgb * 0.92, texture2D(tSoil, wuv).rgb, vSoil);
      col *= mix(0.45, 1.0, smoothstep(-3.0, 0.4, vWorld.y));                                    // the island slab darkens downwards
      diffuseColor.rgb *= col;`);
  };
  return m;
}

function waterMaterial(uniforms) {
  const m = new THREE.MeshStandardMaterial({ color: '#3d8fb8', roughness: 0.08, metalness: 0.0, transparent: true, opacity: 0.86, depthWrite: false, side: THREE.DoubleSide });
  m.onBeforeCompile = (sh) => {
    Object.assign(sh.uniforms, uniforms);
    sh.vertexShader = sh.vertexShader.replace('#include <common>', '#include <common>\nvarying vec3 vWorld;')
      .replace('#include <worldpos_vertex>', '#include <worldpos_vertex>\nvWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;');
    sh.fragmentShader = sh.fragmentShader.replace('#include <common>', '#include <common>\nvarying vec3 vWorld;\nuniform float uTime;\nuniform sampler2D tSplat;\nuniform vec2 uSize;')
      .replace('#include <normal_fragment_maps>', `#include <normal_fragment_maps>
      {
        vec2 p = vWorld.xz; float t = uTime;
        float gx = 0.0, gz = 0.0;
        gx += cos(p.x * 3.1 + p.y * 1.3 + t * 1.7) * 0.06; gz += cos(p.x * 1.1 - p.y * 3.7 - t * 2.3) * 0.07;
        gx += cos(p.x * 7.3 - p.y * 5.1 + t * 3.1) * 0.03; gz += cos(p.y * 8.9 + p.x * 2.0 - t * 3.9) * 0.035;
        vec3 nW = normalize(vec3(-gx, 1.0, -gz));
        normal = normalize((viewMatrix * vec4(nW, 0.0)).xyz);
      }`)
      .replace('#include <color_fragment>', `#include <color_fragment>
      {
        // shore: the splat's water channel, linearly filtered, falls off towards dry tiles -> shallow, lighter water and a foam line
        float wet = texture2D(tSplat, vWorld.xz / uSize).b;
        float deep = smoothstep(0.55, 1.0, wet);
        diffuseColor.rgb = mix(vec3(0.24, 0.55, 0.62), vec3(0.05, 0.25, 0.42), deep);
        float f = sin(vWorld.x * 9.0 + uTime * 1.3) * sin(vWorld.z * 8.0 - uTime * 1.1);
        float foam = smoothstep(0.62, 0.5, wet) * (0.55 + 0.45 * f);
        diffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.92, 0.97, 1.0), clamp(foam, 0.0, 1.0) * 0.7);
        diffuseColor.a = mix(0.9, 0.93, deep);
      }`);
  };
  return m;
}

function fallMaterial(uniforms) {
  return new THREE.ShaderMaterial({
    uniforms, transparent: true, depthWrite: false, side: THREE.DoubleSide,
    vertexShader: `varying vec2 vUv; varying float vY; void main(){ vUv = uv; vec4 w = modelMatrix * vec4(position, 1.0); vY = w.y; gl_Position = projectionMatrix * viewMatrix * w; }`,
    fragmentShader: `varying vec2 vUv; uniform float uTime;
      float h(vec2 p){ return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
      float n(vec2 p){ vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
        return mix(mix(h(i), h(i + vec2(1, 0)), f.x), mix(h(i + vec2(0, 1)), h(i + vec2(1, 1)), f.x), f.y); }
      void main(){
        float s = n(vec2(vUv.x * 9.0, vUv.y * 2.5 + uTime * 2.6)) * 0.6 + n(vec2(vUv.x * 23.0, vUv.y * 5.0 + uTime * 3.4)) * 0.4;
        vec3 col = mix(vec3(0.35, 0.62, 0.78), vec3(0.93, 0.98, 1.0), smoothstep(0.35, 0.8, s));
        float foam = smoothstep(0.25, 0.0, vUv.y);
        col = mix(col, vec3(1.0), foam * 0.8);
        float edge = smoothstep(0.0, 0.08, vUv.x) * smoothstep(1.0, 0.92, vUv.x);
        gl_FragColor = vec4(col, (0.72 + 0.25 * s) * edge);
        #include <colorspace_fragment>
      }`,
  });
}

/** Foam rings where falls land: a soft white disc, drawn flat on the lower water. */
function foamMaterial(uniforms) {
  return new THREE.ShaderMaterial({
    uniforms, transparent: true, depthWrite: false,
    vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
    fragmentShader: `varying vec2 vUv; uniform float uTime;
      void main(){ vec2 d = vUv - 0.5; float r = length(d) * 2.0; float a = atan(d.y, d.x);
        float ring = 0.55 + 0.45 * sin(a * 7.0 + uTime * 3.0) * sin(r * 9.0 - uTime * 4.0);
        float alpha = smoothstep(1.0, 0.3, r) * (0.55 + 0.35 * ring);
        gl_FragColor = vec4(vec3(0.96, 0.99, 1.0), alpha);
        #include <colorspace_fragment>
      }`,
  });
}

/**
 * @param scene THREE.Scene
 * @param grid TileGrid
 * @param kit loadKit() result
 * @param dressing dressGrid() result ({ pieces, falls })
 */
export function createTileView(scene, grid, kit, dressing) {
  const group = new THREE.Group();
  group.name = 'tiles';
  scene.add(group);
  const t0 = performance.now();
  const uniforms = {
    tGrass: { value: texture('terrain_grass') }, tDirt: { value: texture('terrain_dirt') }, tSand: { value: texture('terrain_sand') },
    tRock: { value: texture('kit_rock') }, tSoil: { value: texture('kit_soil') }, tSplat: { value: splatTexture(grid) },
    uSize: { value: new THREE.Vector2(grid.width, grid.depth) }, uGrid: { value: 0 }, uTime: { value: 0 },
  };
  const ground = groundMaterial(grid, uniforms), walls = wallMaterial(uniforms), water = waterMaterial(uniforms);
  const stats = { tops: 0, walls: 0, water: 0, pieces: 0, instances: 0, pieceTris: 0, ms: 0 };
  const tops = [], wallMeshes = [];

  for (let z0 = 0; z0 < grid.depth; z0 += CHUNK) for (let x0 = 0; x0 < grid.width; x0 += CHUNK) {
    const range = { x0, z0, x1: Math.min(grid.width, x0 + CHUNK), z1: Math.min(grid.depth, z0 + CHUNK) };
    const t = buildTileTops(grid, kit.templates, range);
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(t.positions, 3));
    g.setAttribute('normal', new THREE.BufferAttribute(t.normals, 3));
    const mesh = new THREE.Mesh(g, ground);
    mesh.receiveShadow = true;
    mesh.userData = { range, tileOfTri: t.tileOfTri };
    group.add(mesh); tops.push(mesh);
    stats.tops += t.triangles;

    const w = buildTileWalls(grid, range, { skirtBase: -3 });
    if (w.triangles) {
      const wg = new THREE.BufferGeometry();
      wg.setAttribute('position', new THREE.BufferAttribute(w.positions, 3));
      wg.setAttribute('normal', new THREE.BufferAttribute(w.normals, 3));
      wg.setAttribute('soil', new THREE.BufferAttribute(w.soil, 1));
      const wm = new THREE.Mesh(wg, walls);
      wm.receiveShadow = true; wm.castShadow = true;
      group.add(wm); wallMeshes.push(wm);
      stats.walls += w.triangles;
    }
    const wa = buildWater(grid, range);
    if (wa.triangles) {
      const ag = new THREE.BufferGeometry();
      ag.setAttribute('position', new THREE.BufferAttribute(wa.positions, 3));
      ag.computeVertexNormals();
      const am = new THREE.Mesh(ag, water);
      am.receiveShadow = true;
      am.renderOrder = 2;
      group.add(am);
      stats.water += wa.triangles;
    }
  }

  // waterfalls: a bent sheet from the upper water down to the lower water on each fall edge, and foam where it lands
  const fallMat = fallMaterial(uniforms), foamMat = foamMaterial(uniforms);
  for (const f of dressing.falls ?? []) {
    const ax = f.ax, az = f.az, bx = f.bx, bz = f.bz, out = 0.45;
    const pos = [], uv = [];
    const rows = [[0, f.top + 0.02, 1], [0.12, f.top - 0.15, 0.9], [out * 0.7, (f.top + f.bottom) / 2, 0.45], [out, f.bottom, 0]];
    for (const [o, y, v] of rows) {
      pos.push(ax + f.nx * o, y, az + f.nz * o, bx + f.nx * o, y, bz + f.nz * o);
      uv.push(0, v, 1, v);
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
    g.setAttribute('uv', new THREE.Float32BufferAttribute(uv, 2));
    g.setIndex([0, 2, 1, 1, 2, 3, 2, 4, 3, 3, 4, 5, 4, 6, 5, 5, 6, 7]);
    const sheet = new THREE.Mesh(g, fallMat);
    sheet.renderOrder = 3;
    group.add(sheet);
    const foam = new THREE.Mesh(new THREE.PlaneGeometry(1.8, 1.8), foamMat);
    foam.rotation.x = -Math.PI / 2;
    foam.position.set(f.x + f.nx * (out + 0.3), f.bottom + 0.02, f.z + f.nz * (out + 0.3));
    foam.renderOrder = 4;
    group.add(foam);
  }

  // pieces: one InstancedMesh per piece primitive
  const props = new THREE.Group();
  props.name = 'pieces';
  group.add(props);
  const m4 = new THREE.Matrix4(), q = new THREE.Quaternion(), qt = new THREE.Quaternion(), up = new THREE.Vector3(0, 1, 0), nv = new THREE.Vector3();
  const pos = new THREE.Vector3(), scl = new THREE.Vector3();
  const tuned = new Set();
  for (const [name, list] of Object.entries(dressing.pieces)) {
    const prims = kit.pieces.get(name);
    if (!prims || !list.length) { if (!prims) console.warn(`kit has no piece ${name}`); continue; }
    for (const { geometry, material } of prims) {
      if (!tuned.has(material)) {
        tuned.add(material);
        if (material.alphaTest > 0 || material.transparent) {
          // foliage cards: alpha cut-out, both sides drawn, and both sides lit with the authored (outward / upward) normal instead of flipping it
          material.alphaTest = 0.45; material.transparent = false; material.side = THREE.DoubleSide;
          material.onBeforeCompile = (sh) => { sh.fragmentShader = sh.fragmentShader.replace('#include <normal_fragment_begin>', THREE.ShaderChunk.normal_fragment_begin.replace('gl_FrontFacing ? 1.0 : - 1.0', '1.0')); };
        }
      }
      const im = new THREE.InstancedMesh(geometry, material, list.length);
      list.forEach((e, i) => {
        q.setFromAxisAngle(up, e.ry);
        if (e.tilt) { nv.set(...e.tilt).lerp(up, 0.35).normalize(); qt.setFromUnitVectors(up, nv); q.premultiply(qt); }
        pos.set(e.x, e.y, e.z);
        scl.set(e.s * (e.sx ?? 1), e.s * e.sy, e.s);
        im.setMatrixAt(i, m4.compose(pos, q, scl));
      });
      im.castShadow = SHADOW_KINDS.test(name);
      im.receiveShadow = true;
      im.name = name;
      im.computeBoundingSphere();
      props.add(im);
      stats.pieces++;
      stats.pieceTris += (geometry.index ? geometry.index.count : geometry.attributes.position.count) / 3 * list.length;
    }
    stats.instances += list.length;
  }
  stats.ms = performance.now() - t0;

  return {
    group, stats, uniforms,
    topMeshes: () => tops,
    setGrid: (v) => { uniforms.uGrid.value = v ? 1 : 0; },
    setWireframe: (v) => { ground.wireframe = walls.wireframe = v; },
    setProps: (v) => { props.visible = v; },
    tick: (t) => { uniforms.uTime.value = t; },
  };
}

export { NO_WATER };
