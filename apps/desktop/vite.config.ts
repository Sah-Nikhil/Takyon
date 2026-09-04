import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/**
 * The version About shows. Read from `package.json` rather than hard-coded,
 * because `bun run bump` already moves that one and a second copy would drift.
 */
const { version } = JSON.parse(
  readFileSync(fileURLToPath(new URL("./package.json", import.meta.url)), "utf8"),
) as { version: string };

// `@types/bun` at the workspace root already declares `process`, so no
// `@ts-expect-error` is needed here (unlike Tauri's scaffold, which assumes it is
// absent).
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],

  define: { __APP_VERSION__: JSON.stringify(version) },

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
