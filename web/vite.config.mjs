// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));

/**
 * Where the single-file artifact is written.
 *
 * `cargo build` runs this through `build.rs`, which points
 * KAIMETER_WIZARD_OUT at OUT_DIR — so a cargo build never writes a build output
 * into the source tree. A bare `npm run build` falls back to web/wizard.html
 * for hand-testing the artifact from file://.
 */
const outputFile = process.env.KAIMETER_WIZARD_OUT
  ? resolve(process.env.KAIMETER_WIZARD_OUT)
  : resolve(here, 'wizard.html');

/**
 * The wizard ships as ONE self-contained file: it must open from file:// with no
 * network and no external assets, and the Rust binary embeds it with
 * include_str!(concat!(env!("OUT_DIR"), "/wizard.html")).
 *
 * viteSingleFile inlines the JS and CSS. This plugin then moves the result to
 * `outputFile`, so the build output *is* the shipped artifact and nothing
 * downstream needs to know a bundler exists.
 *
 * Building into a scratch directory and copying at the end avoids writing the
 * output over the source file while Vite is still reading it.
 */
function emitWizard() {
  const scratch = resolve(here, '.vite-out');
  return {
    name: 'kaimeter-emit-wizard',
    config: () => ({
      build: { outDir: scratch, emptyOutDir: true },
    }),
    // writeBundle runs after Vite has flushed every asset; closeBundle can fire
    // before index.html exists on disk.
    writeBundle() {
      mkdirSync(dirname(outputFile), { recursive: true });
      copyFileSync(resolve(scratch, 'index.html'), outputFile);
    },
    closeBundle() {
      rmSync(scratch, { recursive: true, force: true });
    },
  };
}

export default defineConfig({
  root: here,
  base: './',
  plugins: [react(), tailwindcss(), viteSingleFile(), emitWizard()],
  resolve: {
    // The shadcn convention: components import from `@/lib/utils` etc.
    alias: {
      '@': resolve(here, 'src'),
      // Build outputs live outside src/ so a rebuild does not mutate the
      // watched source tree (see build.rs).
      '@generated': resolve(here, '.generated'),
    },
  },
  build: {
    cssCodeSplit: false,
    assetsInlineLimit: 100_000_000,
    target: 'es2020',
  },
});
