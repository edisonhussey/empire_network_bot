// NPC kinds (robber baron tower, dungeons, ...): what the *logic* needs to know about each: how much room it takes on the map.
// The 3D model for a kind is registered separately in the renderer (src/map/npc_view.js). Add kinds here when their art arrives.
export const NPC_KINDS = {
  dungeon: { label: 'Dungeon', footprintHalf: 1 },          // generic placeholder marker
  robber_baron: { label: 'Robber Baron', footprintHalf: 1 },  // the watchtower (assets/tower.glb)
};

export const registerNpcKind = (kind, spec) => { NPC_KINDS[kind] = { footprintHalf: 1, label: kind, ...spec }; };
export const npcSpec = (kind) => NPC_KINDS[kind] ?? { label: kind, footprintHalf: 1 };
