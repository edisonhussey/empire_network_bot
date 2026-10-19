// Single static mesh -> .glb (the castle). One primitive per `Prim` (one material each); see gltf_io.mjs for the plumbing.
// Vertex attributes: POSITION, NORMAL, COLOR_0 (linear, 16-bit normalised). Geometry is non-indexed (flat shading needs a normal per face).
import { BinPack, addPrimitive, writeContainer } from './gltf_io.mjs';

export function writeGlb(path, prims, { name = 'asset', extras = {} } = {}) {
  const pack = new BinPack();
  const materials = [];
  const primitives = prims.filter((p) => p.triangles).map((p) => addPrimitive(pack, p, materials));
  const json = {
    asset: { version: '2.0', generator: 'interactive_world/tools/build_castle.mjs' },
    scene: 0,
    scenes: [{ nodes: [0] }],
    nodes: [{ mesh: 0, name }],
    meshes: [{ name, primitives }],
    materials,
    extras,
  };
  return { bytes: writeContainer(path, json, pack), primitives: primitives.length };
}
