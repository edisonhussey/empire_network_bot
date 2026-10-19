// The map asset library (assets/map_library.glb): every piece by name -> its mesh parts (geometry + material), ready for instancing.
// Pieces are centred on their own origin with y = 0 on the ground, so placing one is just a position, a rotation and a scale (`scale`: see asset_scale.js).
import { slotOf } from '../world/team_slots.js';
import { assetScale } from './asset_scale.js';

/** parts: [{ geometry, material }] per piece; team-coloured materials (banners) are swapped for `team`'s shared ones via `skins`. */
export async function loadLibrary(loader, url, { skins, team }) {
  const gltf = await loader.loadAsync(url);
  const info = gltf.parser.json.extras?.assets ?? {};
  const assets = new Map();
  for (const name of Object.keys(info)) {
    const node = gltf.scene.getObjectByName(name);
    const parts = [];
    node?.traverse((o) => {
      if (!o.isMesh) return;
      parts.push({ geometry: o.geometry, material: slotOf(o.material.name) ? skins.materialFor(team, o.material) : o.material });
    });
    assets.set(name, { name, parts, ...info[name], scale: assetScale(name) });
  }
  return assets;
}
