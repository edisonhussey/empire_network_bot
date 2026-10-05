/// Pure display formatting. No DOM, no application state: every function maps
/// its arguments to a string, so they can be reasoned about in isolation.

export function expiryLabel(epoch) {
  if (!epoch) return "Token required";
  const remaining = Math.max(0, epoch * 1000 - Date.now());
  const days = Math.floor(remaining / 86400000);
  const hours = Math.floor((remaining % 86400000) / 3600000);
  return days > 0 ? `${days}d ${hours}h remaining` : `${hours}h remaining`;
}

export function compactNumber(value) {
  return new Intl.NumberFormat(undefined, { notation: Math.abs(value || 0) >= 10000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(value || 0);
}

export function humanize(value) {
  return value.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

/// How long a session has been up, as "2h30" or "45m".
export function sessionDuration(ms) {
  if (!ms) return "";
  const minutes = Math.floor((Date.now() - ms) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, "0")}`;
}

/// A short wait, for "clears in 12m" style copy.
export function humanWait(ms) {
  if (!ms || ms <= 0) return "now";
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

/// Age of a timestamp, for "last used 2h ago".
export function relativeTime(ms) {
  if (!ms) return "never";
  const seconds = Math.max(0, (Date.now() - ms) / 1000);
  if (seconds < 90) return "just now";
  const minutes = seconds / 60;
  if (minutes < 90) return `${Math.round(minutes)}m ago`;
  const hours = minutes / 60;
  if (hours < 36) return `${Math.round(hours)}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/// The cadence names are about request timing, so say that rather than echoing
/// the stored word.
export function tempoLabel(value) {
  const names = { greedy: "fast gaps", sporadic: "sporadic gaps", advanced: "randomised gaps" };
  return names[value] || humanize(value);
}
