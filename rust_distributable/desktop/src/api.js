/// The only place the window talks to the service. Everything it shows is a
/// projection of what this API returns; nothing here decides business rules.
export const API = "http://127.0.0.1:47821/v1";

async function responseJson(response) {
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Request failed (${response.status})`);
  return body;
}

export async function api(path, options) {
  return responseJson(await fetch(`${API}${path}`, options));
}
