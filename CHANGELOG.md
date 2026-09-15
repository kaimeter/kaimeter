# Changelog

Notable changes to the Kaimeter repository. The working paper is versioned as
`paper-vX.Y`; the reference implementation will follow semantic versions from
v0.1.

## paper-v1.1 - 2026-09-15

Working paper published on Zenodo: version DOI
[10.5281/zenodo.22740284](https://doi.org/10.5281/zenodo.22740284), concept DOI
[10.5281/zenodo.22740283](https://doi.org/10.5281/zenodo.22740283).

### Added

- Repository conventions, commit protocol and contribution guide.
- Pinned Rust toolchain, workspace lints, rustfmt and clippy configuration.
- `xtask check-comments` implementing the comment policy, wired into the
  pre-commit hook.
- Rust CI quality gates, path-filtered to `crates/**` until v0.1: format,
  clippy, tests, coverage, cargo-deny, rustdoc and mutation testing.
- Working paper build: `paper/metadata.yaml`, LaTeX template, Pandoc filter,
  build script and the Appendix B conformance vector.
- Paper CI: PDF build in the pinned `pandoc/latex:3.11` image, markdown lint,
  link check, arithmetic and metadata consistency assertions.
- CC BY 4.0 license text and the paper README.
- Root README, citation metadata, Zenodo deposit metadata and status page.
- Publishing and contribution workflow documentation, pull request template.
