import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { changed, senderName, stickToBottom, timeLabel } from "./chat-model.js";

/// The Chat tab: the alliance messages the account in view has heard, with who said them and
/// when. Read-only. The service listens in the background (`direct/chat.rs`) and stores each
/// message as it arrives; this only reads them while the tab is open. Nothing is sent to the game.

const POLL_MS = 3000;

export function createChat() {
  const $ = (selector) => document.querySelector(selector);
  const state = { open: false, timer: 0, data: null, drawn: null, error: "" };

  function line(message, now) {
    const row = element("div", `chat-line${message.system ? " chat-system" : ""}`);
    row.append(
      element("span", "chat-time", timeLabel(message.sent_at_ms, now)),
      element("b", "chat-name", senderName(message)),
      element("span", "chat-text", message.text),
    );
    return row;
  }

  function render() {
    const list = $("#chat-list");
    const note = $("#chat-note");
    if (!list) return;
    if (!state.data) {
      note.textContent = state.error || "Loading…";
      return;
    }
    const { messages, connected } = state.data;
    note.textContent = state.error
      || (!connected ? "Connect the account to listen; messages heard earlier are shown."
        : messages.length ? "" : "Listening. Alliance messages appear here as they arrive.");
    $("#chat-count").textContent = messages.length ? `${messages.length} messages` : "–";
    if (!changed(state.drawn, messages)) return;
    const stick = stickToBottom(list.scrollTop, list.clientHeight, list.scrollHeight);
    const now = Date.now();
    list.replaceChildren(...messages.map((message) => line(message, now)));
    if (stick) list.scrollTop = list.scrollHeight;
    state.drawn = messages;
  }

  async function load() {
    try {
      state.data = await api(`/chat${accountContext.query()}`);
      state.error = "";
    } catch (error) {
      state.error = error.message;
    }
    render();
  }

  // Another account in view: its own alliance, drawn from scratch.
  accountContext.subscribe(() => {
    state.data = null; state.drawn = null; state.error = "";
    $("#chat-list")?.replaceChildren();
    if (state.open) load();
  });

  return {
    show() {
      if (state.open) return;
      state.open = true;
      state.drawn = null;
      load();
      state.timer = setInterval(load, POLL_MS);
    },
    hide() {
      state.open = false;
      clearInterval(state.timer);
    },
  };
}
