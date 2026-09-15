# Parameter tables

JSON data tables consumed by `kaimeter-rules`. Every row carries its own
provenance — legal citation, source URL and retrieval date — so a value can
be checked without trusting the code that uses it (whitepaper §4.2, §9.1).

| File | Content | Legal basis |
| --- | --- | --- |
| `gwp.json` | Global warming potentials (CF₄, C₂F₆) | IR (EU) 2025/2547, Annex II, section G, Table 6 |
| `default-values-annex-I.json` | Aluminium default values per country and CN code, plus the `Other Countries and Territories` fallback table | IR (EU) 2025/2621, Annex I, as corrected by IR (EU) 2026/1740 |
| `default-values-annex-IV.json` | Highest default values for aluminium precursors of unknown origin | IR (EU) 2025/2621, Annex IV, as corrected by IR (EU) 2026/1740 |

## Scope

Bundle `2026.2.0` covers the aluminium sector only. The default-value tables
therefore carry the aluminium CN codes; other sectors, and the Annex II and
Annex III tables for indirect emissions and electricity, arrive with their
sectors. `default-values-annex-IV.json` is included because unknown-origin
precursors need it as soon as a complex good is evaluated.

## Regenerating the default values

`tools/extract-default-values.py` (Python 3 standard library only) reads the
Commission's definitive-period workbook and writes the two Annex tables. It
refuses to run unless the workbook matches `--expect-sha256`, so the tables
always name the reviewed file.

```sh
python tools/extract-default-values.py path/to/workbook.xlsx parameters \
    --expect-sha256 900583811c7e1194799eb9bdbad2d6d7e1100f5a7d80a664c1584a8fce6f9f35 \
    --retrieved 2026-09-16 \
    --source-url "https://taxation-customs.ec.europa.eu/document/download/1c05d211-80cb-4aaa-8ef0-e08005a95d7e_en?filename=DV%20correcting%20act_final%20update_06.08.xlsx"
```

The workbook was published on 10 August 2026 and retrieved on 2026-09-16:
default values for the definitive period, in Excel format, from the
Commission's CBAM legislation and guidance page. The Commission's own
disclaimer applies: the file is informational and the legally binding values
are those in the Regulation.

The `Other Countries and Territories` fallback and the Annex IV highest
values are used per the selection rules in [Guidance No. 3](https://taxation-customs.ec.europa.eu/document/download/29b9eec7-1a4b-4eb6-ab85-96a0c9e35fd0_en?filename=Guidance%20No.%203%20-%20CBAM%20methods%20for%20the%20calculation%20of%20emissions%20embedded%20in%20goods.pdf),
section 4.10: a good that is not listed for its country of production, or
whose country row shows `-`, falls back to the other-countries table; a
precursor of unknown origin uses the Annex IV highest value.
