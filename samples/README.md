# Sample documents

Test and demonstration inputs for the Kaimeter extraction wizard.
Every file's provenance is recorded below — Kaimeter's repo policy is that
nothing enters without a source and a license.

The three document classes map to CBAM's complete-dossier requirement
(spec R23): energy & fuel bills, raw-material invoices & mill test certs,
production output & customs records.

## energy-bills/

Chinese electricity bills — the primary input for Scope 2 (electricity) data.

| File                             | What it is                                                                                   | Source                                                                             | License                          |
| -------------------------------- | -------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | -------------------------------- |
| `csg-bill.png`                   | Real China Southern Power Grid (南方电网) bill image, 2665×2840                              | [Accurio/CSG-Bill-Reader](https://github.com/Accurio/CSG-Bill-Reader) (`bill.png`) | Apache-2.0 (repo license)        |
| `csg-statement-sample.txt`       | Synthetic 南方电网电费通知单 (operational statement) — realistic mock, no real customer data | Generated for Kaimeter from a mock template, August 2026                           | CC0 (Kaimeter-generated fixture) |
| `efapiao-electricity-sample.txt` | Synthetic VAT e-fapiao (数电发票) for electricity — realistic mock, no real customer data    | Generated for Kaimeter from a mock template, August 2026                           | CC0 (Kaimeter-generated fixture) |

## fuel-bills/

Natural gas / diesel / coal purchase documents — direct fuel combustion (part of Scope 1).

| File                     | What it is                                                                                    | Source                                                   | License                          |
| ------------------------ | --------------------------------------------------------------------------------------------- | -------------------------------------------------------- | -------------------------------- |
| `gas-invoice-sample.txt` | Synthetic 天然气 e-fapiao (natural gas, 9% VAT class) — realistic mock, no real customer data | Generated for Kaimeter from a mock template, August 2026 | CC0 (Kaimeter-generated fixture) |

## materials/

Raw-material invoices and mill test certificates (材质单) — upstream precursor inputs.

| File                          | What it is                                                                                                  | Source                                                   | License                          |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | -------------------------------- |
| `material-invoice-sample.txt` | Synthetic wire-rod (线材盘圆 Φ6.5mm Q235B) purchase invoice — realistic mock, no real customer data         | Generated for Kaimeter from a mock template, August 2026 | CC0 (Kaimeter-generated fixture) |
| `mill-test-cert-sample.txt`   | Synthetic steel product quality certificate (材质单, GB/T 700-2006) — realistic mock, no real customer data | Generated for Kaimeter from a mock template, August 2026 | CC0 (Kaimeter-generated fixture) |

## production/

Production output statements and customs records — the net-tonne denominator and CN codes.

| File                             | What it is                                                                                             | Source                                                   | License                          |
| -------------------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------- | -------------------------------- |
| `production-log-sample.txt`      | Synthetic monthly production statement (生产月报) — realistic mock, no real customer data              | Generated for Kaimeter from a mock template, August 2026 | CC0 (Kaimeter-generated fixture) |
| `customs-declaration-sample.txt` | Synthetic export customs declaration (出口货物报关单, HS 7318) — realistic mock, no real customer data | Generated for Kaimeter from a mock template, August 2026 | CC0 (Kaimeter-generated fixture) |

Notes:

- Synthetic files contain NO real customer, account, or invoice data — they are
  template mocks built to exercise the field extractor. Placeholder identifiers
  (XXXXXX) are intentional.
- Attribution for `csg-bill.png` is also recorded in [NOTICE](../NOTICE) where required.
- Real permissively-licensed samples for fuel bills, material certificates, and
  customs declarations are still being sourced (most public datasets on CN hosts
  have unclear licenses). Until they are cleared, the synthetic fixtures above
  are the parser's test corpus.
- Planned: a synthetic degradation suite (HTML/CSS templates rendered to images,
  then noise / tilt / fade / perspective warping applied) to stress-test offline
  OCR against wrinkled or rotated physical prints — generated with Jinja2,
  Playwright, and OpenCV, committed under this folder as they are produced.
