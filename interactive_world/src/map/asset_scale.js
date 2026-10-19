// How big each map_library.glb piece is drawn, as [x, y, z] multipliers applied when it is placed (1 scene unit = 1 m). The library was modelled at
// 1.25 units per tile; LEGACY_SCALE (= CELL / 1.25 = 8) makes a piece fill the same number of nodes as before, which is right for rocks, cliffs and
// fields. Things with a real-world size get their own numbers, chosen from what they are (the model's size is in the comment).
import { LEGACY_SCALE as U } from '../world/config.js';

const TABLE = {
  house_thatch: 3.6, house_tile: 3.6,                  // 2.3 x 1.8 x 2.0 modelled -> 8.4 m wide, 6.5 m to the ridge
  farm_building: 5,                                    // 3.7 x 2.0 x 2.1 -> an 18 m barn
  watchtower_s: 3.7,                                   // 3.8 tall -> 14 m
  crop_field: [U, 3, U], hay_bales: [U, 3, U],         // crops and bales ~1 m, not 2.6 m
  fence: [7.5, 2.3, 3],                                // 1.33 long segment -> 10 m (one node), 1.2 m high
  bank_stones: 3,                                      // 1.5 m long clusters become 4.6 m
  tuft_a: 3, tuft_b: 3,                                // 0.25 m grass becomes ~0.7 m
  bridge_3: [3.5, 3.5, U],                             // 4.5 m wide deck; the length (U per modelled unit) is then stretched to the river by the field (sz)
};
const BRIDGE = [3.5, 3.5, U];

/** [x, y, z] multipliers for a library piece. */
export function assetScale(name) {
  const s = TABLE[name] ?? (name.startsWith('bridge_') ? BRIDGE : U);
  return Array.isArray(s) ? s : [s, s, s];
}
