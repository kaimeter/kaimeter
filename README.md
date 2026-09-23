# Kaimeter

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.22740283.svg)](https://doi.org/10.5281/zenodo.22740283)
[![Paper](https://github.com/kaimeter/kaimeter/actions/workflows/paper.yml/badge.svg)](https://github.com/kaimeter/kaimeter/actions/workflows/paper.yml)

Open, verifiable CBAM compliance: the CBAM methodology encoded as executable,
hash-identified rule bundles, with zero-knowledge proofs that a declared
embedded-emissions figure is the correct output of the identified rules —
without disclosing the underlying activity data. The design and its legal fit
are set out in the working paper *Prove, Don't Disclose* (Kaimeter Working
Paper 2026-01).

- Working paper and build instructions: [`paper/`](paper/README.md)
- Repository status: [`docs/STATUS.md`](docs/STATUS.md)
- Citation: [`CITATION.cff`](CITATION.cff) — preferred citation is the paper

## Release status

| Release | Target | Content | Status |
| --- | --- | --- | --- |
| `paper-v1.1` | September 2026 | Working paper preprint, DOI [10.5281/zenodo.22740284](https://doi.org/10.5281/zenodo.22740284) | Published 2026-09-15 |
| `v0.1` | October 2026 | `kaimeter-rules`: period, sector-dependent mark-ups, default-value selection, Art. 14 precursor averaging, scope table for current Annex I aluminium codes; primary aluminium with slope-method PFCs; the Appendix B vector passing at full precision; `bundle-hash`; no prover | [Released 2026-09-16](https://github.com/kaimeter/kaimeter/releases/tag/v0.1.0) |
| `v0.2` | November 2026 | First fixed-interpreter zkVM guest evaluating the aluminium bundle as witness with an in-circuit Merkle commitment; native/guest differential tests; overvoltage method; secondary aluminium; two-supplier precursor vector | Release candidate |
| `v0.3` | Q1 2027 | Steel sector; proof envelope and offline verifier with an accepted-bundle registry; stable interpreter image IDs; attestation-policy schema and key resolution; signed bundle distribution; reproducible-build pipeline with SBOM and provenance | Planned |
| Later | 2027–2028 | Cement and fertilisers (indirect emissions); hydrogen; Article 9 once adopted; downstream module as adopted; recursive proof composition; independent circuit audit | Planned |

## Repository layout

| Path | Purpose |
| --- | --- |
| `crates/` | Reference implementation crates (`kaimeter-interpreter`, `kaimeter-rules`, `kaimeter-guest`) |
| `tools/` | Repository tooling, including the on-demand AWS prover |
| `paper/` | Working paper sources, LaTeX template, build script and conformance vectors |
| `xtask/` | Repository automation, including the comment-policy check |
| `.github/workflows/` | Rust quality gates, the paper build and the guest workspace gates |

## License

- Text (working paper and documentation): [CC BY 4.0](paper/LICENSE-CC-BY-4.0)
- Code: [Apache License 2.0](LICENSE)
