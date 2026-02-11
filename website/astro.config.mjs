// @ts-check
import { defineConfig } from "astro/config";

import react from "@astrojs/react";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import vercel from "@astrojs/vercel";

// https://astro.build/config
export default defineConfig({
  site: "https://fluxdown.zerx.dev",
  adapter: vercel(),
  integrations: [react(), sitemap()],

  vite: {
    plugins: [tailwindcss()],
  },
});
