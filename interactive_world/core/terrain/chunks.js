// Chunking: the map is cut into square blocks of cells, and each block's geometry is built independently from the grid. A chunk OWNS the
// connectors on its cells' east and south edges, so every interior edge is filled exactly once even across chunk borders (the east / south
// neighbour may live in the next chunk; reading it is fine, the grid is global data).
//
// Editing one cell changes its own top, its own east/south connectors, and the connectors its west and north neighbours own, so the chunks to
// rebuild are the cell's chunk plus, at a chunk border, the chunk to the west / north (chunksTouchedBy).
import { buildTopSurfaces, buildCellOutlines } from './mesh_top.js';
import { buildConnectors, buildSkirt } from './mesh_connectors.js';

export const CHUNK_CELLS = 20;   // 20 x 20 cells = 100 m x 100 m; a 100 x 100 map is 5 x 5 chunks

/** Every chunk of the grid: [{ key, cx, cz, range: { x0, z0, x1, z1 } }]. */
export function chunkLayout(grid, size = CHUNK_CELLS) {
  const out = [];
  for (let cz = 0; cz * size < grid.depth; cz++) {
    for (let cx = 0; cx * size < grid.width; cx++) {
      out.push({ key: `${cx},${cz}`, cx, cz, range: { x0: cx * size, z0: cz * size, x1: Math.min(grid.width, (cx + 1) * size), z1: Math.min(grid.depth, (cz + 1) * size) } });
    }
  }
  return out;
}

/** All geometry of one chunk as typed arrays: { top, connectors, skirt, outlines }. `base` is the skirt's bottom height. */
export function buildChunk(grid, range, { base, outlines = true } = {}) {
  return {
    top: buildTopSurfaces(grid, range),
    connectors: buildConnectors(grid, range),
    skirt: buildSkirt(grid, range, base),
    outlines: outlines ? buildCellOutlines(grid, range) : null,
  };
}

/** Keys of the chunks whose geometry depends on cell (cx, cz). */
export function chunksTouchedBy(cx, cz, size = CHUNK_CELLS) {
  const keys = new Set([`${Math.floor(cx / size)},${Math.floor(cz / size)}`]);
  if (cx > 0) keys.add(`${Math.floor((cx - 1) / size)},${Math.floor(cz / size)}`);   // the west neighbour owns the edge between us
  if (cz > 0) keys.add(`${Math.floor(cx / size)},${Math.floor((cz - 1) / size)}`);   // the north neighbour owns the edge between us
  return [...keys];
}
