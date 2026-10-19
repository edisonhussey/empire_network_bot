// Animation clips for the squad: Idle, March, Attack, Cheer. A pose is sampled at a handful of keys per loop (linear interpolation in the
// viewer), shifted by a per-soldier phase so the squad never moves in perfect lockstep.
//
// Angle convention (soldier faces +z): a negative X rotation swings an arm or leg forward; a positive X rotation on the torso leans forward.
const rad = (d) => (d * Math.PI) / 180;
const qx = (deg) => [Math.sin(rad(deg) / 2), 0, 0, Math.cos(rad(deg) / 2)];
const qy = (deg) => [0, Math.sin(rad(deg) / 2), 0, Math.cos(rad(deg) / 2)];
const smooth = (x) => { const t = Math.min(1, Math.max(0, x)); return t * t * (3 - 2 * t); };
const wrap = (u) => u - Math.floor(u);

/** Smoothly interpolated looping track: keys [[u, value], ...] with u from 0 to 1 (first and last values equal). */
function track(u, keys) {
  u = wrap(u);
  for (let i = 0; i < keys.length - 1; i++) {
    const [u0, v0] = keys[i], [u1, v1] = keys[i + 1];
    if (u >= u0 && u <= u1) return v0 + (v1 - v0) * smooth((u - u0) / (u1 - u0 || 1));
  }
  return keys[keys.length - 1][1];
}

// How each weapon arm moves in an attack: [rest, wind-up, strike].
const STRIKE = { sword: [-50, -150, -8], spear: [-90, -108, -8], banner: [-60, -60, -60], none: [-30, -30, -30] };
const SWAY = { sword: -50, spear: -90, banner: -60, none: -30 };

/** Pose of one soldier at loop position u. Returns joint values: body offset [x,y,z], and X-rotation angles in degrees. */
const POSES = {
  Idle: { seconds: 2.0, keys: 8, pose: (u, kind) => ({
    body: [0, 0.0035 * Math.sin(u * Math.PI * 2), 0],
    armR: kind === 'banner' ? -60 : SWAY[kind] + 3 * Math.sin(u * Math.PI * 2),
    armL: -30 - 3 * Math.sin(u * Math.PI * 2 + 1),
    legR: 0, legL: 0, torso: 1.5 * Math.sin(u * Math.PI * 2), head: 2 * Math.sin(u * Math.PI * 2 + 0.6), sway: 5 * Math.sin(u * Math.PI * 2),
  }) },
  March: { seconds: 0.8, keys: 8, pose: (u, kind) => {
    const s = Math.sin(u * Math.PI * 2);
    return {
      body: [0, 0.011 * Math.abs(s), 0],
      armR: kind === 'banner' ? -60 : SWAY[kind] - 18 * s, armL: -32 + 14 * s,
      legL: -34 * s, legR: 34 * s, torso: 3, head: 2 * Math.abs(s) - 2, sway: 14 * s,
    };
  } },
  Attack: { seconds: 1.2, keys: 12, pose: (u, kind) => {
    const [rest, wind, hit] = STRIKE[kind];
    const arm = track(u, [[0, rest], [0.3, wind], [0.45, hit], [0.72, hit * 0.7 + rest * 0.3], [1, rest]]);
    const lunge = track(u, [[0, 0], [0.3, -0.014], [0.45, 0.045], [0.75, 0.012], [1, 0]]);
    return {
      body: [0, 0.006 * Math.sin(Math.min(1, u / 0.45) * Math.PI), lunge],
      armR: arm, armL: kind === 'none' || kind === 'banner' ? track(u, [[0, -30], [0.35, -165], [0.7, -140], [1, -30]]) : track(u, [[0, -30], [0.3, -20], [0.45, -62], [0.8, -40], [1, -30]]),
      legL: track(u, [[0, 0], [0.3, 8], [0.45, -26], [0.8, -6], [1, 0]]), legR: track(u, [[0, 0], [0.3, -4], [0.45, 14], [0.8, 4], [1, 0]]),
      torso: track(u, [[0, 0], [0.3, -9], [0.45, 20], [0.8, 6], [1, 0]]), head: track(u, [[0, 0], [0.45, 6], [1, 0]]), sway: 22 * Math.sin(u * Math.PI * 2),
    };
  } },
  Cheer: { seconds: 1.0, keys: 10, pose: (u, kind) => {
    const hop = Math.abs(Math.sin(u * Math.PI * 2));
    const wiggle = 12 * Math.sin(u * Math.PI * 4);
    return {
      body: [0, 0.05 * hop, 0],
      armR: kind === 'banner' ? -60 : -165 + wiggle, armL: kind === 'sword' ? -125 - wiggle : -165 - wiggle, // shield arms stay lower so the shield clears the head
      legL: -12 * hop, legR: 12 * hop, torso: -4 * hop, head: -8, sway: 25 * Math.sin(u * Math.PI * 2),
    };
  } },
};

export const CLIPS = Object.keys(POSES);

/**
 * Build the channels of clip `name` for the soldiers. Each unit: { kind, phase, nodes: { body, torso, head, armR, armL, legR, legL, banner? } }.
 * `phaseSpread` scales how far apart in time the soldiers are.
 */
export function clipChannels(name, units) {
  const { seconds, keys, pose } = POSES[name];
  const times = Array.from({ length: keys + 1 }, (_, i) => +((i / keys) * seconds).toFixed(4));
  const channels = [];
  const spread = { Idle: 1, March: 0.12, Attack: 0.28, Cheer: 0.35 }[name];
  for (const unit of units) {
    const samples = times.map((_, i) => pose(i / keys + unit.phase * spread, unit.kind));
    const n = unit.nodes;
    channels.push({ node: n.body, path: 'translation', times, values: samples.map((p) => p.body) });
    for (const joint of ['torso', 'head', 'armR', 'armL', 'legR', 'legL']) {
      if (name !== 'Idle' && unit.kind === 'banner' && joint === 'armR') continue; // the bearer keeps hold of the pole
      channels.push({ node: n[joint], path: 'rotation', times, values: samples.map((p) => qx(p[joint])) });
    }
    if (n.banner !== undefined) channels.push({ node: n.banner, path: 'rotation', times, values: samples.map((p) => qy(p.sway)) });
  }
  return channels;
}
