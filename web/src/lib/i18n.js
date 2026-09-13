// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Translation.
 *
 * Dictionaries come from two places, merged at read time:
 *
 * 1. **Injected by the server** into `window.__KAIMETER__` at startup, from the
 *    `locales/` directory. This is what makes an on-disk `locales/` directory
 *    re-localize the UI with no rebuild.
 * 2. **Generated into the bundle** from `locales/*.json` by
 *    `web/scripts/gen-locales.mjs`. This is the fallback for the `file://`
 *    artifact, where no server exists and nothing may be fetched (R22).
 *
 * Shape is `{ "<locale>": { "<key>": "<text>" } }`, `ui.` prefix already
 * stripped, matching `I18n::ui_dictionaries()`.
 */

import { FALLBACK_DICTIONARIES, FALLBACK_TERMS } from '@generated/locales';

/** Injected dictionaries win; the bundled ones fill the gaps. */
function dictionaries() {
  const injected = globalThis.__KAIMETER__ ?? {};
  const merged = { ...FALLBACK_DICTIONARIES };
  for (const [code, messages] of Object.entries(injected)) {
    merged[code] = { ...(merged[code] ?? {}), ...messages };
  }
  return merged;
}

/** Locale codes available, `en` first because it is canonical. */
export function localeCodes() {
  const codes = Object.keys(dictionaries());
  return codes.sort((a, b) => (a === 'en' ? -1 : b === 'en' ? 1 : a.localeCompare(b)));
}

export function lookup(locale, key) {
  const d = dictionaries();
  return d[locale]?.[key] ?? d.en?.[key] ?? key;
}

/**
 * Substitute `{name}` placeholders. Values come from the locale files, which are
 * authored content — rendered as text, never as markup.
 */
export function interpolate(text, vars) {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name) =>
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : whole,
  );
}

/** A `t` function bound to one locale. */
export function translator(locale) {
  return (key, vars) => interpolate(lookup(locale, key), vars);
}

/**
 * Terms from the locked compliance termbase. These are normative, so the UI must
 * not paraphrase them.
 */
export function term(locale, key) {
  const injected = globalThis.__KAIMETER_TERMS__ ?? {};
  const source = Object.keys(injected).length ? injected : FALLBACK_TERMS;
  return source[key]?.[locale] ?? source[key]?.en ?? key;
}
