import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

// `@types/bun` at the workspace root already declares `process`, so no
// `@ts-expect-error` is needed here (unlike Tauri's scaffold, which assumes it is
// absent).
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@takyon/shared": fileURLToPath(
        new URL("../../packages/shared/src/index.ts", import.meta.url),
      ),
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // Don't let Vite paint over Rust compiler errors.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
    // `packages/shared` is outside this app's root; Vite needs permission to serve it.
    fs: { allow: [fileURLToPath(new URL("../..", import.meta.url))] },
  },
});
