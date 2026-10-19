// A glTF scene writer for animated assets: shared meshes, a node tree and keyframe clips (translation / rotation channels).
//
//   const s = new SceneBuilder();
//   const mesh = s.mesh('torso', [primA, primB]);          // one mesh, one primitive per Prim; materials are shared by name
//   const node = s.node('torso', { mesh, t: [0, 0.07, 0], children: [...] });
//   s.root(node);
//   s.clip('March', [{ node, path: 'rotation', times: [...], values: [[x,y,z,w], ...] }]);
//   s.write('assets/army.glb');
import { BinPack, addPrimitive, writeContainer } from './gltf_io.mjs';

export class SceneBuilder {
  constructor({ generator = 'interactive_world/tools' } = {}) {
    this.pack = new BinPack();
    this.generator = generator;
    this.materials = [];
    this.materialIndex = new Map();
    this.meshes = [];
    this.nodes = [];
    this.roots = [];
    this.animations = [];
  }

  /** A mesh from Prims. Prims with the same name share one material (first one defines it). */
  mesh(name, prims) {
    const primitives = [];
    for (const p of prims) {
      if (!p.triangles) continue;
      let idx = this.materialIndex.get(p.name);
      const primitive = addPrimitive(this.pack, p, this.materials, idx ?? null);
      if (idx === undefined) this.materialIndex.set(p.name, primitive.material);
      primitives.push(primitive);
    }
    this.meshes.push({ name, primitives });
    return this.meshes.length - 1;
  }

  /** A node; `children` are node indices. Returns the node index. */
  node(name, { mesh, t, r, s, children } = {}) {
    const n = { name };
    if (mesh !== undefined) n.mesh = mesh;
    if (t) n.translation = t;
    if (r) n.rotation = r;
    if (s) n.scale = s;
    if (children?.length) n.children = children;
    this.nodes.push(n);
    return this.nodes.length - 1;
  }

  root(index) {
    this.roots.push(index);
  }

  /** A looping clip. channels: [{ node, path: 'translation'|'rotation'|'scale', times: number[], values: number[][] }] (LINEAR). */
  clip(name, channels) {
    const samplers = [], out = [];
    const timeAccessors = new Map();
    for (const ch of channels) {
      const key = ch.times.join(',');
      if (!timeAccessors.has(key)) {
        timeAccessors.set(key, this.pack.accessor(new Float32Array(ch.times), 5126, 'SCALAR', ch.times.length, { min: [Math.min(...ch.times)], max: [Math.max(...ch.times)] }));
      }
      const width = ch.values[0].length;
      const output = this.pack.accessor(new Float32Array(ch.values.flat()), 5126, width === 4 ? 'VEC4' : 'VEC3', ch.values.length);
      samplers.push({ input: timeAccessors.get(key), output, interpolation: 'LINEAR' });
      out.push({ sampler: samplers.length - 1, target: { node: ch.node, path: ch.path } });
    }
    this.animations.push({ name, samplers, channels: out });
  }

  write(path, extras = {}) {
    const json = {
      asset: { version: '2.0', generator: this.generator },
      scene: 0,
      scenes: [{ nodes: this.roots }],
      nodes: this.nodes,
      meshes: this.meshes,
      materials: this.materials,
      ...(this.animations.length ? { animations: this.animations } : {}),
      extras,
    };
    return writeContainer(path, json, this.pack);
  }
}
