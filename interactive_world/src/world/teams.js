// Teams and their colours. A team is { id, name, primary, secondary } (sRGB hex strings). Geometry exported with team_a / team_b materials
// (see team_slots.js) is tinted with these. Materials are cached per team, so changing a team's colours and calling refresh() recolours
// every castle flag and every soldier of that team at once. No three.js import: it only touches material.color / material.clone().
import { TEAM_GREY, slotOf } from './team_slots.js';

export const TEAMS = {
  blue:   { id: 'blue',   name: 'Blue',   primary: '#1f5cb8', secondary: '#c4213d' },
  red:    { id: 'red',    name: 'Red',    primary: '#c4213d', secondary: '#e8c040' },
  green:  { id: 'green',  name: 'Green',  primary: '#4f9a2e', secondary: '#f0e6c8' },
  gold:   { id: 'gold',   name: 'Gold',   primary: '#d9a93a', secondary: '#1f5cb8' },
  purple: { id: 'purple', name: 'Purple', primary: '#7a3fb0', secondary: '#f0e6c8' },
  baron:  { id: 'baron',  name: 'Baron',  primary: '#2a56a8', secondary: '#e0b24a' },   // robber baron banners (not offered as a kingdom colour)
};

// vertex colours are linear-grey TEAM_GREY; scale the material colour so (material x vertex) lands exactly on the team colour
const GREY_LINEAR = Math.pow((TEAM_GREY + 0.055) / 1.055, 2.4);

export class TeamSkins {
  constructor() {
    this.cache = new Map(); // `${team.id}|${templateName}` -> { material, slot, team }
  }

  #paint(entry) {
    const { material, slot, team } = entry;
    material.color.set(slot === 'a' ? team.primary : team.secondary).multiplyScalar(1 / GREY_LINEAR);
  }

  /** The shared material for a team-coloured template material (also used for instanced meshes). */
  materialFor(team, template) {
    const key = `${team.id}|${template.name}`;
    let entry = this.cache.get(key);
    if (!entry) {
      entry = { material: template.clone(), slot: slotOf(template.name), team };
      this.cache.set(key, entry);
      this.#paint(entry);
    }
    return entry.material;
  }

  /** Give every team-coloured part under `root` the team's shared materials. Safe to call again with another team. */
  apply(root, team) {
    root.traverse((o) => {
      if (!o.isMesh || o.name === 'outline') return;
      o.userData.teamTemplate ??= slotOf(o.material.name) ? o.material : null;
      if (!o.userData.teamTemplate) return;
      o.material = this.materialFor(team, o.userData.teamTemplate);
      if (o.userData.base) o.userData.base = o.material;
    });
  }

  /** Call after editing team.primary / team.secondary. */
  refresh(team) {
    for (const entry of this.cache.values()) if (entry.team.id === team.id) this.#paint(entry);
  }
}
