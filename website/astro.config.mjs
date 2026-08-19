// @ts-check
import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

// https://astro.build/config
export default defineConfig({
  output: "static",
  integrations: [sitemap()],
  site: "https://intercooperative.network",
  base: "",
  // Routes that external parties still link to or have bookmarked. Static
  // output renders these as meta-refresh redirect pages.
  redirects: {
    // Pre-Astro site structure (reported 404 by an outside developer, Dec 2025).
    "/docs/cooperatives/getting-started": "/for-cooperatives",

    // Merged during the #2608 information-architecture pass. Both pages
    // duplicated a job another page already had, and the duplication is what
    // let them drift:
    //
    //   /roadmap    described project state from a hand-maintained JSON file
    //               that had fallen a month behind docs/status.toml, while
    //               /whats-real-now described the same subject from prose.
    //               One surface for project state, generated from canonical
    //               state — see whats-real-now.astro.
    //
    //   /community  listed participation routes alongside /get-involved, and
    //               published repository counts (branch and merged-PR totals)
    //               whose freshness and meaning could not be defended. #1368
    //               rules out metrics we cannot stand behind.
    "/roadmap": "/whats-real-now",
    "/community": "/get-involved",
    "/community/cooperatives": "/for-cooperatives",
  },
  markdown: {
    shikiConfig: {
      theme: "github-dark",
    },
  },
});
