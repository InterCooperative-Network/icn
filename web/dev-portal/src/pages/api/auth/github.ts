/**
 * GitHub OAuth handler — stub.
 *
 * When `GITHUB_OAUTH_CLIENT_ID` is set, this redirects to GitHub's OAuth
 * authorize endpoint. The callback handler (not yet implemented) exchanges
 * the code, checks org membership, and sets the session cookie.
 *
 * Without `GITHUB_OAUTH_CLIENT_ID`, this returns a helpful message pointing
 * at the mock endpoint for dev.
 */
import type { APIRoute } from "astro";

export const GET: APIRoute = ({ url }) => {
  const clientId = process.env.GITHUB_OAUTH_CLIENT_ID;
  if (!clientId) {
    return new Response(
      `GitHub OAuth not configured. Set GITHUB_OAUTH_CLIENT_ID and GITHUB_OAUTH_CLIENT_SECRET, ` +
      `then register a callback at /api/auth/github/callback. ` +
      `In dev, use /api/auth/mock?login=YOUR_GH_LOGIN instead.`,
      { status: 501, headers: { "Content-Type": "text/plain" } }
    );
  }

  // OAuth dance: redirect to GitHub authorize with state + scope read:org.
  const next = url.searchParams.get("next") ?? "/inside";
  const redirectUri = `${url.origin}/api/auth/github/callback`;
  const state = Buffer.from(JSON.stringify({ next })).toString("base64url");
  const authorize = new URL("https://github.com/login/oauth/authorize");
  authorize.searchParams.set("client_id", clientId);
  authorize.searchParams.set("redirect_uri", redirectUri);
  authorize.searchParams.set("scope", "read:org");
  authorize.searchParams.set("state", state);

  return new Response(null, { status: 302, headers: { Location: authorize.toString() } });
};
