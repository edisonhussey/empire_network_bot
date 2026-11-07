/// Pure parts of the Licence tab and the sidebar plan line: no DOM, no requests.

/// What the sidebar says about the account in view. An expired or missing licence says so
/// instead of a countdown taken from some other account's licence.
export function planLabel(data, now = Date.now()) {
  const licence = data?.licence;
  if (!licence) return "No licence";
  if (!licence.valid) return licence.reason === "licence has expired" ? "Expired" : "Unusable";
  const remaining = Math.max(0, licence.expires_at * 1000 - now);
  if (remaining <= 0) return "Expired";
  const days = Math.floor(remaining / 86400000);
  const hours = Math.floor((remaining % 86400000) / 3600000);
  return days > 0 ? `${days}d ${hours}h remaining` : `${hours}h remaining`;
}

/// One line on where the licence stands for this account.
export function licenceState(data) {
  const licence = data?.licence;
  const castle = data?.main_castle ? ` with its main castle at ${data.main_castle[0]}:${data.main_castle[1]}` : "";
  if (!licence) return `This account has no licence. Drop one issued for ${data?.server || "its server"}${castle}.`;
  if (!licence.valid) return `This account's licence is unusable (${licence.reason || "invalid"}). Drop a new one to replace it.`;
  if (licence.stage === "unactivated") return "Installed. It is bound to this account at its next login.";
  return "Active for this account.";
}

/// A token is the text of a `.token` file or a paste: `OA2.<payload>.<signature>`.
export function tokenFrom(text) {
  const token = String(text || "").trim().split(/\s+/).find((part) => part.startsWith("OA2."));
  return token || "";
}
