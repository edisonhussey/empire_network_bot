/// The only place the window talks to the service. Everything it shows is a
/// projection of what this API returns; nothing here decides business rules.
import { apiBase } from "./platform.js";

export const API = apiBase();

async function responseJson(response) {
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Request failed (${response.status})`);
  return body;
}

export async function api(path, options) {
  return responseJson(await fetch(`${API}${path}`, options));
}
