<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 Keldrion, LLC and contributors -->

# Kaimeter logo

`logo.svg` is the project mark: a rising growth curve over the metering rings,
inside a circular clip. Growth measured — which is what the product is.

## Palette

| Colour    | Value     | Use                         |
| --------- | --------- | --------------------------- |
| Deep pine | `#052E16` | Rings, stems, bud           |
| Green     | `#22C55E` | Growth curve, arrow, ground |
| Mint      | `#BBF7D0` | Highlight on the bud        |

## Sizing

**The detailed mark needs roughly 48px or more.** The dashed rings, the bud's
highlight and the small arrow all fall below a pixel at 32px and the mark reads
as a grey smudge. This is not a bug to fix by redrawing: fine detail is the
point of the artwork at large sizes.

For small placements use the simplified variant instead, which the UI already
does — `MarkSimple` in `web/src/components/logo.jsx` keeps the curve, the bud and
a solid disc and drops everything that cannot survive the scale. `Mark` is the
detailed artwork above.

## Usage

- Clear space: keep a quarter of the mark's width free on every side.
- Do not rotate, outline, or add shadow, and do not recolour the three tones —
  the greens carry the meaning.
