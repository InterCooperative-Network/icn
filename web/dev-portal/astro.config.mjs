// @ts-check
import { defineConfig } from "astro/config";
import node from "@astrojs/node";

// icn.zone — SSR mode because middleware needs to run per-request
// (shortlink resolution + session/scope checks).
export default defineConfig({
  site: "https://icn.zone",
  output: "server",
  adapter: node({ mode: "standalone" }),
  vite: {
    // Keep build deterministic for proof/audit-ability — see scope-doctrine.md.
    build: { sourcemap: false },
  },
});
