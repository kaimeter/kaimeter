# Conventions

How changes are written, tested and committed in this repository. See
[CONTRIBUTING.md](CONTRIBUTING.md) for contribution mechanics.

## Commits

- Conventional Commits: `type(scope): imperative subject`, at most 72
  characters. Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`,
  `test`, `build`, `ci`, `chore`, `revert`.
- Every commit is signed and carries a DCO `Signed-off-by` trailer
  (`git commit -s`). `main` requires both, and history is linear.
- One atomic concern per commit; unrelated changes are never bundled.

## Tests

- Test-driven: write the failing test first, then the implementation.
- Conformance vectors under `tests/vectors/` are the first tests for rule
  code; the worked example in Appendix B of the whitepaper is the first
  vector and is fixed at the bundle's precision.
- Enforced evidence: 90% line coverage, patch coverage on pull requests and
  mutation testing on changed lines.

## Comments

- Comments explain why, not what. `TODO`, `FIXME`, `HACK`, `XXX` and
  commented-out code are rejected by `cargo xtask check-comments`.
- Public items carry doc comments; the API is the contract.
- Rule code carries structured legal annotations (`@legal`, `@source`,
  `@since`) as the sanctioned exception.

## Code

- `kaimeter-rules` is deterministic, `no_std`-compatible and free of
  floating-point arithmetic; fixed-point only.
- Readability is enforced by CI: `rustfmt`, Clippy with `-D warnings`, and
  bounded function length, argument count and cognitive complexity.

## Gates

Rust changes must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
cargo test --doc
```

Paper changes must pass the `paper` workflow: PDF build, link check and
metadata consistency.
