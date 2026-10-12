/// Commander ranges for an attack bot: which commanders each task owns.
///
/// An entry is `{ task_id, commander_count, commander_start }`. With no start (`null`)
/// a task follows the one before it; with one it sits at that commander number and any
/// numbers in between stay unused. The same rule the service applies when it picks a
/// task's commanders (`resolve_commander_ranges` in empire-core). No DOM: see
/// commander-ranges.test.js.

const count = (entry) => Math.max(1, Math.floor(entry.commander_count) || 1);

/// Each entry's inclusive range, in order. An entry is a task, or (`kind: "support"`)
/// commanders reserved for the bot's support object.
export function commanderRanges(entries) {
  const ranges = [];
  let next = 1;
  for (const entry of entries) {
    const size = count(entry);
    const first = entry.commander_start ?? next;
    ranges.push({ task_id: entry.task_id, kind: entry.kind || (entry.support ? "support" : "task"), first, last: first + size - 1, count: size, explicit: entry.commander_start != null });
    next = first + size;
  }
  return ranges;
}

/// Pairs of entries (by index) that share a commander, or start below 1.
export function problems(ranges) {
  const found = [];
  ranges.forEach((range, index) => {
    if (range.first < 1) found.push({ kind: "start", index });
    for (let other = 0; other < index; other += 1) {
      if (range.first <= ranges[other].last && ranges[other].first <= range.last) found.push({ kind: "overlap", index: other, with: index });
    }
  });
  return found;
}

/// The highest commander number any task reaches: how many commanders the bot needs.
export function needed(ranges) {
  return ranges.reduce((most, range) => Math.max(most, range.last), 0);
}

/// Set an entry's range from "from" and "to" numbers. The start becomes explicit; the
/// count is the span. A "to" below "from" is a span of one.
export function withRange(entry, from, to) {
  const first = Math.max(1, Math.floor(Number(from)) || 1);
  const last = Math.max(first, Math.floor(Number(to)) || first);
  return { ...entry, commander_start: first, commander_count: last - first + 1 };
}

/// An entry that follows the previous task again, keeping its size.
export function followPrevious(entry) {
  return { ...entry, commander_start: null };
}

/// What to send to the service: the start only when it was chosen; a support entry
/// has no task.
export function toRequest(entries) {
  return entries.map((entry) => ({
    task_id: entry.kind === "support" ? "" : entry.task_id,
    commander_count: count(entry),
    ...(entry.kind === "support" ? { support: true } : {}),
    ...(entry.commander_start != null ? { commander_start: entry.commander_start } : {}),
  }));
}

/// Slide the entry at `index` one block left or right along the numbers: it trades
/// places with its neighbour in number order, keeping each one's size and the gap
/// between them. Every start becomes explicit afterwards, so the board stays exactly as
/// it was drawn, and the list is re-sorted into number order. Unchanged at the ends.
export function moveBlock(entries, index, direction) {
  const ranges = commanderRanges(entries);
  const order = ranges.map((range, position) => ({ position, ...range })).sort((a, b) => a.first - b.first || a.position - b.position);
  const at = order.findIndex((item) => item.position === index);
  const other = at + direction;
  if (at < 0 || other < 0 || other >= order.length) return entries;
  const [left, right] = direction < 0 ? [order[other], order[at]] : [order[at], order[other]];
  const gap = right.first - left.last - 1;
  // `right` (the later block) now comes first.
  const moved = new Map();
  moved.set(right.position, left.first);
  moved.set(left.position, left.first + right.count + Math.max(0, gap));
  const starts = new Map(order.map((item) => [item.position, moved.get(item.position) ?? item.first]));
  return entries
    .map((entry, position) => ({ ...entry, commander_start: starts.get(position) }))
    .sort((a, b) => a.commander_start - b.commander_start);
}

/// Segments for the bar under the list: each range, and the unused numbers between
/// them, in number order.
export function barSegments(ranges) {
  const ordered = [...ranges].sort((a, b) => a.first - b.first);
  const segments = [];
  let cursor = 1;
  for (const range of ordered) {
    if (range.first > cursor) segments.push({ kind: "gap", first: cursor, last: range.first - 1 });
    segments.push({ kind: "task", ...range });
    cursor = Math.max(cursor, range.last + 1);
  }
  return segments;
}
