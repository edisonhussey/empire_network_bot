// Which kingdom is selected. Set it from the UI or from outside (an API call / server push): the map listens and frames it.
export class Focus {
  constructor(world) {
    this.world = world;
    this.kingdomId = null;
    this.listeners = new Set();
  }

  on(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** Select a kingdom by id (or null for none). Unknown ids are ignored. */
  selectKingdom(id) {
    if (id !== null && !this.world.kingdoms.has(id)) return false;
    this.kingdomId = id;
    for (const fn of this.listeners) fn(id);
    return true;
  }
}
