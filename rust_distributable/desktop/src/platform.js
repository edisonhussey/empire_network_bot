/// Everything that differs between the Tauri window and a plain browser tab, in one place.
///
/// The UI itself is the same in both: it only talks to the daemon over HTTP (api.js). The daemon owns
/// every bot, so closing the window or the tab never stops anything. Anything host-specific (where the
/// API is, clipboard, notices, and later native dialogs) goes through here so the rest of the UI never
/// checks which host it runs in.

const DEFAULT_API = "http://127.0.0.1:47821/v1";
const VITE_DEV_PORT = "1420";

/// Tauri 2 injects this object into its webview; a browser tab never has it.
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/// Where the daemon's `/v1` API is.
/// * `VITE_EMPIRE_API` (build or dev time) wins, for a daemon on another port.
/// * The Tauri window and the Vite dev server (:1420) talk to the daemon on its fixed port.
/// * A page the daemon served itself (http://127.0.0.1:47821/) uses its own origin.
export function apiBase(location = globalThis.location, env = import.meta.env ?? {}) {
  if (env.VITE_EMPIRE_API) return env.VITE_EMPIRE_API.replace(/\/$/, "");
  if (isTauri || !location || !location.protocol?.startsWith("http") || location.port === VITE_DEV_PORT) return DEFAULT_API;
  return `${location.origin}/v1`;
}

export async function copyText(text) {
  await navigator.clipboard.writeText(text);
}

export function notify(message) {
  alert(message);
}
