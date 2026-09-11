// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Screenshot a local file or URL with Playwright's Chromium.
 *
 * Why this exists: the wizard is a UI with no visual test, and the default
 * Chromium launch does not work in a container whose root filesystem is
 * read-only — crashpad cannot derive its `--database` and the process SIGTRAPs.
 * Playwright's own `chromium-headless-shell` build launches correctly, so this
 * is the supported way to look at the UI during development.
 *
 * Usage:
 *   node web/scripts/screenshot.mjs <file-or-url> <out.png> [width] [height]
 *
 * Examples:
 *   node web/scripts/screenshot.mjs wizard.html ../../.tmp/wizard.png
 *   node web/scripts/screenshot.mjs http://127.0.0.1:8080 .tmp/served.png 1440 900
 *
 * Setup, once per machine:
 *   npm install
 *   npx playwright install chromium
 *
 * On a container with a read-only HOME, point the browser cache somewhere
 * writable first:
 *   PLAYWRIGHT_BROWSERS_PATH=.toolchain/pw-browsers npx playwright install chromium
 */

import { chromium } from 'playwright';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Resolve relative paths against the repository root, not the current working
// directory: this script lives in web/, but `npm --prefix web run shot` runs
// with cwd=web/ while a direct `node web/scripts/screenshot.mjs` runs from the
// root. Both should accept the same arguments.
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

const [, , target, out, widthArg, heightArg] = process.argv;

if (!target || !out) {
  console.error('usage: npm run shot -- <file-or-url> <out.png> [width] [height]');
  process.exit(2);
}

const width = Number.parseInt(widthArg ?? '1440', 10);
const height = Number.parseInt(heightArg ?? '900', 10);

// A bare path is treated as a file (relative to the repo root); anything with a
// scheme is passed through untouched.
const url = /^[a-z][a-z0-9+.-]*:/i.test(target)
  ? target
  : pathToFileURL(resolve(repoRoot, target)).href;

if (url.startsWith('file:') && !existsSync(resolve(repoRoot, target))) {
  console.error(`no such file: ${target} (resolved against ${repoRoot})`);
  process.exit(2);
}

const outPath = resolve(repoRoot, out);

const browser = await chromium
  .launch({ args: ['--no-sandbox', '--disable-dev-shm-usage'] })
  .catch((error) => {
    console.error(`could not launch chromium: ${error.message.split('\n')[0]}`);
    console.error('run: npx playwright install chromium');
    process.exit(1);
  });

try {
  const page = await browser.newPage({ viewport: { width, height } });
  await page.goto(url, { waitUntil: 'load', timeout: 30_000 });
  await page.screenshot({ path: outPath, fullPage: true });
  console.log(`${out}  ${width}x${height}  ${url}`);
} finally {
  await browser.close();
}
