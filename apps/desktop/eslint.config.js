import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  { ignores: ["dist", "node_modules", "src-tauri", "playwright-report", "test-results"] },
  ...tseslint.configs.recommended,
  {
    // A leading underscore is the conventional "deliberately unused" marker, and
    // it is load-bearing in `api.mock.ts`: the mock has to keep the real seam's
    // signature exactly, or the two drift and the visual layer starts testing a
    // different API than the one that ships.
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  // `configs.flat.*`, not `configs.*`. The top-level entries are still eslintrc
  // shaped (`plugins` as an array of strings) and ESLint 9 rejects them outright.
  reactHooks.configs.flat["recommended-latest"],
  {
    // ADR-0009: `api.ts` is the ONE file allowed to touch the Tauri bridge. The
    // moment a component imports `invoke` directly, it stops running in the plain
    // Vite dev server and TBC-0007's whole visual layer rots. This rule is the
    // only thing that keeps that honest without a code review every time.
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/api.ts", "src/api.mock.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@tauri-apps/*"],
              message:
                "Only src/api.ts may call into Tauri (ADR-0009). Add a wrapper there and import that.",
            },
          ],
        },
      ],
    },
  },
);
