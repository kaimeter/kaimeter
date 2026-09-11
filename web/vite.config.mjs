// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { copyFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));

/**
 * The wizard ships as ONE self-contained file: it must open from file:// with no
 * network and no external assets, and the Rust binary embeds it with
 * include_str!("../web/wizard.html").
 *
 * viteSingleFile inlines the JS and CSS. This plugin then moves the result to
 * web/wizard.html — the exact path include_str! and the Dockerfile already
 * reference — so the build output *is* the shipped artifact and nothing
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
      copyFileSync(resolve(scratch, 'index.html'), resolve(here, 'wizard.html'));
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
    alias: { '@': resolve(here, 'src') },
  },
  build: {
    cssCodeSplit: false,
    assetsInlineLimit: 100_000_000,
    target: 'es2020',
    rollupOptions: {
      output: {
        // Single chunk, so nothing is emitted as a separate file request.
        inlineDynamicImports: true,
      },
    },
  },
});
