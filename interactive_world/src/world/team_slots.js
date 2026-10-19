// Team-tinted geometry. Parts the game recolours per player (castle flag, tunics, shield rims, banners, parade ring) are exported in
// materials named team_a (primary) / team_b (secondary), with "_ds" variants for double-sided cloth. Their vertex colours are a neutral
// grey (shading only); teams.js multiplies in the team colour at runtime. Shared by the generators (node) and the viewers (browser).
export const TEAM_GREY = 0.9; // sRGB grey baked into the vertex colours of team parts

/** 'a' | 'b' | null for a material name. */
export const slotOf = (name) => (name?.startsWith('team_a') ? 'a' : name?.startsWith('team_b') ? 'b' : null);
