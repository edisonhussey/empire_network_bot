// A hand-authored map in the same spec format as the generator: laid out after the "Medieval Strategy Map" sheet (a river snaking down the middle
// with wooden bridges where the roads cross, dirt roads between keeps, forests, farms with fenced fields, houses, watchtowers, stone nodes and
// cliffs). Coordinates are nodes on a 50 x 50 map (1 node = 10 m, so the river is 7.5 m wide and the roads 3 m); the kingdoms' castles come from the scenario that uses it.
export const SHEET_SPEC = {
  size: 50, seed: 4,
  rivers: [{ points: [[21, -5], [24, 5], [31, 11], [36, 19], [31, 27], [23, 33], [24, 40], [33, 46], [40, 55]], width: 0.75 }],
  roads: [
    { points: [[16.4, 4], [20, 6.5], [25, 9], [30, 13], [37, 14], [40.5, 11]], width: 0.3 },            // blue keep to the green keep, over the river
    { points: [[6, 14.3], [9, 19], [15, 23], [22.7, 25.6]], width: 0.3 },                                  // blue to the purple keep
    { points: [[21.5, 31.2], [17, 36], [12.5, 39.5], [10.8, 41]], width: 0.3 },                            // purple to the gold keep
    { points: [[27.8, 26.5], [32, 28.6], [37, 33], [41.5, 38], [41.5, 40.8]], width: 0.3 },               // purple to the red keep, over the river
    { points: [[45, 16.4], [44.2, 24], [44.2, 33.7]], width: 0.3 },                                        // green to red
    { points: [[37.7, 44.3], [34, 45], [31, 46.5]], width: 0.3 },                                          // red to its southern keep
    { points: [[5, 4.6], [7.4, 5.4]], width: 0.3 }, { points: [[8, 8.4], [6.5, 10.2]], width: 0.3 },        // inside the blue cluster
    { points: [[4.2, 37.2], [7.4, 39.7]], width: 0.3 }, { points: [[44.7, 40.2], [42.2, 42.2]], width: 0.3 },
  ],
  forests: [
    { shape: 'circle', x: 12, y: 19, r: 9, density: 0.85, mix: 0.55 }, { shape: 'circle', x: 38, y: 27, r: 9, density: 0.85, mix: 0.45 },
    { shape: 'circle', x: 17, y: 44, r: 7, density: 0.9, mix: 0.4 }, { shape: 'circle', x: 36, y: 5, r: 7, density: 0.85, mix: 0.5 },
    { shape: 'circle', x: 28, y: 41, r: 6, density: 0.8, mix: 0.6 }, { shape: 'circle', x: 8, y: 29, r: 6, density: 0.7, mix: 0.5 },
    { shape: 'circle', x: 20, y: 14, r: 6, density: 0.75, mix: 0.5 }, { shape: 'circle', x: 46, y: 24, r: 5, density: 0.8, mix: 0.4 },
  ],
  fields: [
    { x: 12, y: 8, w: 3, h: 2, rot: 0, kind: 'crop' }, { x: 15.4, y: 9, w: 2, h: 2, rot: 0, kind: 'hay' },
    { x: 37, y: 38, w: 3, h: 2, rot: 90, kind: 'crop' }, { x: 36, y: 41.4, w: 2, h: 2, rot: 90, kind: 'crop' },
    { x: 47, y: 9, w: 2, h: 2, rot: 0, kind: 'crop' },
  ],
  buildings: [
    { kind: 'farm_building', x: 12, y: 11.4, rot: 0 }, { kind: 'house_thatch', x: 33.4, y: 41.5, rot: 270 }, { kind: 'house_thatch', x: 22.5, y: 11, rot: 90 },
    { kind: 'house_tile', x: 34.5, y: 16.5, rot: 180 }, { kind: 'house_thatch', x: 13.5, y: 29.5, rot: 90 }, { kind: 'house_tile', x: 40.5, y: 30.5, rot: 270 },
    { kind: 'house_thatch', x: 26.5, y: 30, rot: 180 }, { kind: 'house_tile', x: 6, y: 21, rot: 0 }, { kind: 'house_thatch', x: 43, y: 21, rot: 270 },
    { kind: 'watchtower_s', x: 18, y: 21.5, rot: 0 }, { kind: 'watchtower_s', x: 32.5, y: 6, rot: 90 }, { kind: 'watchtower_s', x: 37, y: 23, rot: 180 },
    { kind: 'watchtower_s', x: 19.5, y: 47, rot: 0 }, { kind: 'watchtower_s', x: 7.5, y: 25, rot: 90 }, { kind: 'watchtower_s', x: 29, y: 35.5, rot: 0 },
  ],
  rocks: [
    { x: 20, y: 2, size: 2, variant: 0 }, { x: 30, y: 43, size: 3, variant: 1 }, { x: 5, y: 27, size: 2, variant: 1 }, { x: 47.5, y: 27, size: 1, variant: 0 },
    { x: 16, y: 14.5, size: 1, variant: 1 }, { x: 34, y: 23, size: 2, variant: 0 }, { x: 9.5, y: 34, size: 3, variant: 0 }, { x: 27.5, y: 4, size: 1, variant: 1 },
    { x: 41, y: 17, size: 1, variant: 0 }, { x: 13, y: 24, size: 2, variant: 1 }, { x: 48, y: 31, size: 2, variant: 1 },
  ],
  outcrops: [
    { kind: 'cliff', x: 15, y: 17, rot: 0 }, { kind: 'bump', x: 35.5, y: 12, rot: 30 }, { kind: 'slope', x: 30.5, y: 38.5, rot: 90 },
    { kind: 'cliff_tall', x: 42, y: 47, rot: 180 }, { kind: 'bump', x: 9, y: 32, rot: 0 }, { kind: 'cliff', x: 24, y: 17, rot: 90 },
  ],
};
