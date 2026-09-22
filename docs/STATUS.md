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
and version/DOI consistency. A v1.2 draft in `paper/` adds the cross-version
verification commitment; its DOI fields are updated when it is deposited.

## Reference implementation

`kaimeter-rules` 0.1.0 and `kaimeter-interpreter` 0.1.0 implement the
aluminium bundle (`2026.3.0`, canonical identity
`sha256:f82ab85b1df845a194b45ea2677762fb1b8ba5a9898858000abd5ed1fd911139`):
reporting-period and sector-dependent mark-up rules, default-value selection
with the other-countries and Annex IV fallbacks, Art. 14 precursor averaging,
the Annex I aluminium scope table, the primary route with slope and
overvoltage PFCs, the secondary melting route, complex-good aggregation, and
stubs for the remaining sectors. The rules travel as data in `rules.json`;
the fixed interpreter evaluates them, and `bundle-hash` prints the canonical
identity. The Appendix B and §11 vectors pass at full fixed-point precision.

`kaimeter-guest` evaluates the bundle inside the RISC Zero zkVM from a
witness of per-file Merkle openings, verifying every byte against the pinned
identity before any rule runs, and commits the journal (identity, output,
context). Measured for Appendix B on `r6i.xlarge` (4 vCPU, 32 GB) with RISC
Zero 3.0.6: 5 segments, 4,073,080 user cycles, 4,718,592 total cycles,
2,506 seconds proving and a verified receipt; the guest image ID is
`[3292982656, 1874087874, …]`, machine-local until the v0.3 stable-image-ID
work. Native/guest differential tests cover every conformance vector and
seeded random inputs.

The native suite (200 tests) and the guest suite (5 tests), rustfmt, Clippy
with `-D warnings`, rustdoc, the comment policy, coverage and mutation
testing run in CI, and a Linux guest workflow builds the guest and runs the
differential suite. v0.1.0 was released on 2026-09-16; v0.2 is a release
candidate, and the planned releases are in §10 of the working paper.

## Repository

| Area | State |
| --- | --- |
| Working paper sources | Published as v1.1; v1.2 draft in `paper/` |
| Paper PDF build | Reproducible, pinned `pandoc/latex:3.11` image |
| Rule bundle | `kaimeter-rules` 0.1.0, bundle `2026.3.0`, `kaimeter-bundle-v2` Merkle identity |
| Interpreter | `kaimeter-interpreter` 0.1.0: rules-as-data evaluator and canonical commitment with openings |
| zkVM guest | `kaimeter-guest` (RISC Zero 3.0.6): in-circuit opening verification, journal, differential suite |
| Parameter tables | Aluminium defaults (Annex I, other countries, Annex IV) and GWP, with row-level provenance |
| Comment policy | `cargo xtask check-comments`, wired into the pre-commit hook |
| Rust quality gates | Workspace-wide: format, clippy, tests, coverage, cargo-deny, rustdoc, mutation testing |
| Prover tooling | `tools/prove-aws.sh`: disposable EC2 instance for the release measurements |
| Citation metadata | `CITATION.cff` and `.zenodo.json` for the v1.1 release |
| Conformance vectors | Appendix B aluminium vector checked in verbatim and passing at full precision |
