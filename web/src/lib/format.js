// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Display helpers for reference data.
 *
 * CN codes and countries are stored as identifiers and localized through
 * namespaced dictionary keys (`cn.72083800`, `country.CN`). A missing entry
 * falls back to the identifier itself, so an unseeded code still displays.
 */

import { lookup } from './i18n';

export function cnLabel(locale, code) {
  return lookup(locale, `cn.${code}`);
}

export function countryLabel(locale, iso) {
  return lookup(locale, `country.${iso}`);
}

/** `{n}` in mass strings is tonnes; keep one place for the formatting rule. */
export function tonnes(value) {
  const n = Number(value ?? 0);
  return n.toLocaleString(undefined, { maximumFractionDigits: 3 });
}
