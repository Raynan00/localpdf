import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The UI is tiny and fully offline: no CDN, no web fonts, no analytics.
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: ["es2021", "chrome100", "safari15"],
    sourcemap: false,
  },
});
