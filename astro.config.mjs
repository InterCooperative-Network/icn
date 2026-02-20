// @ts-check
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

// https://astro.build/config
export default defineConfig({
  output: 'static',
  integrations: [sitemap()],
  site: 'https://intercooperative.network',
  base: '',
  markdown: {
    shikiConfig: {
      theme: 'github-dark',
    },
  },
});