// The loopback address Google sends the browser back to after consent.
//
// One constant, because Google compares `redirect_uri` as an exact string: the value registered in
// the Cloud Console, the value stored on the OAuth client, and the value in the authorization
// request all have to be the same characters. The app used to suggest 127.0.0.1:3335 in the OAuth
// panel and 127.0.0.1:8765 in the first-run guide, so whichever one you registered, the other flow
// failed with "Access blocked / redirect_uri_mismatch".
//
// A "Desktop app" credential is the easier route — Google accepts any 127.0.0.1 port for those and
// nothing has to be registered at all. A "Web application" credential must list this exact string.
export const DEFAULT_OAUTH_REDIRECT = "http://127.0.0.1:3335";

/** True when a hand-entered redirect URI can work with the local callback server. */
export function isLoopbackRedirect(value) {
  try {
    const u = new URL(String(value || "").trim());
    return u.protocol === "http:" && ["127.0.0.1", "localhost"].includes(u.hostname);
  } catch { return false; }
}
