import type { APIRoute } from "astro";
import { clearCookie } from "../../../lib/session";

export const GET: APIRoute = ({ url }) => {
  const secure = url.protocol === "https:";
  return new Response(null, {
    status: 302,
    headers: { Location: "/", "Set-Cookie": clearCookie(secure) },
  });
};
