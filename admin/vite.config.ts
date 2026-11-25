import { fileURLToPath, URL } from "node:url";

import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

// Die Bausteine liegen als Git-Submodul unter `vendor/weppelmann-ui`; die
// Aliasse zeigen auf ihre Quellen, damit kein zweiter Bau nötig ist.
const bausteine = fileURLToPath(
  new URL("./vendor/weppelmann-ui/packages/components/src", import.meta.url),
);
const icons = fileURLToPath(
  new URL("./vendor/weppelmann-ui/packages/icons/src", import.meta.url),
);

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      "@weppelmann/components": bausteine,
      "@weppelmann/icons": icons,
    },
  },
  server: {
    port: 5173,
    // Im Dev-Modus gehen `/api` **und** `/auth` an den Server; im Betrieb
    // liefert der Server `admin/dist` selbst aus (gleiche Herkunft, kein CORS).
    // `/auth` trägt die Anmeldung (Cookie, Microsoft-Weiterleitung,
    // Ersteinrichtung) und liegt auf derselben Wurzel wie die Seite.
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8740",
        changeOrigin: false,
      },
      "/auth": {
        target: "http://127.0.0.1:8740",
        changeOrigin: false,
      },
    },
    // Schriften liegen in `crates/oberflaeche/schriften` außerhalb von admin/.
    fs: { allow: [".."] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
