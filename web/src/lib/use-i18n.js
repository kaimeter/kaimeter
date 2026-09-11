// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { getLocale, setLocale, subscribe } from './locale';
import { interpolate, lookup, translator } from './i18n';

/** The active locale, re-rendering the caller when it changes. */
export function useLocale() {
  return useSyncExternalStore(subscribe, getLocale);
}

/**
 * The `t` function for the active locale.
 *
 * `t(key)` looks up a message; `t(key, vars)` also substitutes `{name}`
 * placeholders. Namespaced reference keys (`cn.72083800`, `country.CN`) go
 * through the same lookup, so call sites do not need a second helper.
 */
export function useT() {
  const locale = useLocale();
  return useMemo(() => {
    const base = translator(locale);
    return (key, vars) => interpolate(base(key), vars);
  }, [locale]);
}

/** Convenience for the common case of switching language. */
export function useLocaleSwitch() {
  return useCallback((next) => setLocale(next), []);
}

/** Look up a single key without building a `t` function. */
export function useKey(key) {
  const locale = useLocale();
  return lookup(locale, key);
}
