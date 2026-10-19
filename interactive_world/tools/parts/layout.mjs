// The castle's plan. One place for the numbers every part shares (units follow the sheet: footprint 2.5 x 2.0, +z front, +x right).
export const H = {
  floor: 0.5,      // courtyard and terrace floor above the ground
  parapet: 0.82,   // top of the terrace walls
  merlon: 0.97,    // top of the crenellations
  eave: 1.1,       // hall eaves
  ridge: 1.56,     // hall ridge
  keepTop: 1.86,
  towerTop: 1.98,
};
export const HALL = { x0: 0.0, x1: 2.16, z0: 0.1, z1: 0.82 };
export const KEEP = { x0: 0.64, x1: 1.34, z0: 0.0, z1: 0.52 };
export const WEST = { x0: 0.0, x1: 0.46, z0: 0.82, z1: 2.0 };       // west rampart (the long left wall, canopy walk beside it)
export const FRONT = { x0: 0.46, x1: 1.86, z0: 1.68, z1: 2.0 };     // front terrace wall (door bay in it)
export const COURT = { x0: 0.46, x1: 1.62, z0: 0.82, z1: 1.68 };
export const TOWER = { cx: 2.04, cz: 1.04, r: 0.43 };
export const DORMERS = [0.9, 1.56];   // x centres of the two cross gables on the hall's front slope
export const DOOR_X = 1.12;           // x centre of the front door bay
