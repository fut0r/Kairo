// Display-only masking for the connection form. The Rust side is what parses
// URLs and keeps passwords out of everything it returns; this only hides the
// password in a field the user is not currently typing in.

export const MASK = "••••";

/** `postgres://user:secret@host/db` → `postgres://user:••••@host/db`. */
export function maskUrlPassword(url: string): string {
  const scheme = url.indexOf("://");
  if (scheme < 0) return url;

  const start = scheme + 3;
  const rest = url.slice(start);
  // Use the last `@`, so a password containing `@` or `/` is still hidden.
  const at = rest.lastIndexOf("@");
  if (at < 0) return url;

  const userinfo = rest.slice(0, at);
  const colon = userinfo.indexOf(":");
  if (colon < 0 || colon === userinfo.length - 1) return url;

  return `${url.slice(0, start)}${userinfo.slice(0, colon + 1)}${MASK}${rest.slice(at)}`;
}

export function urlHasPassword(url: string): boolean {
  return maskUrlPassword(url) !== url;
}
