# Prove, Don't Disclose
## Verifiable CBAM Compliance Across Industrial Supply Chains Using Rules as Code and Zero-Knowledge Proofs

**Yiu Ming Patrick Ma**
Keldrion, LLC (Delaware, USA)
`[[email]]` · `[[https://keldrion.com]]`

**Version 1.1 — Working Paper — September 2026**
DOI: `[[10.5281/zenodo.XXXXXXX]]` · Reference implementation: `[[https://github.com/keldrion/…]]`
License: CC BY 4.0 (text) · Apache License 2.0 (code)

*Suggested citation:* Ma, Yiu Ming Patrick (2026). *Prove, Don't Disclose: Verifiable CBAM Compliance Across Industrial Supply Chains Using Rules as Code and Zero-Knowledge Proofs.* Keldrion Working Paper 2026-01. DOI `[[…]]`.

**Keywords:** carbon border adjustment mechanism, CBAM, rules as code, computational law, zero-knowledge proofs, embedded emissions, steel, aluminium, cement, fertilisers, downstream goods, supply-chain privacy, data sovereignty, open source

---

## Abstract

The European Union's Carbon Border Adjustment Mechanism (CBAM) entered its definitive period on 1 January 2026. From 2027, EU importers of cement, iron and steel, aluminium, fertilisers and hydrogen must surrender certificates for the emissions embedded in their imports; a legislative package now in trilogue would extend the mechanism from 2028 to several hundred downstream steel- and aluminium-containing products and commit the Commission to reviewing the product list annually. Embedded emissions must either be verified actual values supplied by the non-EU producer — and, for complex goods, by that producer's own suppliers — or punitive default values carrying a 10 %, 20 % and 30 % mark-up over 2026–2028.

This creates a *disclosure paradox*: importers are legally liable for numbers they cannot compute; producers hold the data but face commercial, competitive and legal reasons not to release it across borders; downstream manufacturers depend on upstream data they have no power to obtain; and accredited-verifier capacity is only now coming into existence.

This paper proposes an architecture that resolves the paradox by combining two techniques not previously applied together to border-carbon compliance. First, **rules as code**: the CBAM methodology (Regulation (EU) 2023/956 Annex IV, as implemented by Implementing Regulation (EU) 2025/2547, with default values, benchmarks and mark-ups) is encoded per sector as an executable, tested, versioned *rule bundle* whose cryptographic hash identifies exactly which rules were applied. Second, **zero-knowledge proofs**: a producer executes the codified methodology over its private activity data and attested inputs and produces a succinct proof that the declared figure is the correct output of the identified rule version — without revealing the data. Proofs compose recursively along the supply chain, so a manufacturer of a complex or downstream good can incorporate precursor emissions from installations it never sees.

We describe the rule-bundle and proof formats, the trust anchors connecting proofs to the accredited verification the Regulation requires, deployment models including fully air-gapped operation for producers in jurisdictions with cross-border data-transfer restrictions, and a worked scenario following metal from a smelter through an extruder and a downstream manufacturer to an EU declaration. Aluminium is used as the running example; the architecture is sector-agnostic by construction, and we set out how it applies to steel, cement, fertilisers, hydrogen and the proposed downstream scope. We are explicit that the approach **complements rather than replaces** accredited verification, and we state limitations, open questions and a roadmap. The reference implementation is open source under Apache 2.0.

---

## 1. Introduction: the disclosure paradox

CBAM is the first border-carbon regime to move from reporting to financial liability at scale. It converts the process emissions of a non-EU installation into a customs-adjacent cost borne by the EU importer, and it does so across sectors — cement, iron and steel, aluminium, fertilisers, hydrogen and electricity — that together account for a large share of the EU's imported industrial carbon.

The Regulation places the legal obligation on the **authorised CBAM declarant** (the importer or its indirect customs representative). The declarant must declare embedded emissions per tonne of goods and surrender a corresponding number of certificates. But the declarant does not operate the steel mill, the smelter, the kiln or the ammonia plant. The data required to compute actual embedded emissions belong to the **operator** of a third-country installation — and, for the majority of traded goods, to that operator's own upstream suppliers. The Regulation therefore contemplates that operators supply verified data to declarants, or declarants fall back on default values.

Three forces push against that data flow:

1. **Commercial sensitivity.** Installation-level activity data reveals cost structure, technology vintage, capacity utilisation and supplier relationships. An EU importer is frequently also a competitor, or supplies competitors. Within the supply chain, a downstream manufacturer asking its steel or aluminium supplier for process data is asking a counterparty with pricing leverage to hand over its cost base.
2. **Legal restriction.** Producers in China — the largest producer of steel, aluminium and cement, and the largest source of many of the downstream goods proposed for inclusion — operate under the Personal Information Protection Law, the Data Security Law and cross-border data-transfer rules that make releasing industrial operating data to a foreign counterparty a compliance question in its own right.
3. **Verification scarcity.** Actual values must be verified by an accredited verifier (Article 8). The accreditation framework (Delegated Regulation (EU) 2025/2551) was adopted in late 2025, verifier registration in the CBAM Registry opened during 2026, and capacity remains far short of the number of installations exporting to the EU.

The fallback — default values — is deliberately punitive. Under Implementing Regulation (EU) 2025/2621 (as corrected by IR 2026/1740), default values carry a mark-up of 10 % in 2026, 20 % in 2027 and 30 % from 2028. A producer whose real emissions are below the country default is penalised on every tonne for which it fails to demonstrate that fact.

The proposed downstream extension makes the paradox structural rather than incidental. A manufacturer of fasteners, wire, structural components or household articles emits little itself; nearly all of its embedded emissions are the emissions of the steel or aluminium it bought. Under the Commission's approach, its obligation reduces essentially to *mass of CBAM material in the product × precursor embedded emissions*. It therefore needs a number from a supplier it cannot compel — and, for lower tiers, cannot even identify.

The result is a market failure in information: the producer has a fact worth money to the importer, and neither the legal framework nor commercial trust currently allows that fact to travel with the goods.

This paper proposes to let the *fact* travel without the *data*.

### 1.1 Contributions

1. **Rules as code for CBAM, per sector.** We specify a versioned, hash-identified *rule bundle* that encodes the CBAM embedded-emissions methodology as executable code with legal citations attached to every parameter and branch, structured so that sector-specific methodology sits on shared cross-sector logic, and we show how rule changes — of which there have already been several in 2026 — and scope changes propagate deterministically (§4).
2. **Zero-knowledge compliance proofs bound to a rule version.** We define a proof statement in which the public inputs include the rule-bundle hash, so a verifier learns not only that "the number is right" but "right *according to which rules*" (§5).
3. **Recursive composition along the supply chain.** We show how a producer of complex or downstream goods incorporates the proven embedded emissions of precursors purchased from other installations without those installations disclosing anything beyond the proof — through as many tiers as the physical chain has (§6).
4. **A trust-anchor model that fits the legal framework.** We map proofs onto the roles the Regulation defines — operator, accredited verifier, declarant, Registry — and state what a proof does and does not establish (§7).
5. **Deployment for data sovereignty.** We describe an air-gapped deployment model, signed offline rule updates, and reproducible builds so that the proving software can be independently audited by the party running it (§8).
6. **An open reference implementation** with test vectors, published alongside this paper, with aluminium as the first implemented sector (§10).

### 1.2 Scope

The architecture addresses all goods listed in Annex I of the CBAM Regulation and any goods added to it. Sectors differ in methodology, and the differences are handled in the rule bundle rather than in the architecture:

| Sector | Indirect (electricity) emissions included? | Distinctive methodology features | Typical precursor depth |
|---|---|---|---|
| Iron and steel | No (Annex II) | Multiple routes (BF-BOF, DRI-EAF, scrap-EAF); pig iron, DRI and crude steel as precursors; pre-consumer scrap proposed for inclusion | Deep |
| Aluminium | No (Annex II) | Perfluorocarbons from anode effects; primary vs. secondary | Medium |
| Hydrogen | No (Annex II) | Route-specific (steam methane reforming, electrolysis) | Shallow |
| Cement | **Yes** | Clinker as precursor; calcination CO₂; electricity emission factors | Shallow, indirect-heavy |
| Fertilisers | **Yes** | N₂O from nitric acid; ammonia and nitric acid as precursors | Medium |
| Downstream goods (proposed, 2028) | Follows precursor | Emissions dominated by precursor content; anti-circumvention origin rules | Whatever the input chain is |

**Electricity** as a CBAM good is out of scope for this paper and the reference implementation: it is a grid-operator regime with its own default-factor logic, not a supply-chain problem.

**Aluminium is the running example** throughout because it is the most demanding single-installation case (PFC emissions require a distinct measurement method) and because it has a deep downstream chain that reaches into the proposed 2028 scope. Nothing in the architecture is specific to it.

### 1.3 Non-goals

We do not propose to change the methodology, replace accredited verification, or alter the allocation of legal liability. We do not address the UK CBAM or prospective US measures in detail, though §15 notes how the architecture generalises to them.

---

## 2. Background: the CBAM definitive regime

This section summarises the legal position as of September 2026. It is not legal advice; the instruments cited (listed with identifiers in Appendix C) are authoritative.

### 2.1 Timeline that matters for 2026 imports

- **1 January 2026** — definitive period begins; 2026 is the first reporting period for which certificates are owed.
- **1 February 2027** — Member States begin selling certificates on the common central platform; 2026-import certificates priced on 2026 quarterly average EU ETS prices.
- **From 2027, quarterly** — declarants must hold certificates covering at least 50 % of embedded emissions in goods imported year-to-date.
- **30 September 2027** — first annual CBAM declaration (for calendar 2026) and surrender of certificates.
- **1 November 2027** — unused 2026-import certificates cancelled without compensation.

### 2.2 The downstream extension

On 17 December 2025 the Commission proposed (COM(2025) 989) to extend CBAM from 1 January 2028 to roughly 180 downstream steel- and aluminium-intensive products, to bring pre-consumer scrap into scope, to strengthen anti-circumvention powers (including application of true-origin default values where "slight modification" patterns are found), and to require an annual Commission review adding further downstream goods. The Council adopted a general approach on 12 June 2026 covering roughly 200 products. Parliament's Environment Committee adopted its position on 6 July 2026 (56–11–12) covering roughly 457 CN codes, having lowered the emissions-intensity threshold used to select goods. Parliament's plenary mandate was scheduled for the September 2026 session `[[update with result]]`, with trilogue targeted to conclude by the end of 2026.

Whatever the final list, three features are settled across all three institutions and matter for this paper: downstream goods are coming; their embedded emissions will be computed from precursor content; and the scope will thereafter change annually.

### 2.3 What must be computed

For a good produced in a given installation and reporting period, the **specific embedded emissions** (SEE, in t CO₂e per tonne of good) are the attributed emissions of the production process — direct only for Annex II sectors, direct and indirect otherwise — divided by the activity level, plus, for *complex goods*, the embedded emissions of consumed *precursors*. IR 2025/2547 specifies, among other things:

- the reporting period (Art. 7);
- system boundaries per production route for each sector (Annex II);
- monitoring methods for direct emissions (calculation-based and measurement-based, following the EU ETS Monitoring and Reporting Regulation), and sector-specific methods such as the slope and overvoltage methods for PFCs;
- indirect emissions for cement and fertilisers, using grid or contract-specific electricity emission factors;
- precursor attribution, including weighted averaging when precursors under one CN code arrive from multiple installations or periods (Arts. 13–14);
- treatment of precursors of EU or excluded-territory origin (zero embedded emissions added).

The number of certificates to be surrendered follows from total embedded emissions, less the deduction for a carbon price effectively paid in the country of origin (Article 9, whose implementing rules remained pending at the time of writing), and less the adjustment for free allocation under the EU ETS (Article 31, using the benchmarks in IR 2025/2620).

### 2.4 Why the rules are a moving target

In the nine months since the definitive period began: default values were published, then corrected retroactively with a structural change to how mark-ups are computed (now within the Registry rather than tabulated); methodology guidance was reissued (Guidance Document 3, August 2026); verifier registration rules came into application; the Article 9 implementing act remained in draft; and the scope of the mechanism itself entered legislative revision. The Commission must revise default values and mark-ups by December 2027 and, under the proposal, review the product list every year thereafter. Any compliance system that hard-codes the rules — or the scope — will be wrong within a year. This motivates §4.

---

## 3. Problem statement and requirements

We seek a mechanism by which an EU declarant $D$ can obtain, for goods from installation $I$, a value $y$ (SEE, t CO₂e/t) such that:

- **R1 — Correctness.** $y$ equals the output of the CBAM methodology for $I$'s sector and production route applied to $I$'s true activity data for the relevant period.
- **R2 — Confidentiality.** $D$ learns nothing about $I$'s activity data beyond $y$ and what $y$ implies.
- **R3 — Rule identity.** $D$ (and any auditor or competent authority) can determine exactly which version of the methodology, default values, benchmarks and parameters produced $y$.
- **R4 — Anchoring.** $y$ is traceable to inputs the legal framework recognises — in particular, to the report of an accredited verifier where actual values are claimed.
- **R5 — Composability.** If $I$ produces complex or downstream goods from precursors supplied by installations $I_1 \dots I_k$, $I$ can incorporate their embedded emissions without $I_1 \dots I_k$ disclosing their data to $I$, and this holds recursively through any number of tiers.
- **R6 — Sovereignty.** $I$ can run the entire computation on infrastructure it controls, with no network egress, and can independently verify that the software it runs is the software whose source it has inspected.
- **R7 — Auditability.** Anyone can re-verify $y$'s proof cheaply, at any later date, against the rule version identified in R3.
- **R8 — Rule and scope agility.** When the rules or the product scope change, the change is published as a discrete, signed, reviewable artifact; existing proofs remain valid *with respect to the rules they cite*; and adding a sector or a CN code does not change the architecture.

No existing approach satisfies all eight. Data-sharing platforms (including the Registry's own operator module) satisfy R1 and R4 but fail R2, R5 and R6. Product carbon footprint exchange standards address interoperability but neither confidentiality nor rule identity. Prior proposals for zero-knowledge carbon claims address R2 but treat the computation as a fixed circuit for a single product, failing R3, R5 at depth, and R8, and do not engage with the legal roles in R4.

---

## 4. Rules as code: the methodology as a versioned executable artifact

### 4.1 Principle

Rules as code (RaC) is the practice of publishing legislation or regulation as machine-executable logic alongside — not instead of — the natural-language text, so that the logic can be tested, versioned and reused. The OECD's *Cracking the Code* (2020) and systems such as OpenFisca and Catala established the approach for tax and benefit law. We apply it to a technical emissions methodology, where it has three specific advantages: the methodology is already largely arithmetic; its inputs are already required to be monitored under a documented plan; and the consequences of rule drift are financial and immediate.

### 4.2 The rule bundle

A **rule bundle** $B$ is a directory of source code and data. Cross-sector logic lives once, in `common/`; each sector contributes only what is specific to it.

```
rules/
  cbam/
    bundle.json               # metadata: jurisdiction, period, legal basis, supersedes, changelog
    common/
      period.ts               # reporting-period determination (IR 2025/2547 Art. 7)
      precursors.ts           # attribution, weighted averaging, EU-origin zeroing (Arts. 13–14)
      defaults.ts             # default-value selection and mark-up (IR 2025/2621 as corrected)
      indirect.ts             # electricity emission factors, for sectors where indirect counts
      certificates.ts         # Art. 9 deduction, Art. 31 free-allocation adjustment
      scope.ts                # Annex I CN codes → sector/route mapping, with appliesFrom dates
    sectors/
      aluminium/
        boundaries.ts         # routes: primary, secondary, downstream processing
        pfc.ts                # slope and overvoltage methods
      steel/
        boundaries.ts         # routes: BF-BOF, DRI, EAF, rolling/finishing
        scrap.ts              # pre-consumer scrap treatment (proposed)
      cement/
        boundaries.ts         # clinker, cement, calcined clay
        calcination.ts
      fertilisers/
        boundaries.ts         # ammonia, nitric acid, urea, mixed fertilisers
        n2o.ts
      hydrogen/
        boundaries.ts
      downstream/
        content.ts            # precursor-content model for downstream goods (proposed, 2028)
        origin.ts             # anti-circumvention / true-origin rules (proposed)
    parameters/               # data tables, each with source URL and retrieval date
      gwp.json
      default-values-annex-I.json
      default-values-annex-IV.json
      benchmarks-2025-2620.json
      electricity-factors.json
    tests/                    # test vectors per sector, including Commission guidance examples
    CITATIONS.md              # provision-by-provision mapping to legal text
```

Every function carries a structured annotation linking it to the provision it implements:

```ts
/**
 * @legal IR (EU) 2025/2547, Annex II, point B.7.1 (slope method)
 * @source http://data.europa.eu/eli/reg_impl/2025/2547/oj
 * @since bundle 2026.1.0
 */
export function pfcSlopeMethod(input: SlopeInput, p: PfcParameters): PfcResult { … }
```

Adding a sector means adding a directory under `sectors/` and entries in `scope.ts`; adding a CN code to the downstream list means a row in `scope.ts` with an `appliesFrom` date and a legal citation. Neither touches `common/`, the proof system, or the envelope format of §9. This is the architectural form of requirement R8.

### 4.3 Version identity

The bundle is identified by a hash over its canonical serialisation:

$$
h_B = H\big(\, \text{canon}(B) \,\big)
$$

where $H$ is a collision-resistant hash (SHA-256 for the bundle; a proof-system-native hash such as Poseidon for the in-circuit commitment) and $\text{canon}$ is a deterministic serialisation that excludes non-semantic content. $h_B$ appears as a **public input in every proof** (§5). Two parties disagreeing about a result can immediately determine whether they are disagreeing about the *inputs* or about the *rules*.

Bundles are versioned semantically — `2026.1.0`, `2026.1.1` — and each bundle's metadata records what it supersedes and why, with the legal instrument that motivated the change. The July 2026 correction to default values is bundle `2026.2.0`, applying from 1 January 2026, superseding `2026.1.x`. When the downstream extension is adopted, its CN codes enter `scope.ts` with `appliesFrom: 2028-01-01` in a `2027.x` bundle — visible, reviewable and testable a year before they bite. A proof generated under an earlier bundle remains a valid proof of what the rules *were understood to be*; the operator can re-prove under the new bundle with the same private inputs, and the two proofs together document the change.

### 4.4 Determinism and arithmetic

Zero-knowledge proof systems operate over finite fields; the methodology is specified in decimal. We therefore fix:

- a **fixed-point scale** $s = 10^6$ for all quantities, i.e. every value $x$ is represented as the integer $\lfloor x \cdot s \rceil$;
- **rounding rules** at each step that mirror the Regulation's stated precision;
- an **evaluation order** defined by the bundle, not by the host language.

The reference implementation ships two evaluators — a plain TypeScript evaluator for everyday use and a provable evaluator for proof generation — and a **differential test suite** asserting they agree on every test vector and on randomly generated inputs. Independent third parties are invited to maintain their own evaluators against the same vectors.

### 4.5 Worked example: primary aluminium with PFC emissions

Consider a smelter using prebake cells, monitored under the slope method. Inputs for the reporting period:

| Symbol | Meaning | Illustrative value |
|---|---|---|
| $Pr_{Al}$ | Primary aluminium produced | 100 000 t |
| $E_{CO_2}$ | Direct CO₂ from anode consumption, fuels, flue-gas treatment (mass balance) | 155 000 t |
| $AEM$ | Anode-effect minutes per cell-day | 0.25 |
| $SEF_{CF_4}$ | Slope emission factor, (kg CF₄ / t Al) per (AE-min / cell-day) | 0.143 |
| $F_{C_2F_6}$ | Weight fraction C₂F₆ / CF₄ | 0.121 |
| $GWP_{CF_4}, GWP_{C_2F_6}$ | Global-warming potentials carried in the bundle (AR5 values as adopted in the EU ETS MRR: 6 630 and 11 100) | — |

$$
E_{CF_4} = AEM \times \frac{SEF_{CF_4}}{1000} \times Pr_{Al} = 0.25 \times \frac{0.143}{1000} \times 100\,000 = 3.575 \text{ t CF}_4
$$

$$
E_{C_2F_6} = E_{CF_4} \times F_{C_2F_6} = 3.575 \times 0.121 = 0.433 \text{ t C}_2\text{F}_6
$$

$$
E_{PFC} = E_{CF_4} \cdot GWP_{CF_4} + E_{C_2F_6} \cdot GWP_{C_2F_6} \approx 23\,702 + 4\,802 = 28\,504 \text{ t CO}_2\text{e}
$$

$$
SEE_{Al} = \frac{E_{CO_2} + E_{PFC}}{Pr_{Al}} = \frac{155\,000 + 28\,504}{100\,000} = 1.835 \text{ t CO}_2\text{e / t}
$$

*(Figures are illustrative, within typical ranges for modern prebake smelters; they are not measurements from any installation. Authoritative GWP values are those in the applicable Annex, carried in `gwp.json`.)*

Every line corresponds to a function in `sectors/aluminium/`, and each function's tests include this example. The same pattern — sector module computes attributed emissions; `common/` handles period, precursors, defaults and certificates — applies to a BF-BOF steel mill (`sectors/steel/boundaries.ts`, with pig iron and crude steel as intermediate precursors), a clinker kiln (`calcination.ts` plus `indirect.ts`), or a nitric-acid plant (`n2o.ts`).

### 4.6 Default values and mark-ups as rules

Default-value selection has several branches — country listed or not, value present or "–", precursor of unknown origin (Annex IV) — and the mark-up depends on the year of import:

$$
DV_{\text{applied}}(c, g, Y) = DV_{\text{total}}(c, g) \times \big(1 + m(Y)\big), \qquad m(2026)=0.10,\; m(2027)=0.20,\; m(Y \geq 2028)=0.30
$$

The proposed anti-circumvention rules add a further branch: where goods are found to have undergone only slight modification in an intermediate country, the default of the *true origin* applies. Encoding this as a rule with the history attached is exactly the case where a hand-maintained spreadsheet fails silently and a versioned bundle does not.

---

## 5. Zero-knowledge compliance proofs

### 5.1 What is proven

A zero-knowledge proof allows a prover to convince a verifier that a statement is true without revealing why. Here the statement is:

> *"I know private inputs $w$ such that running the rule bundle identified by $h_B$, for the sector and route identified in the context, on $w$ yields the public output $y$, and every input in $w$ that the bundle requires to be attested carries a valid attestation from a party whose key is committed in $c_A$."*

Formally, with public inputs $x = (h_B,\, y,\, c_A,\, \pi_{\text{ctx}})$ and private witness $w$:

$$
\mathcal{R}(x, w) \iff F_{h_B}(w) = y \;\wedge\; \mathsf{Attested}(w, c_A) \;\wedge\; \mathsf{Context}(w, \pi_{\text{ctx}})
$$

where $F_{h_B}$ is the provable evaluator for bundle $B$, $\mathsf{Attested}$ checks signatures on attested inputs against keys in the commitment $c_A$, and $\pi_{\text{ctx}}$ binds the proof to a context — CN code, sector, production route, reporting period, and a commitment to the installation identifier — so a proof cannot be replayed for different goods or periods.

The verifier learns $y$ and the context. It does not learn any component of $w$.

### 5.2 Public and private inputs

| Public (in the proof) | Private (never leaves the installation) |
|---|---|
| Rule-bundle hash $h_B$ | Production tonnage, activity data, fuel and material flows |
| Output $y$ (SEE) and, optionally, coarse aggregates the operator *chooses* to reveal | Sector-specific measurements (anode-effect minutes, N₂O concentrations, clinker factors, electricity consumption) |
| Context: CN code, sector, route, period, installation commitment | Precursor purchase quantities and supplier identities |
| Attestation commitment $c_A$ | The attestations themselves (verifier report contents, meter data) |
| Hashes of incorporated upstream proofs (§6) | Upstream proofs' private inputs (never known to this prover either) |

### 5.3 Trust anchors

A proof establishes that the *computation* is correct. It cannot establish that the *inputs* are true — an installation that lies to its own software obtains a valid proof of a false number. The architecture therefore requires that inputs which determine the result be **attested**, and the proof checks the attestations. The framework recognises three classes:

1. **Accredited-verifier attestation.** Where actual values are claimed, Article 8 requires verification by an accredited verifier. The verifier, after its site visit and review, signs a structured attestation of the verified inputs (or of the verified result and the bundle hash used). This is the anchor the legal framework itself recognises, and it is the primary anchor in this architecture.
2. **Instrument attestation.** Signed meter or laboratory data. Supplementary evidence, and useful in the interim before verifier capacity exists; not a substitute for anchor 1.
3. **Upstream proof.** For precursors, the attestation *is* another proof (§6).

We call the resulting pattern **verify once, prove many times**: the expensive, human, legally required act — accredited verification — happens once per installation per period. The operator can then generate any number of proofs for different shipments, customers, declarants or jurisdictions, each bound to the same attestation, without repeating the disclosure.

### 5.4 Proof systems and engineering choices

The design is proof-system-agnostic. The reference implementation's initial target is a circuit DSL with recursion support and a TypeScript-native verifier, so that verification runs in a browser or Node process with no proprietary tooling. A general-purpose zero-knowledge virtual machine — which proves the execution of ordinary compiled code and would allow the rule bundle to *be* the provable program with no separate circuit — is the intended second target; it strengthens R3 (the bundle hash becomes the program hash) and R8 (a new sector is new code, not a new circuit) at the cost of longer proving times.

Two engineering points are worth stating. First, in version 0.x the provable evaluator is a hand-written mirror of the plain evaluator for the implemented sector, and correspondence is established by the differential tests of §4.4; automatic derivation of circuits from rule modules is roadmap, not a claim. Second, proving time for a single installation's methodology at the fixed-point precision above is seconds to low minutes on commodity hardware; verification is milliseconds. Neither is a bottleneck for annual or per-shipment use.

---

## 6. Multi-tier supply chains and recursive composition

### 6.1 The paradox, one tier up

Most CBAM goods that cross the EU border are **complex goods**: steel sections, aluminium profiles, sheet, wire, castings, cement from imported clinker, mixed fertilisers from imported ammonia — each embodying precursor goods produced elsewhere, often by a different company. IR 2025/2547 requires the producer of a complex good to include the embedded emissions of its precursors, with weighted averaging where precursors under one CN code come from several installations or periods (Arts. 13–14).

This presents the disclosure paradox a second time, one tier upstream: the extruder needs the smelter's SEE; the re-roller needs the slab producer's; the nitrate plant needs the ammonia plant's. None of those suppliers wants to give a customer with negotiating leverage its process data.

### 6.2 Proofs compose

Let an upstream installation produce a proof $\pi_1$ for public output $y_1$ under bundle $h_B$. The downstream producer's proof $\pi_2$ takes $(\pi_1, y_1)$ as *private* inputs, verifies $\pi_1$ **inside its own circuit**, uses $y_1$ in the precursor-attribution step, and outputs $y_2$. The downstream producer learns nothing about its supplier beyond $y_1$; the EU declarant verifying $\pi_2$ learns only $y_2$ and — if the producer chooses to expose it — the hash of $\pi_1$ as evidence that a precursor proof was incorporated.

$$
\pi_2 \;:\; \exists\, w_2, \pi_1, y_1 \;.\; \mathsf{Verify}(\pi_1, (h_B, y_1, \dots)) \;\wedge\; F_{h_B}\big(w_2 \,\|\, y_1\big) = y_2
$$

Weighted averaging across $k$ suppliers generalises directly: $\pi_2$ verifies $\pi_{1,1} \dots \pi_{1,k}$ and computes the mass-weighted average per Art. 14(2), or — where the operator can evidence single-supplier use per Art. 14(3) — the specific value. Precursors of EU origin, or from excluded territories, contribute zero embedded emissions; the bundle encodes this branch and the proof can attest the origin claim via a customs document hash or signed supplier declaration.

### 6.3 Depth

Because $\pi_2$ is itself a proof of the same form, a third tier composes over it identically, and so on. The chain ore → pig iron → crude steel → hot-rolled coil → cold-rolled sheet → stamped component → assembled article is five compositions, each performed by the party that holds the relevant physical inputs, each learning only the SEE of the good it bought. No party ever holds the full chain's data — including the platform provider, who holds none of it.

### 6.4 Downstream goods and the 2028 extension

The proposed downstream extension is where this matters most. For a downstream good, the Commission's approach reduces embedded emissions to the CBAM-material content of the product multiplied by the embedded emissions of that material, plus (where material) the producer's own processing emissions. For a manufacturer of fasteners, hardware, structural parts or household articles this has three consequences:

- **Its number is almost entirely someone else's number.** A bolt maker's own emissions per tonne are small; the steel's are not. Without an actual value for the steel, the bolt maker's importer defaults — with the 30 % mark-up from 2028.
- **The someone else is often two or more tiers away.** A stamping shop buys sheet from a service centre that buys coil from a mill. The stamping shop may not know which mill. A proof passed down the chain carries the mill's SEE without carrying the mill's identity.
- **Anti-circumvention and proofs point the same way.** The proposal's anti-circumvention powers target goods whose declared origin conceals their true origin. A proof bound to an installation commitment and a verifier attestation *is* evidence of true origin at the material level. The architecture and the enforcement objective are aligned, not in tension.

The bundle handles downstream goods in `sectors/downstream/content.ts`: the rule takes the good's CN code, the mass of each CBAM precursor material it contains (an attested input — bill of materials, weighed inputs, or a Commission-specified content coefficient where actual content is not monitored), the corresponding upstream proofs, and the producer's own attested process emissions, and returns the SEE. The product list itself is data in `scope.ts`, not logic; when trilogue settles the list — and each year the Commission revises it — the change is a signed bundle release.

### 6.5 The proof as a supply-chain currency

The practical consequence is that the **proof format becomes a currency of trust within the supply chain**, independent of the EU declarant. A mill or smelter can issue proofs to every customer at marginal cost; a component maker can differentiate on a low SEE without exposing its supplier list; a trader can pass proofs through without ever holding data it would be liable for protecting; and a manufacturer of downstream goods who today has never heard of CBAM can, in 2028, satisfy its importer by forwarding what its suppliers already produce.

---

## 7. Fit with the legal framework

It is essential to be precise about what this architecture is and is not.

### 7.1 What it does not replace

- **Accredited verification (Art. 8, Annex VI).** Actual values must be verified by an accredited verifier, with a site visit in the terms the accreditation rules require. A zero-knowledge proof is not verification and does not make a verifier unnecessary. It is a mechanism for *transporting* the result of verification without transporting the data.
- **The declarant's liability.** Regulation 2025/2083 allows the declarant to delegate *submission* of the declaration, and IR 2025/2550 provides for delegated access to the Registry; the delegating declarant remains responsible for its obligations. A proof does not shift liability. It gives the declarant an audit trail commensurate with the liability it already bears.
- **The CBAM Registry.** The Registry's operator portal, verifier module and declarant portal remain the channel of record. The architecture produces the *inputs* to a declaration and the *evidence* behind them; it does not bypass the Registry.
- **Default values.** Where an operator cannot or will not obtain verification, default values apply, with mark-up. The architecture makes the alternative — proving an actual value — available to operators who could not previously do so without disclosure.

### 7.2 Where it fits

| Role | Today | With this architecture |
|---|---|---|
| **Operator** (non-EU installation, any tier) | Enters data in the Registry operator portal or sends the communication template to each declarant or customer; discloses activity data | Runs the bundle locally; obtains verifier attestation once; issues proofs to any number of declarants and downstream producers |
| **Accredited verifier** | Verifies data; issues report | Additionally signs a structured attestation the proof can check; can rely on the bundle to check arithmetic, concentrating effort on inputs and monitoring, where human judgement matters |
| **Downstream manufacturer** | Requests data from suppliers; often cannot obtain it; importer defaults | Receives supplier proofs; composes its own; never sees supplier data |
| **Declarant** | Receives data or uses defaults; liable for correctness | Receives $y$ and proof; verifies in milliseconds; declares $y$; retains proof, bundle hash and attestation commitments as evidence |
| **Competent authority / Commission** | Reviews declarations; may request evidence | Can re-verify proofs against the published bundle at any time; can publish or endorse bundles as reference implementations |

### 7.3 Evidentiary status

Nothing in the CBAM framework currently accords a cryptographic proof any specific evidentiary status. We do not claim one. We claim that a declarant holding a verifier attestation, a proof bound to that attestation, and a public rule bundle is in a materially stronger evidentiary position than a declarant holding a spreadsheet emailed by a supplier. Whether competent authorities come to *prefer* this form of evidence is an empirical question the coming declaration cycles will answer; §15 proposes engagement to accelerate it.

---

## 8. Deployment models and data sovereignty

### 8.1 The sovereignty requirement

For a producer in China — or in any jurisdiction with comparable controls — the question "where does my data go?" is prior to every other question. Cross-border transfer of industrial operating data may require security assessment or standard-contract filings; state-owned enterprises face additional internal controls; and commercially, releasing installation data to a foreign vendor is simply unattractive. Any architecture that requires data to leave the installation to be computed upon will not be adopted at scale in the jurisdictions that produce most of the world's steel, aluminium and cement.

The architecture is therefore designed so that **nothing but the proof, its public inputs and the rule-bundle hash ever needs to leave the installation** — and so that the operator can verify this claim rather than trust it.

### 8.2 Deployment tiers

| Tier | Description | Typical user |
|---|---|---|
| **Air-gapped** | Software installed from verified media on a network-isolated host. No outbound connectivity. Rule bundles imported as signed archives via removable media. Proofs exported as files or QR codes. | Producers in restrictive jurisdictions; SOEs; anyone whose IT policy forbids egress |
| **Self-hosted, connected** | Same software on the operator's own infrastructure with outbound access for bundle updates over signed channels. No inbound access. | Producers and downstream manufacturers with standard enterprise IT |
| **Hosted (cloud)** | Operated by a service provider for parties who do not wish to self-manage. Appropriate for EU declarants, who must in any case hold the data they declare, and for smaller operators without IT capacity. | EU importers; SMEs |

The software is identical across tiers. The air-gapped tier is not a restricted edition; it is the reference configuration.

### 8.3 Properties the software must have, and how they are demonstrated

- **No telemetry, no licence check, no update check.** Stated in the documentation, enforced in code, verifiable by network monitoring during evaluation.
- **Signed rule bundles.** Each bundle is distributed with a detached signature from the maintainers' published keys; the software refuses unsigned or mis-signed bundles. Because every proof carries $h_B$, an offline installation that lags behind a correction cannot silently produce proofs under stale rules without that fact being visible to the verifier.
- **Reproducible builds.** Release binaries are built deterministically so that any party can build from published source and obtain a byte-identical artifact. This converts "trust the vendor" into "verify the build", which for a US-origin tool deployed in China is a precondition, not a nicety.
- **Software bill of materials and provenance** (SBOM; SLSA-style provenance attestations) published with every release.
- **Platform support** for the Linux distributions in actual use at target installations, including openEuler and Kylin, with offline package mirrors documented.

### 8.4 What crosses the border

For a shipment from a non-EU manufacturer to an EU importer, the complete cross-border data flow is:

1. Proof $\pi$ (a few kilobytes);
2. Public inputs: $h_B$, $y$, context (CN code, sector, route, period), attestation commitment;
3. Optionally, hashes of incorporated upstream proofs.

No activity data, no supplier identities, no verifier report contents. The verifier report remains with the operator and the verifier, available to a competent authority on lawful request through channels that already exist.

---

## 9. Interoperability: proof and rule-bundle specification

For the approach to succeed it must not be proprietary. We propose — and the reference implementation adopts — open specifications for two artifacts.

### 9.1 Rule-bundle specification (sketch)

- Canonical serialisation for hashing (deterministic ordering, normalised whitespace, excluded paths listed in `bundle.json`).
- Required metadata: `jurisdiction`, `sectors[]`, `appliesFrom`, `appliesTo`, `legalBasis[]`, `supersedes`, `changelog`.
- Required annotation schema for legal citations on every exported function and every parameter table row.
- Required test-vector format per sector, so independent evaluators can be conformance-tested.
- Signature format (detached, over the canonical hash).

### 9.2 Proof envelope (sketch)

```json
{
  "version": "1",
  "proofSystem": "[[system/version]]",
  "ruleBundleHash": "sha256:…",
  "publicInputs": {
    "output": { "see_tCO2e_per_t": "1.835000" },
    "context": { "cnCode": "7601", "sector": "aluminium", "route": "primary", "period": "2026" },
    "attestationCommitment": "…",
    "incorporatedProofs": ["…"]
  },
  "proof": "base64…",
  "verifierHints": { "verificationKeyHash": "…" }
}
```

A declarant's software verifies the envelope, checks $h_B$ against the list of bundles it accepts, and records the envelope as evidence. The envelope is identical for a steel coil, a bag of cement or a box of fasteners; only the context differs.

### 9.3 Relationship to existing standards

Product carbon footprint exchange formats (e.g., WBCSD PACT) and industry data-space initiatives (e.g., Catena-X) define how a footprint value travels between systems. The proof envelope is complementary: it can be carried as an attachment to such a record, adding proof of correctness and rule identity to a format that otherwise transports an unverified assertion. We would welcome standardisation of the envelope within one of these bodies rather than as a stand-alone specification.

---

## 10. Reference architecture and implementation

The reference implementation, published with this paper under Apache 2.0, comprises:

| Component | Language / technology | Status at v0.1 |
|---|---|---|
| Rule bundle `cbam` — `common/` | TypeScript modules + JSON parameter tables, legal annotations, test vectors | Period, precursor averaging, default-value selection with mark-ups complete; Art. 9 placeholder; Art. 31 prototype |
| Rule bundle — `sectors/aluminium/` | TypeScript | Primary smelting (slope method), secondary melting, one downstream processing route |
| Rule bundle — `sectors/steel/`, `cement/`, `fertilisers/`, `hydrogen/`, `downstream/` | TypeScript | Directory structure, boundary definitions and `scope.ts` entries only; methodology not yet implemented |
| Plain evaluator | TypeScript | Complete for implemented modules |
| Provable evaluator | Circuit DSL, hand-mirrored; recursion for precursor incorporation | Primary aluminium complete; precursor incorporation prototype |
| Differential test harness | TypeScript | Complete |
| Prover / verifier CLI | TypeScript (Node), runs offline | Complete |
| Bundle signing and import | Detached signatures; offline import | Complete |
| Registry export | Generates values in the format of the CBAM communication template / declaration data | Prototype |
| Reproducible build pipeline | Containerised deterministic build; SBOM; provenance | In progress |

Everything in this paper described in the present tense is implemented at the tagged release cited on the title page; everything described as roadmap or intended is not. Readers are encouraged to run the test vectors — in particular the worked example of §4.5 — and to report discrepancies.

---

## 11. Worked scenario: smelter → extruder → manufacturer → EU declaration

**Actors.** *S*, a primary aluminium smelter in China. *S′*, a second smelter. *X*, an extruder in China buying billet from *S* and *S′*. *M*, a manufacturer of aluminium window and door hardware in China, buying profiles from *X* — a product category of the kind proposed for inclusion from 2028. *D*, an EU importer (authorised CBAM declarant). *V*, an accredited verifier engaged by *S*, *X* and *M*.

**Step 1 — Rules.** All parties use bundle `2026.2.0` (hash $h_B$) for 2026 goods, incorporating IR 2025/2547 and the corrected default values of IR 2026/1740.

**Step 2 — Smelters.** *S* runs the bundle on 2026 activity data (as in §4.5) and obtains $y_S = 1.835$. *V* performs verification, including the site visit, and signs an attestation over the verified inputs and $h_B$. *S* generates $\pi_S$. *S′* does likewise, obtaining $y_{S'} = 1.910$ and $\pi_{S'}$. Neither smelter's activity data leaves its premises.

**Step 3 — Extruder.** *X* consumed 600 t of *S*'s billet and 430 t of *S′*'s billet to produce 1 000 t of profiles (CN 7604 10 90). Its own direct emissions — gas-fired homogenising and ageing furnaces — were 120 t CO₂. The bundle computes the weighted-average precursor SEE per Art. 14(2):

$$
\bar{y}_{\text{prec}} = \frac{600 \times 1.835 + 430 \times 1.910}{1030} = 1.866
$$

$$
y_X = \frac{120}{1000} + 1.03 \times 1.866 = 0.120 + 1.922 = 2.042 \text{ t CO}_2\text{e / t}
$$

*X* generates $\pi_X$, which verifies $\pi_S$ and $\pi_{S'}$ internally. *X* learns $y_S$ and $y_{S'}$ but nothing else about its suppliers.

**Step 4 — Declarant, 2026 goods.** *D* imports 2 400 t of profiles from *X* in 2026. It receives $\pi_X$ and $y_X$, verifies the proof in its own software, confirms $h_B$ is an accepted bundle, and records total embedded emissions of $2\,400 \times 2.042 = 4\,901$ t CO₂e for these goods. Had *D* been unable to obtain actual values, it would have applied the China default for CN 7604 10 90 with the 2026 mark-up of 10 % — a figure the bundle also computes, so *D* can see the difference the proof made.

**Step 5 — Certificates.** By 30 September 2027, *D*'s declaration includes $y_X$ and the quantity, the free-allocation adjustment per Art. 31 using the benchmark for the relevant route from IR 2025/2620, and — once the Article 9 implementing act is in force — any deduction for carbon price effectively paid under China's national ETS, for which *S* and *X* can generate a separate proof over allowance records without disclosing them. *D* surrenders the resulting certificates and retains $\pi_X$, the bundle hash and the attestation commitments as evidence.

**Step 6 — A rule change.** Suppose in 2027 the Commission revises default values or a GWP. Bundle `2027.1.0` is published and signed. *S*, *S′* and *X* import it (offline, if necessary), re-run on the *same* private inputs, and issue new proofs. *D* holds both generations of proof and can show any reviewer exactly what changed and why.

**Step 7 — The fourth tier, 2028.** Suppose the downstream extension is adopted and *M*'s hardware enters scope from 1 January 2028. Bundle `2027.2.0` adds the relevant CN codes to `scope.ts` with `appliesFrom: 2028-01-01`. *M* buys 1 000 t of profiles from *X* (now carrying $\pi_X'$ under the current bundle, $y_X' = 2.03$), machines and finishes them into 950 t of hardware with 50 t of pre-consumer scrap returned to *X*, and consumes 40 t CO₂ in its own finishing processes. *V* attests *M*'s bill of materials and process data. The downstream rule in `content.ts` computes:

$$
y_M = \frac{40}{950} + \frac{950 \times 2.03}{950} = 0.042 + 2.030 = 2.072 \text{ t CO}_2\text{e / t}
$$

*(Treatment of the scrap flow and of any content coefficients follows whatever the adopted text specifies; the figure is illustrative.)* *M* generates $\pi_M$, which verifies $\pi_X'$ internally — and, through it, the smelters' proofs — without *M* knowing which smelters supplied *X*, or *X* knowing *M*'s customers. *D*, or a different declarant importing hardware, verifies $\pi_M$ and declares $y_M$. Absent the proof, the importer would default with the 30 % mark-up — on a product whose emissions are 98 % someone else's.

The chain has grown by one link. Nothing else has changed.

---

## 12. Related work and statement of novelty

**Rules as code.** OpenFisca (France; tax-benefit simulation), Catala (Merigoux, Chataing & Protzenko, 2021 †), Blawx (Morris) and the OECD OPSI *Cracking the Code* report (2020 †) established methods and governance for executable legislation. None address emissions methodology or bind encoded rules to cryptographic proofs of their execution.

**Zero-knowledge proofs for sustainability claims.** Recent work proposes zk-SNARKs for verifying product carbon footprint or supply-chain emissions claims without revealing supplier data (e.g., arXiv 2506.16347 †), and industry commentary has suggested zero-knowledge techniques for CBAM data. These works treat the computation as a fixed circuit for a single product or footprint. They do not address rule versioning or rule identity, do not map onto the CBAM legal roles (in particular accredited verification), do not treat multi-tier precursor composition under the Regulation's specific averaging rules or the precursor-content model for downstream goods, do not address a multi-sector and annually-expanding regulatory scope, and do not address deployment under data-sovereignty constraints.

**Carbon data exchange.** WBCSD PACT and Catena-X define exchange formats for footprint data and rely on organisational trust; the Registry's operator module lets a non-EU operator share data with several declarants but requires disclosure to the Registry and to each declarant.

**Verifiable computation over regulation.** Proposals for proof-carrying tax or KYC compliance exist in the cryptography literature, but to our knowledge none has combined a legally annotated, versioned, multi-sector rule bundle with recursive proofs along a physical supply chain for a regime with live financial liability.

**Novelty claimed.** The contribution of this paper is the *combination* and its specific engineering: (i) a hash-identified rules-as-code encoding of the CBAM methodology, structured as shared cross-sector logic plus per-sector modules, in which rule identity is a public input to the proof; (ii) recursive composition matching the Regulation's precursor rules to arbitrary depth, including the proposed downstream content model; (iii) a trust-anchor model placing accredited verification, not the proof, at the root; and (iv) an air-gapped, reproducible deployment model that makes adoption in restrictive jurisdictions realistic. We make no claim of novelty for zero-knowledge proofs, rules as code, or recursive proof composition individually.

---

## 13. Limitations and open questions

We prefer to state these ourselves.

1. **Garbage in, proven out.** A proof cannot detect falsified inputs. The architecture's integrity rests on the attestation layer, and ultimately on accredited verification. Where verification is unavailable, instrument attestations are weaker evidence and should be described as such.
2. **Verifier capacity.** The bottleneck the architecture is designed to relieve is also the anchor it depends on. It reduces the *marginal* cost of using one verification many times; it does not create verifiers.
3. **Sector coverage.** At v0.1 only aluminium is implemented. The claim that the architecture is sector-agnostic rests on the bundle structure and on the observation that other sectors' methodologies are arithmetic over attested inputs in the same way; it has not yet been demonstrated in code for steel, cement, fertilisers or hydrogen. Cement and fertilisers, with indirect emissions, are the first real test of `common/indirect.ts`.
4. **Downstream methodology is not yet law.** §6.4 and Step 7 of §11 describe a proposed regime. The product list, the content model, the treatment of scrap and the anti-circumvention rules may all change in trilogue. The bundle's `downstream/` module should be regarded as a design placeholder until the text is adopted.
5. **Circuit correctness.** The provable evaluator is hand-mirrored; the differential test suite is strong evidence but not proof of equivalence. A soundness bug in the circuit would be a silent failure. Independent audit is a prerequisite for anyone relying on proofs for financial decisions.
6. **Regulatory acceptance.** No competent authority has stated how it will treat proofs as evidence. Until one does, the proof's practical value is as *superior internal evidence*, not as a recognised compliance instrument.
7. **Article 9.** Rules for recognising third-country carbon prices remain pending. Proofs over carbon-price deductions should not be issued until the implementing act is adopted.
8. **Fixed-point and rounding.** Mismatches between the bundle's rounding and the Registry's internal computation could produce small discrepancies. We track this in the test vectors and welcome reference examples from the Commission.
9. **Identity and key management.** Attestations require verifiers and operators to hold signing keys. Key compromise, rotation and revocation rely for now on conventional PKI practice.
10. **Governance of the bundle.** Whoever maintains the canonical bundle holds influence over what "the rules as code" say — and, as scope expands, over which goods are in it. §14 addresses this; we regard neutral, multi-stakeholder governance as necessary for legitimacy.
11. **Export controls and sanctions.** The software includes cryptographic components and is intended for use in China. Publicly available source code is treated favourably under US export regulations, but installation and support services remain subject to restricted-party rules. This is a compliance matter for any service provider, not a property of the software.

---

## 14. Open-source governance and sustainability

The reference implementation is released under the Apache License 2.0, chosen for its explicit patent grant and its acceptance by enterprise and public-sector legal teams in the EU, China and the United States. Contributions are accepted under the Developer Certificate of Origin. The project's trademarks are reserved; forks are free to use the code and must use their own name.

The rule bundles are published under the same licence. We expect and encourage independent implementations of the evaluator against the published test vectors; conformance, not code, is the standard. We particularly encourage sector bodies — steel, cement and fertiliser associations, and their verifiers — to contribute and review the modules for their sectors, since their members will bear the consequences of an error.

We intend to seek a neutral foundation home for the project so that no single company — including the author's — controls the canonical bundle. Until then, the maintainers' signing keys and change process are published, every change carries its legal motivation, and the changelog is public.

The company that publishes this paper earns revenue from training, deployment, integration, support and delegated filing services around the software. It does not earn revenue from the software itself, and the software is designed to outlive any single provider: an operator's proofs, bundles and evidence remain verifiable by anyone with the open-source verifier regardless of whether the original provider continues to exist.

---

## 15. Roadmap and generalisation

**Near term (to the first declaration cycle, September 2027).** Complete precursor recursion for all aluminium production routes; implement `sectors/steel/` — the largest CBAM import volume and the deepest precursor chains; publish conformance test vectors matched to Commission guidance examples; achieve reproducible builds with published provenance; commission an independent circuit audit; pilot with a small number of operator–declarant pairs; engage competent authorities and at least one accredited verifier on the attestation format.

**Medium term (2027–2028).** Implement cement and fertilisers, exercising the indirect-emissions path; hydrogen; encode the Article 9 deduction once its implementing act is adopted; encode the downstream extension as adopted, with the product list as data and the content model as rule, in time for operators to test a full year before 1 January 2028; migrate the provable evaluator to a general-purpose zkVM so that the bundle itself is the provable program; propose the proof envelope to a standards body.

**Generalisation.** The architecture is jurisdiction-agnostic by construction: a rule bundle is a jurisdiction, a set of sectors and a period. The UK CBAM (from January 2027) and prospective US measures such as the proposed Foreign Pollution Fee Act — which, as introduced, would cover steel, aluminium and other materials — would each be a bundle over the *same* attested installation data. An operator verified once could prove compliance to three regimes without three disclosures. We regard "one attested dataset, many jurisdictions' rules, many proofs" as the long-term value of separating the rules from the data — and the annual expansion of CBAM's own scope as the first demonstration that the separation is necessary.

---

## Acknowledgements

`[[Reviewers to be named with permission.]]` Errors are the author's.

## Disclaimer

This paper describes a technical architecture. It is not legal advice, and it does not purport to state how any competent authority will treat the evidence it describes. Statements about the proposed downstream extension describe legislative proposals that had not been adopted at the time of writing. Readers should rely on the cited legal instruments and on qualified counsel.

---

## References

Legal instruments (via EUR-Lex, `https://eur-lex.europa.eu`; see Appendix C for roles):

1. Regulation (EU) 2023/956 of 10 May 2023 establishing a carbon border adjustment mechanism.
2. Regulation (EU) 2025/2083 of 8 October 2025 amending Regulation (EU) 2023/956 as regards simplifying and strengthening the carbon border adjustment mechanism. ELI: `http://data.europa.eu/eli/reg/2025/2083/oj`
3. Commission Implementing Regulation (EU) 2025/2547 of 10 December 2025 on the methods for the calculation of emissions embedded in goods. ELI: `http://data.europa.eu/eli/reg_impl/2025/2547/oj`
4. Commission Implementing Regulation (EU) 2025/2550 amending Implementing Regulation (EU) 2024/3210 as regards the CBAM registry.
5. Commission Delegated Regulation (EU) 2025/2551 of 20 November 2025 on accreditation of verifiers.
6. Commission Implementing Regulation (EU) 2025/2620 (CBAM benchmarks). †
7. Commission Implementing Regulation (EU) 2025/2621 (default values), as corrected by Commission Implementing Regulation (EU) 2026/1740 of 20 July 2026. ELI: `http://data.europa.eu/eli/reg_impl/2026/1740/oj`
8. Commission Implementing Regulation (EU) 2018/2066 (Monitoring and Reporting Regulation).
9. European Commission, *Proposal for a Regulation amending Regulation (EU) 2023/956 as regards extending the scope to certain downstream products and strengthening anti-circumvention*, COM(2025) 989, 17 December 2025. †
10. Council of the European Union, general approach on COM(2025) 989, 12 June 2026. †
11. European Parliament, ENVI Committee report on COM(2025) 989 (rapporteur M. Chahim), adopted 6 July 2026. †
12. European Commission, DG TAXUD, *Guidance Document 3: CBAM methods for the calculation of emissions embedded in goods*, August 2026.

Literature:

13. OECD Observatory of Public Sector Innovation, *Cracking the Code: Rulemaking for Humans and Machines*, 2020. †
14. Merigoux, D., Chataing, N., Protzenko, J., "Catala: A Programming Language for the Law", *Proc. ACM Program. Lang.* (ICFP), 2021. †
15. OpenFisca, `https://openfisca.org`. †
16. Morris, J., Blawx, `https://www.blawx.com`. †
17. `[[Author(s)]]`, "Zero-knowledge proofs for supply-chain carbon claims", arXiv:2506.16347, 2025. † *(confirm title and authors)*
18. WBCSD, *PACT Technical Specifications for PCF Data Exchange*. †
19. Catena-X Automotive Network, *PCF Rulebook*. †
20. Goldwasser, S., Micali, S., Rackoff, C., "The Knowledge Complexity of Interactive Proof Systems", *SIAM J. Comput.*, 1989.
21. Ben-Sasson, E., Chiesa, A., Tromer, E., Virza, M., "Scalable Zero Knowledge via Cycles of Elliptic Curves", CRYPTO 2014.
22. Reproducible Builds project, `https://reproducible-builds.org`; SLSA, `https://slsa.dev`.

---

## Appendix A — Glossary

| Term | Meaning |
|---|---|
| **Authorised CBAM declarant** | The EU-established importer or indirect customs representative legally responsible for declaring and surrendering certificates |
| **Operator** | The person operating a non-EU installation producing CBAM goods, at any tier |
| **Accredited verifier** | A body accredited under Delegated Regulation 2025/2551 to verify embedded emissions |
| **Complex good / precursor** | A good whose production consumes other CBAM goods (precursors), whose embedded emissions must be included |
| **Downstream good** | A finished or semi-finished product proposed for inclusion from 2028 whose embedded emissions derive mainly from its CBAM-material content |
| **Annex II sector** | A sector (iron and steel, aluminium, hydrogen) for which only direct emissions count |
| **SEE** | Specific embedded emissions, t CO₂e per tonne of good |
| **Default value** | Country- and CN-code-specific fallback emissions figure, with mark-up |
| **Rule bundle** | A versioned, hash-identified set of executable rules and parameter tables encoding a methodology across one or more sectors |
| **Attestation** | A signed statement about inputs (from a verifier, an instrument, or an upstream proof) |
| **Zero-knowledge proof** | A cryptographic proof that a statement is true which reveals nothing beyond the truth of the statement |
| **Recursive proof** | A proof that verifies one or more other proofs as part of its own statement |
| **Air-gapped** | Operated on a host with no network connectivity |

## Appendix B — Reproducing the worked example

```bash
git clone [[repo]] && cd [[repo]]
npm ci
npm run bundle:hash rules/cbam                    # prints h_B
npm run eval -- --bundle rules/cbam --input tests/vectors/aluminium/primary-slope-01.json
npm run prove -- --bundle rules/cbam --input tests/vectors/aluminium/primary-slope-01.json --out proof.json
npm run verify -- proof.json                      # prints public inputs and OK
```

Expected output for `primary-slope-01.json`: `see_tCO2e_per_t = 1.835000`.

## Appendix C — Legal instruments and their roles

| Instrument | Role |
|---|---|
| Regulation (EU) 2023/956 | Basic CBAM Regulation; Annex I goods; Annex II direct-only sectors; Annex IV calculation methods |
| Regulation (EU) 2025/2083 | Simplification amendments: 50-tonne threshold, 30 September deadline, certificate sales from 1 Feb 2027, 50 % holding rule, delegated submission |
| IR (EU) 2025/2547 | Methods for calculating embedded emissions in the definitive period |
| IR (EU) 2025/2620 | Benchmarks for the Art. 31 free-allocation adjustment |
| IR (EU) 2025/2621, corrected by IR (EU) 2026/1740 | Default values and mark-ups |
| IR (EU) 2025/2550 | CBAM Registry: operator portal, verifier registration, delegated access |
| DR (EU) 2025/2551 | Accreditation of verifiers |
| COM(2025) 989 and legislative positions | Proposed downstream extension, scrap, anti-circumvention, annual review (not adopted at time of writing) |
