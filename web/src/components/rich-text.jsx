// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Render a locale string that carries inline markup.
 *
 * Nine strings in `locales/*.json` are authored with `<b>` for emphasis (the
 * "plain words" explainers and the CBAM primer). They are trusted content from
 * the repository's own locale files — but a `locales/` directory on disk is
 * operator-supplied, so this parses only a whitelist of inline tags into React
 * elements and renders everything else as text. No `dangerouslySetInnerHTML`,
 * no attribute or script surface.
 */

import { createElement, Fragment } from 'react';

const ALLOWED = new Set(['b', 'strong', 'i', 'em', 'code']);

/** The five entities the locale files actually use. */
const ENTITIES = {
  '&amp;': '&',
  '&lt;': '<',
  '&gt;': '>',
  '&quot;': '"',
  '&#39;': "'",
  '&nbsp;': '\u00a0',
};

function decodeEntities(text) {
  return text.replace(/&(?:amp|lt|gt|quot|#39|nbsp);/g, (m) => ENTITIES[m] ?? m);
}

/** `text <b>bold</b> more` -> ['text ', <b>bold</b>, ' more'] */
function parse(text) {
  const parts = [];
  const pattern = /<(\/?)([a-zA-Z]+)>/g;
  const stack = [];
  let cursor = 0;
  let match;

  const push = (node) => {
    if (stack.length === 0) parts.push(node);
    else stack[stack.length - 1].children.push(node);
  };

  while ((match = pattern.exec(text)) !== null) {
    const [tag, closing, name] = match;
    if (!ALLOWED.has(name.toLowerCase())) continue; // drop unknown tags verbatim

    const literal = text.slice(cursor, match.index);
    if (literal) push(literal);
    cursor = match.index + tag.length;

    if (closing) {
      const open = stack.pop();
      if (open) push(createElement(open.name, { key: `${open.name}-${match.index}` }, ...open.children));
    } else {
      stack.push({ name: name.toLowerCase(), children: [] });
    }
  }

  const tail = text.slice(cursor);
  if (tail) push(tail);

  // Unbalanced markup: emit whatever is still open rather than losing text.
  while (stack.length) {
    const open = stack.pop();
    parts.unshift(...open.children);
  }
  return parts;
}

export function RichText({ text }) {
  if (!text) return null;
  if (!/[<&]/.test(text)) return text;
  return createElement(Fragment, null, ...parse(decodeEntities(text)));
}

/** Strip inline markup, for places that need a plain string (aria, titles). */
export function plain(text) {
  return (text ?? '').replace(/<\/?[a-zA-Z]+>/g, '');
}
