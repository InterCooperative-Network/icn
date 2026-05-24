/**
 * Dev-only mock auth endpoint.
 *
 * Sets a session cookie for an arbitrary GitHub login. Disabled when
 * NODE_ENV=production.
 *
 * Usage: /api/auth/mock?login=fahertym&next=/inside
 */
import type { APIRoute } from "astro";
import { makeSetCookie } from "../../../lib/session";

export const GET: APIRoute = ({ request, url }) => {
  if (process.env.NODE_ENV === "production") {
    return new Response("disabled in production", { status: 404 });
  }
  const login = url.searchParams.get("login");
  if (!login) {
    return new Response("missing ?login", { status: 400 });
  }
  const next = url.searchParams.get("next") ?? "/inside";
  const secure = url.protocol === "https:";
  // For dev convenience the mock asserts org membership = true.
  const cookie = makeSetCookie(login, true, secure);
  return new Response(null, {
    status: 302,
    headers: { Location: next, "Set-Cookie": cookie },
  });
};
