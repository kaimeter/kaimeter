// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Thin client for the binary's own `/api` surface.
 *
 * Two deployment modes, one code path (R22):
 *
 * - **Served by the binary** (http/https): every call reaches the Rust core, so
 *   the declaration export, audit chain and compliance math all see the real
 *   records.
 * - **Opened from `file://`** (the offline demo artifact): there is no server, so
 *   every call fails fast and the caller falls back to local state. Nothing
 *   throws, nothing blocks the UI, and nothing is sent anywhere.
 *
 * That is why every function here resolves to `{ ok, data }` instead of
 * throwing: an absent backend is a supported mode, not an error.
 */

const isServed = typeof location !== 'undefined' && location.protocol.startsWith('http');

async function call(path, init) {
  if (!isServed) return { ok: false, offline: true };
  try {
    const res = await fetch(`/api${path}`, {
      headers: { 'content-type': 'application/json' },
      ...init,
    });
    const body = await res.json().catch(() => null);
    if (!res.ok) return { ok: false, status: res.status, data: body };
    return { ok: true, data: body };
  } catch {
    // A dead server is the offline case, not a crash.
    return { ok: false, offline: true };
  }
}

const get = (path) => call(path);
const post = (path, body) =>
  call(path, { method: 'POST', body: JSON.stringify(body ?? {}) });

export const api = {
  /** True when a server is answering; false is the offline artifact. */
  served: isServed,

  reference: {
    cnCodes: () => get('/reference/cn-codes'),
    defaults: () => get('/reference/defaults'),
  },

  get: {
    /** The year is required: the exemption is a calendar-year total (R1). */
    deminimis: (year) => get(`/deminimis?year=${year}`),
    /**
     * Per-consignment net exposure (R7). The core refuses to guess a carbon
     * price, so a price must be supplied when the cache is cold.
     */
    exposure: (cnCode, year, etsPrice) => {
      const params = new URLSearchParams({ cn: cnCode, year: String(year) });
      if (etsPrice != null) params.set('ets_price', String(etsPrice));
      return get(`/exposure?${params}`);
    },
    calendar: (year) => get(`/calendar?year=${year}`),
    holding: (year) => get(`/holding?year=${year}`),
    audit: () => get('/audit'),
    price: () => get('/price'),
  },

  consignments: {
    /** Bulk import of a broker H1 / SAD export. */
    importSad: (payload) => post('/consignments/import-sad', payload),
  },

  exportDeclaration: (payload) => post('/export/declaration', payload),
  sealPack: (payload) => post('/pack/seal', payload),
  verifyPack: (payload) => post('/pack/verify', payload),
  attach: (payload) => post('/attachments', payload),
  setPrice: (payload) => post('/price', payload),
};
