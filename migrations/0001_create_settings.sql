-- SPDX-License-Identifier: Apache-2.0
-- Copyright 2026 Keldrion, LLC and contributors
-- Migration 0001: minimal settings table (key/value store for app defaults).

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
