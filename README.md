# Welcome to Kaimeter

简体中文：[README.zh-CN.md](README.zh-CN.md)

## What's Kaimeter?

Kaimeter is an open-source compliance toolkit for the EU's Carbon Border
Adjustment Mechanism (CBAM). It is built and maintained by Keldrion, LLC.

From January 1st, 2026, the EU charges for the carbon embedded in imported cement,
iron & steel, aluminium, fertilisers, electricity, and hydrogen — with
downstream goods next. The importer (the **declarant**) files an annual CBAM
declaration and surrenders certificates: the first declaration is due
**September 30th, 2027** for calendar-year 2026 imports, and certificates first go on sale
**February 1st, 2027**. The data the declarant must report lives upstream, at the
mill — that is the gap Kaimeter closes.

The core is open source so anyone can audit the math — the default-value
tables, the discount rules, and the certificate-cost formula — before trusting
a filing to it.

## Who it's for

Kaimeter is made for the three roles in a CBAM chain:

- **Import teams (the declarants)** — the declaration, the certificates, and
  the penalties are theirs alone. Kaimeter captures one record per
  consignment, watches mass against the 50-tonne line, computes embedded
  emissions on the actual or default basis (mark-ups included), projects
  certificate cost against the ETS price, assembles the verifier's dossier,
  and exports the declaration-ready file. Because the filing is theirs, the
  math must be checkable — the core is open source for exactly that.
- **Trading companies** — they file nothing, but every buyer demands the
  data with every order, and the data passes through the trader. Kaimeter is
  the data product: requests sent to mills in the mills' own language,
  responses checked against each order, packaged per buyer in the
  declaration format. Because the trader that delivers the data keeps the
  orders.
- **Mills and producers** — they hold the data everyone else needs, and
  supplying it should not require becoming compliance experts. Without the
  mill's actual data, the buyer files on default values and pays the
  penalty mark-up on top — +10% in 2026, +20% in 2027, +30% from 2028
  (Implementing Reg (EU) 2025/2621) — and that penalty lands in the landed
  price of the mill's goods, against competitors whose data is ready. With
  Kaimeter, the mill's data stays on its own machine, its dossier stays
  ready, every request is answered in one click, and nothing leaves until
  the mill approves — so its buyers pay on real emissions, not the penalty.

## What's in scope, and when

CBAM obligations are date-driven and they compound. If your goods are in
scope, the reporting must be filed through this tool or an alternative — the
dates do not negotiate.

| Industry / goods                                                                     | Obligation                                                                                                                                                                                                                                              | First declaration due                 |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| Iron & steel (basic goods)                                                           | In force since January 1st, 2026                                                                                                                                                                                                                        | September 30th, 2027                  |
| Aluminium (basic goods)                                                              | In force since January 1st, 2026                                                                                                                                                                                                                        | September 30th, 2027                  |
| Cement                                                                               | In force since January 1st, 2026                                                                                                                                                                                                                        | September 30th, 2027                  |
| Fertilisers                                                                          | In force since January 1st, 2026                                                                                                                                                                                                                        | September 30th, 2027                  |
| Electricity                                                                          | In force since January 1st, 2026                                                                                                                                                                                                                        | September 30th, 2027                  |
| Hydrogen                                                                             | In force since January 1st, 2026                                                                                                                                                                                                                        | September 30th, 2027                  |
| Downstream goods — ENVI position: ~457 product groups incl. solar panels, heat pumps | Proposed from **January 1st, 2028** (COM(2025)989 of December 17th, 2025; Council position agreed June 12th, 2026; Parliament ENVI voted July 6th, 2026; plenary vote September 2026; trilogue Q4 2026 — all three institutions support the 2028 start) | September 30th, 2029 (proposed cycle) |
| Other ETS sectors — glass, ceramics, paper & pulp, bulk chemicals                    | Under review (Art. 30 assessment); no application date adopted                                                                                                                                                                                          | —                                     |

Importers receiving goods in the categories above must declare them by
September 30th of the following year. CBAM tracks the calendar year —
January 1st through December 31st — and every CBAM deadline is Brussels
time (CET/CEST: UTC+1 in winter, UTC+2 in summer). The 50-tonne exemption applies to all CBAM goods **combined**: an
importer's total net mass across every in-scope product in a calendar year,
not per product code. Electricity and hydrogen get no exemption at all.

**Example 1 — below the exemption line.** A Rotterdam distributor imports
35 tonnes of aluminium profiles (CN code 7604) and 10 tonnes of steel
fittings (CN code 7307) during calendar year 2026 (January 1st –
December 31st) — 45 tonnes of CBAM goods in total, bought through a
Singapore trading house. 45 tonnes is under the 50-tonne line, so no CBAM
declaration is required at all — not by the distributor, not by the trader,
not by the producers (Reg (EU) 2025/2083). The only job: keep records that
prove the total — one bigger order next year crosses the line and changes
everything, because crossing it makes every tonne of the year reportable.

> **Who files what:** distributor — nothing. Trader — nothing. Producer —
> nothing. Just watch the line.

**Example 2 — above the line.** A Düsseldorf importer brings in 800 tonnes
of hot-rolled steel coil (CN code 7208) during calendar year 2026 — 120
shipments from a Chinese mill. Above 50 tonnes, the importer is fully in:
they must be an authorised declarant, keep data for every consignment, buy
CBAM certificates from February 1st, 2027 onward, and file the annual
declaration by September 30th, 2027. The emissions numbers in that
declaration come from the mill — the process route, the electricity mix,
the inputs. If the mill's numbers are verified by an EU-accredited verifier,
the importer pays on the real emissions. If not, the importer pays on
Brussels' default values — which are set conservatively and carry a
penalty mark-up on top: +10% in 2026, +20% in 2027, +30% from 2028
(Implementing Reg (EU) 2025/2621). Unverified data is the expensive option.

> **Who files what:** importer — the declaration, and buys the certificates.
> Mill — nothing, but every data request has to be answered. Trader —
> nothing, but the data has to pass through someone.

**Example 3 — the trader's position.** A Hong Kong trading company moves
2,000 tonnes of hot-rolled steel coil (CN code 7208) across four mills and
six European buyers during calendar year 2026. The trader files nothing —
but all six buyers must file, and each demands the emissions data with its
orders. The trader gathers the mill data, checks it, and attaches it to each
order.

The trader has no legal obligation to file. The commercial obligation lands
anyway: a buyer who cannot get the data places the next order with a trader
who can. For the trader, CBAM is cancellation risk — orders lost, not
certificates owed.

> **Who files what:** buyers — declarations. Trader — nothing; the data
> passes through the trader. Mills — nothing; they answer the data
> requests. The trader's product is the data, not the steel.

**Example 4 — the next wave.** An Italian manufacturer imports 120 tonnes
of steel components for heat pumps (a downstream product) from Vietnam.
As of August 2026: no CBAM obligation at all. The EU's proposal brings these
goods into
scope from January 1st, 2028 — if adopted, this manufacturer becomes a
declarant with the same obligations as Example 2, almost all of them brand
new to them. The importer who starts the data habit now enters 2028 already
running.

> **Who files what:** today — nobody. From 2028 (proposed) — the
> manufacturer files; the producer holds the data. Same shape as Example 2.

## What Kaimeter does

Kaimeter tracks imported mass against the 50-tonne exemption line — across
all CBAM goods combined, and it alerts the importer before the line is
crossed, because authorised-declarant status must be applied for in
advance. It captures
one record per consignment, and computes embedded emissions on either the
actual or default basis — mark-ups included. It projects certificate
cost against the weekly EU ETS price, prepares the dossiers verifiers want to
see, and exports declaration-ready files that match the official CBAM
registry format. It runs as a single-file local binary — a browser-based
interface on the user's own machine, no IT infrastructure, no server
configuration, no cloud account required. Kaimeter ships in English and Simplified Chinese at
launch — see [Internationalization](#internationalization) for the language
plan and how to contribute one.

### Built with

- **Core:** Rust — one static binary, no runtime dependencies
- **Storage:** embedded SQLite, data stays local — the same engine in
  every deployment lane

## Internationalization

Kaimeter maintains localized UI strings, exports, and regulatory terminology
via key-value JSON dictionaries stored in the `/locales` directory.
Standardized translation keys ensure strict semantic parity across all
supported languages for manufacturers, traders, and importers. The locale
assets for the supported languages are embedded in the executable at
compile time, so the single-file binary works out of the box; a `locales`
directory next to it (or `KAIMETER_LOCALES_DIR`) overrides the embedded
strings without a rebuild.

| Language            | Code    | Status      | File                                       |
| ------------------- | ------- | ----------- | ------------------------------------------ |
| English             | `en`    | Complete    | [`locales/en.json`](locales/en.json)       |
| French              | `fr`    | Planned     | —                                          |
| German              | `de`    | Planned     | —                                          |
| Italian             | `it`    | Planned     | —                                          |
| Japanese            | `ja`    | Planned     | —                                          |
| Korean              | `ko`    | Planned     | —                                          |
| Polish              | `pl`    | Planned     | —                                          |
| Simplified Chinese  | `zh-CN` | In Progress | [`locales/zh-CN.json`](locales/zh-CN.json) |
| Spanish             | `es`    | Planned     | —                                          |
| Traditional Chinese | `zh-TW` | Planned     | —                                          |
| Turkish             | `tr`    | Planned     | —                                          |
| Vietnamese          | `vi`    | Planned     | —                                          |

### Contributing Translations

To add a new language or update existing strings, submit a pull request with
the corresponding JSON schema in `locales/<code>.json`. See
[CONTRIBUTING.md](CONTRIBUTING.md) for key naming conventions and locale
structure guidelines.

## Getting Started

Download the latest single-file executable for your platform from
[Releases](https://github.com/kaimeter/kaimeter/releases). No runtime, no
installer, no cloud account.

```bash
./kaimeter # Windows: kaimeter.exe
# the interface opens at http://127.0.0.1:8080
```

Building from source:

```bash
git clone https://github.com/kaimeter/kaimeter.git
cd kaimeter
cargo build --release
```

## Contributing

We encourage you to contribute to Kaimeter. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the contribution rules: licence
headers, provenance and AI-assistance disclosure, and the review workflow.

Everyone interacting in Kaimeter's codebases, issue trackers, and chat
channels is expected to treat others with respect.

If you find a security vulnerability, report it privately to the maintainers
— do not open a public issue.

## License

Kaimeter is released under the [Apache License, Version 2.0](LICENSE).
Copyright 2026 Keldrion, LLC and contributors.
