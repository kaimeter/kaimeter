# Status

Snapshot of what exists and what comes next. Updated with each release.

## Working paper

The v1.1 preprint — *Prove, Don't Disclose: Verifiable CBAM Compliance Across
Industrial Supply Chains Using Rules as Code and Zero-Knowledge Proofs*,
Kaimeter Working Paper 2026-01 — is published on Zenodo:

- Version DOI: [10.5281/zenodo.22740284](https://doi.org/10.5281/zenodo.22740284)
- Concept DOI (all versions): [10.5281/zenodo.22740283](https://doi.org/10.5281/zenodo.22740283)
- Publication date: 2026-09-15

The paper workflow builds the PDF from the sources with the pinned
`pandoc/latex:3.11` image and asserts link health, the Appendix B arithmetic
and version/DOI consistency.

## Reference implementation

`kaimeter-rules` 0.1.0 implements the aluminium bundle (`2026.2.0`, canonical
identity `sha256:cd4f4446cf09ac8c0b5bb5a5cf91dd774eeb885c034674c09507696f60604c27`):
reporting-period and sector-dependent mark-up rules, default-value selection
with the other-countries and Annex IV fallbacks, Art. 14 precursor averaging,
the Annex I aluminium scope table, the primary route with slope-method PFCs,
and stubs for the remaining sectors. `bundle-hash` prints the canonical
identity, and the Appendix B vector passes at full fixed-point precision. No
prover is included; the first zkVM guest is planned for v0.2.

The crate suite (113 tests), rustfmt, Clippy with `-D warnings`, rustdoc, the
comment policy, coverage and mutation testing run in CI. v0.1.0 was released
on 2026-09-16; the planned releases are in §10 of the working paper and
tracked in the root README.

## Repository

| Area | State |
| --- | --- |
| Working paper sources | Published as v1.1; sources in `paper/` |
| Paper PDF build | Reproducible, pinned `pandoc/latex:3.11` image |
| Rule bundle | `kaimeter-rules` 0.1.0, bundle `2026.2.0`, canonical SHA-256 identity |
| Parameter tables | Aluminium defaults (Annex I, other countries, Annex IV) and GWP, with row-level provenance |
| Comment policy | `cargo xtask check-comments`, wired into the pre-commit hook |
| Rust quality gates | Workspace-wide: format, clippy, tests, coverage, cargo-deny, rustdoc, mutation testing |
| Citation metadata | `CITATION.cff` and `.zenodo.json` for the v1.1 release |
| Conformance vectors | Appendix B aluminium vector checked in verbatim and passing at full precision |
