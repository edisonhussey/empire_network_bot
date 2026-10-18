/// Pure display rules for the Chat tab: no DOM, no network (see chat-model.test.js).

/// "09:13" for today, "Oct 16 · 09:13" for another day. `timeZone` is for tests; the app
/// uses the machine's own.
export function timeLabel(ms, nowMs, timeZone) {
  const time = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false, timeZone });
  const day = new Intl.DateTimeFormat("en-GB", { year: "numeric", month: "numeric", day: "numeric", timeZone });
  if (day.format(ms) === day.format(nowMs)) return time.format(ms);
  const date = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", timeZone });
  return `${date.format(ms)} · ${time.format(ms)}`;
}

/// Who a line is from. A negative id is the game itself.
export function senderName(message) {
  if (message.system) return "Game";
  return message.player_name || `Player ${message.player_id}`;
}

/// Whether a refreshed list has anything new to draw: a new last message or a different count.
export function changed(previous, next) {
  if (!previous) return true;
  if (previous.length !== next.length) return true;
  return previous.at(-1)?.id !== next.at(-1)?.id;
}

/// A scrolled-to-the-bottom reader stays at the bottom when messages arrive; one who has
/// scrolled up to read is left where they are.
export function stickToBottom(scrollTop, clientHeight, scrollHeight, slack = 40) {
  return scrollHeight - (scrollTop + clientHeight) <= slack;
}
