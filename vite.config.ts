import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import path from 'node:path'

// `BUDDY_UI_LOCAL=1` points @buddy/ui at the sibling clone instead of the
// tag-pinned git install. An alias rather than `npm link` for two reasons: HMR
// crosses the boundary, because Vite treats the aliased path as source rather
// than a built package; and it survives `npm ci`, which wipes node_modules and
// any symlink in it.
//
// `npm run typecheck` needs tsconfig.local.json to match, or tsc resolves the
// published .d.ts while Vite resolves the working copy and the two disagree in
// silence.
const localUi = process.env.BUDDY_UI_LOCAL
  ? { '@buddy/ui': path.resolve(__dirname, '../buddy-ui/src') }
  : {}

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  resolve: { alias: localUi },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
    server: {
      deps: {
        // Vitest leaves node_modules untransformed, so a `vi.mock` of
        // `@tauri-apps/api/*` would not reach inside @buddy/ui and its
        // components would call the real bridge — which in jsdom returns
        // nothing, so a row that needs a notch layout renders null and the
        // test fails with no clue why. Inlining puts the package in the
        // transformed graph, where the mocks apply.
        inline: ['@buddy/ui'],
      },
    },
  },
})
