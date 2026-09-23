# Legal citations

Provision-by-provision mapping of the rule bundle to the instruments it
encodes. Each row corresponds to the `@legal` annotations on the code and to
the `legal` field carried by every parameter table row.

| Rule | Provision |
| --- | --- |
| `common::period::ReportingPeriod` | IR (EU) 2025/2547, Article 7 (identification of the reporting period) |
| `common::markups::markup` | IR (EU) 2025/2621, Article 4(2), as corrected by IR (EU) 2026/1740 |
| `common::defaults::default_value` | IR (EU) 2025/2621, Annex I, as corrected by IR (EU) 2026/1740; fallback rules per Commission Guidance No. 3, section 4.10 |
| `common::defaults::precursor_default_value` | IR (EU) 2025/2621, Annex IV, as corrected by IR (EU) 2026/1740 |
| `common::precursors::weighted_average` | IR (EU) 2025/2547, Articles 13-14; EU and excluded-origin zero-rating per Regulation (EU) 2023/956, Annex III |
| `common::precursors::see_complex` | Regulation (EU) 2023/956, Annex IV (complex goods); IR (EU) 2025/2547, Articles 13-14 |
| `common::scope::classify` | Regulation (EU) 2023/956, Annex I, as amended by Regulation (EU) 2025/2083 |
| `sectors::aluminium::pfc::slope` | IR (EU) 2025/2547, Annex II, sections B.7.1 (Equations 21-23) and B.7.3 (Equation 26) |
| `sectors::aluminium::pfc::overvoltage` | IR (EU) 2025/2547, Annex II, sections B.7.2 (Equations 24-25) and B.7.3 (Equation 26) |
| `sectors::aluminium::boundaries::see_primary_slope` | IR (EU) 2025/2547, Annex II, sections B and B.7 (system boundaries and PFC methods) |
| `sectors::aluminium::boundaries::see_primary_overvoltage` | IR (EU) 2025/2547, Annex II, sections B and B.7 (system boundaries and PFC methods) |
| `sectors::aluminium::boundaries::see_secondary` | IR (EU) 2025/2547, Annex I, point 3.17.2.2 (secondary melting); Regulation (EU) 2023/956, Annex IV |
| `rules.json` (`aluminium.primary.slope`) | IR (EU) 2025/2547, Annex II, sections B.7.1 and B.7.3 (Equations 21-23 and 26) |
| `rules.json` (`aluminium.primary.overvoltage`) | IR (EU) 2025/2547, Annex II, sections B.7.2 and B.7.3 (Equations 24-26) |
| `rules.json` (`aluminium.secondary.see`) | IR (EU) 2025/2547, Annex I, point 3.17.2.2 (secondary melting); Regulation (EU) 2023/956, Annex IV |
| `rules.json` (`common.complex.see`) | Regulation (EU) 2023/956, Annex IV (complex goods); IR (EU) 2025/2547, Articles 13-14 |
| `parameters/gwp.json` | IR (EU) 2025/2547, Annex II, section G, Table 6 |
| `parameters/default-values-annex-I.json` | IR (EU) 2025/2621, Annex I, as corrected by IR (EU) 2026/1740 |
| `parameters/default-values-annex-IV.json` | IR (EU) 2025/2621, Annex IV, as corrected by IR (EU) 2026/1740 |

The instruments are published on EUR-Lex under the ELI links recorded in the
`@source` annotations; the Commission's CBAM legislation and guidance page
carries the guidance documents and the default-values workbook from which the
parameter tables are generated.
