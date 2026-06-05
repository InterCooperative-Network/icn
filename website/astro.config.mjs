// @ts-check
import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

// https://astro.build/config
export default defineConfig({
  output: "static",
  integrations: [sitemap()],
  site: "https://intercooperative.network",
  base: "",
  // Legacy routes from the pre-Astro site structure that external parties
  // still link to / have bookmarked (reported 404 by an outside developer,
  // Dec 2025). Static output renders these as meta-refresh redirect pages.
  redirects: {
    "/docs/cooperatives/getting-started": "/for-cooperatives",
    "/community/cooperatives": "/community",
  },
  markdown: {
    shikiConfig: {
      theme: "github-dark",
    },
  },
});
