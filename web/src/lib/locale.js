// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Locale state.
 *
 * Kept outside React so any module can read it, with a subscription the app
 * re-renders on. Deliberately tiny: the dictionaries themselves are injected by
 * the server, so there is no fetching, no loading state, and nothing to fail.
 */

const listeners = new Set();

let current = 'en';

/** Prefer the browser's language when the dictionaries carry a match. */
export function detectInitialLocale() {
  const codes = new Set(Object.keys(globalThis.__KAIMETER__ ?? {}));
  const nav = globalThis.navigator?.language ?? 'en';
  if (codes.has(nav)) return nav;
  const base = nav.split('-')[0];
  for (const code of codes) {
    if (code === nav || code.split('-')[0] === base) return code;
  }
  return codes.has('en') ? 'en' : [...codes][0] ?? 'en';
}

export function getLocale() {
  return current;
}

export function setLocale(next) {
  if (next === current) return;
  current = next;
  if (typeof document !== 'undefined') document.documentElement.lang = next;
  for (const fn of listeners) fn(next);
}

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}
