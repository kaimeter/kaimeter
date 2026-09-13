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
 *
 * The payloads and responses are typed from the Rust handlers themselves:
 * `cargo test --features ts` derives `web/.generated/*.ts` from the structs in
 * `src/http/api.rs`, so a call site that drifts from the wire contract fails
 * `npm run typecheck` instead of failing in front of a declarant.
 *
 * @typedef {import('@generated/DeminimisResponse').DeminimisResponse} DeminimisResponse
 * @typedef {import('@generated/ExposureResponse').ExposureResponse} ExposureResponse
 * @typedef {import('@generated/ExportBody').ExportBody} ExportBody
 * @typedef {import('@generated/ImportSadBody').ImportSadBody} ImportSadBody
 * @typedef {import('@generated/NewConsignmentBody').NewConsignmentBody} NewConsignmentBody
 * @typedef {import('@generated/PackSealBody').PackSealBody} PackSealBody
 * @typedef {import('@generated/PackSealResponse').PackSealResponse} PackSealResponse
 * @typedef {import('@generated/PackVerifyBody').PackVerifyBody} PackVerifyBody
 * @typedef {import('@generated/PriceBody').PriceBody} PriceBody
 * @typedef {import('@generated/PriceResponse').PriceResponse} PriceResponse
 */

/**
 * Every call resolves, never throws. On a rejection `data` is the core's own
 * error envelope (`{error: {key, message}}`), so a caller can surface it.
 *
 * Both variants carry every field, which keeps the union discriminated: the
 * compiler narrows on `ok`, so the failure branch knows `status`/`offline`
 * exist and the success branch knows `data` does.
 *
 * @template T
 * @typedef {{ ok: true, data: T } | { ok: false, offline: boolean, status: number, data: any }} ApiResult
 */

const isServed = typeof location !== 'undefined' && location.protocol.startsWith('http');

/**
 * @template T
 * @param {string} path
 * @param {RequestInit} [init]
 * @returns {Promise<ApiResult<T>>}
 */
async function call(path, init) {
  if (!isServed) return { ok: false, offline: true, status: 0, data: null };
  try {
    const res = await fetch(`/api${path}`, {
      headers: { 'content-type': 'application/json' },
      ...init,
    });
    const body = await res.json().catch(() => null);
    if (!res.ok) return { ok: false, offline: false, status: res.status, data: body };
    return { ok: true, data: body };
  } catch {
    // A dead server is the offline case, not a crash.
    return { ok: false, offline: true, status: 0, data: null };
  }
}

/**
 * @template T
 * @param {string} path
 * @returns {Promise<ApiResult<T>>}
 */
const get = (path) => call(path);

/**
 * @template T
 * @param {string} path
 * @param {unknown} [body]
 * @returns {Promise<ApiResult<T>>}
 */
const post = (path, body) => call(path, { method: 'POST', body: JSON.stringify(body ?? {}) });

/**
 * @template T
 * @param {string} path
 * @param {unknown} body
 * @returns {Promise<ApiResult<T>>}
 */
const put = (path, body) => call(path, { method: 'PUT', body: JSON.stringify(body ?? {}) });

export const api = {
  /** True when a server is answering; false is the offline artifact. */
  served: isServed,

  reference: {
    /** @returns {Promise<ApiResult<unknown>>} */
    cnCodes: () => get('/reference/cn-codes'),
    /** @returns {Promise<ApiResult<unknown>>} */
    defaults: () => get('/reference/defaults'),
  },

  get: {
    /**
     * The year is required: the exemption is a calendar-year total (R1).
     * @param {number} year
     * @returns {Promise<ApiResult<DeminimisResponse>>}
     */
    deminimis: (year) => get(`/deminimis?year=${year}`),
    /**
     * The year's exposure projection (R7). The core refuses to guess a carbon
     * price, so a price must be supplied when its cache is cold.
     * @param {number} year
     * @param {number} [etsPrice]
     * @returns {Promise<ApiResult<ExposureResponse>>}
     */
    exposure: (year, etsPrice) => {
      const params = new URLSearchParams({ year: String(year) });
      if (etsPrice != null) params.set('ets_price', String(etsPrice));
      return get(`/exposure?${params}`);
    },
    /** @returns {Promise<ApiResult<unknown>>} */
    calendar: (year) => get(`/calendar?year=${year}`),
    /** @returns {Promise<ApiResult<unknown>>} */
    holding: (year) => get(`/holding?year=${year}`),
    /** @returns {Promise<ApiResult<unknown>>} */
    audit: () => get('/audit'),
    /** @returns {Promise<ApiResult<PriceResponse>>} */
    price: () => get('/price'),
  },

  consignments: {
    /**
     * One consignment, validated by the core before it is stored.
     * @param {NewConsignmentBody} payload
     * @returns {Promise<ApiResult<unknown>>}
     */
    create: (payload) => post('/consignments', payload),
    /**
     * Bulk import of a broker H1 / SAD export: the document text itself, not
     * pre-parsed rows — the core owns the parser (R15).
     * @param {ImportSadBody} payload
     * @returns {Promise<ApiResult<unknown>>}
     */
    importSad: (payload) => post('/consignments/import-sad', payload),
  },

  /**
   * @param {ExportBody} payload
   * @returns {Promise<ApiResult<unknown>>}
   */
  exportDeclaration: (payload) => post('/export/declaration', payload),

  /**
   * @param {PackSealBody} payload
   * @returns {Promise<ApiResult<PackSealResponse>>}
   */
  sealPack: (payload) => post('/pack/seal', payload),

  /**
   * @param {PackVerifyBody} payload
   * @returns {Promise<ApiResult<unknown>>}
   */
  verifyPack: (payload) => post('/pack/verify', payload),

  /**
   * `PUT`, not `POST`: the route is `get(..).put(..)`.
   * @param {PriceBody} payload
   * @returns {Promise<ApiResult<unknown>>}
   */
  setPrice: (payload) => put('/price', payload),
};
