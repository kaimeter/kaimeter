# Fixed interpreter and guest contract (v0.2)

Status: accepted (2026-09-16). This record is checkpoint unit 1 of the v0.2
workstream (`guest: record the RISC Zero target and interpreter contract`).
Accepted by the maintainer, which releases the remaining units: the
interpreter crate, the `rules.json` rule content, the overvoltage and
secondary routes, the differential suite and the guest.

## 1. Scope

Whitepaper §10 asks v0.2 for the first fixed-interpreter zkVM guest: the
aluminium bundle supplied as witness, `h_B` recomputed in circuit,
native/guest differential tests, the overvoltage method, the secondary route
and a two-supplier precursor vector reproducing §11.

In scope:

- the interpreter contract of §5.4 — bundle as witness, in-circuit `h_B`,
  a stable interpreter identity;
- the aluminium rule content of bundle `2026.3.0`;
- differential tests across the plain evaluator, the interpreter run natively
  and the guest.

Out of scope, deliberately deferred:

- attestation policy `h_P`, signature verification and the attested-input
  root (v0.3; §5.1, §9.1);
- the proof envelope and the offline verifier (v0.3; §9.2);
- incorporation of upstream proofs and recursive composition (§6; later);
- signed bundle distribution and the reproducible-build pipeline with SBOM
  (v0.3);
- steel, cement, fertilisers, hydrogen and indirect emissions (v0.3+).

## 2. Target

RISC Zero is the target zkVM. The guest is a RISC Zero program built to the
zkVM's RISC-V target and proved with `risc0-zkvm`; both versions are pinned in
the guest workspace and installed at those versions in CI.

- The guest is built with the containerised `cargo risczero build`, so the ELF
  — and therefore the image ID a verifier pins — is reproducible from
  published source.
- Local guest builds embed the checkout path, so an absolute image ID is only
  reproducible from the containerised build. v0.2 records the measured ID with
  the release metadata; the pinned test and the container build follow with
  the stable-image-ID work of v0.3 (whitepaper §10), including aligning the
  container toolchain with the lockfile's dependency versions.
- Guest builds and proving run on Linux only (CI or WSL2). The root workspace
  stays free of the RISC Zero toolchain so the native Windows gates keep
  passing.

## 3. Canonical rule encoding

The rules travel as data. A new semantic file, `rules.json`, sits at the root
of `kaimeter-rules` and is covered by the bundle hash like any other semantic
file. The evaluator reads it, and any parameter tables it names, from the same
witness set.

- `schema: kaimeter-rules-v1`.
- `rules`: id → program. A program is an expression DAG over a closed
  instruction set; evaluated over an invocation record it yields the rule's
  result.
- Every rule carries `legal`, `source` and `since` fields, so the encoded rule
  keeps the annotations the source carries today.
- Invocation records are named and typed: scalars, UTF-8 strings, booleans and
  lists of records (precursor supplies, for example).
- Values are the bundle's fixed-point scalar (i128 at 10⁶, rounding half to
  even), strings, booleans and lists. No floating point, no time, no I/O, no
  randomness.

Instruction set v1:

- constants and inputs — `const`, `input`;
- table access — `table`, selecting a named parameter table's row by
  predicates over its string columns (equality, longest-prefix match for CN
  codes) and returning a named column;
- arithmetic at the bundle scale — `add`, `sub`, `mul`, `div`, `neg`, `abs`,
  `min`, `max`, `round`;
- aggregation — `sum` and `count` over lists;
- control — `select`, `eq`, `ne`, `lt`, `le`, `gt`, `ge`, `and`, `or`, `not`,
  over scalars, strings and ISO dates;
- any evaluation error — overflow, division by zero, missing row, no matching
  branch — aborts the computation; no proof is produced.

The instruction set is the only thing fixed by the guest image. New rules,
sectors, CN codes, default values, parameter tables and formulas are new
`rules.json` and parameter content under the same schema and do not move the
image ID; adding an *operation* is an interpreter change and a new image ID.
That is the line between R8 (regulatory agility) and interpreter versioning.

`rules.json` is authored, not generated from the Rust rule source. The plain
evaluator and the interpreter are deliberately independent implementations,
and the differential suite (§6) is the binding evidence between them;
generating one from the other would make that test vacuous. The file is
serialised in a fixed form (UTF-8, LF, schema key order) so review diffs are
meaningful; the hashed bytes are exactly the committed bytes.

## 4. In-circuit bundle commitment

- The identity is `kaimeter-bundle-v2`: a SHA-256 Merkle tree over the same
  semantic file set, one leaf per file, with domain-separated leaf and node
  tags, path-sorted leaves and an RFC 6962-style split that promotes an odd
  trailing node instead of duplicating it. The root is `h_B`, unchanged in
  meaning: it commits the exact evaluated file set, LF-normalised, with
  excluded paths absent.
- The guest receives the pinned root and one inclusion proof per file the
  evaluation reads, verifies every opening against the root, and only then
  evaluates. No rule or parameter byte enters the computation
  unauthenticated, and in-circuit work scales with the bytes actually read,
  not with the bundle size. The linear construction measured 188,622,280
  cycles for Appendix B, most of it hashing the whole crate; the openings
  construction exists to remove that cost.
- The construction lives once, in `kaimeter-interpreter`; `kaimeter-rules`
  re-exports it so the pinned native identity and the in-circuit verification
  cannot drift. `open_file` and `verify_opening` are conformance-tested
  against an independently computed root vector and exhaustively for small
  trees.
- SHA-256 remains the commitment. A proof-friendly hash over the same leaves
  stays available as a fallback, released as a new bundle version, never an
  in-place change.

## 5. Crate layout and identities

```text
crates/kaimeter-interpreter   no_std + alloc: fixed-point arithmetic,
                              canonical Merkle commitment + SHA-256, rule
                              schema, evaluator F
crates/kaimeter-rules         plain evaluator, parameter tables, bundle
                              metadata and the bundle-hash binary; depends on
                              kaimeter-interpreter
crates/kaimeter-guest/        separate workspace, excluded from the root
  guest/                      RISC Zero program: witness, checks, journal
  host/                       build, execute, prove and differential harness
```

- The root `Cargo.toml` excludes `crates/kaimeter-guest`, so `cargo clippy
  --workspace`, the tests, coverage and mutation gates never require `rzup`.
- Two identities, deliberately separate: `h_B` identifies the rules and
  parameters; the guest image ID identifies the interpreter. A rule change
  moves `h_B` and leaves the image ID; an interpreter change moves the image
  ID. The envelope (v0.3) carries both.
- Versions: bundle `2026.3.0`; `kaimeter-interpreter` starts at `0.1.0`;
  release `v0.2.0`.

## 6. Evaluators and differential testing

Three executions of the same methodology:

1. the plain evaluator — the typed functions in `kaimeter-rules`;
2. the interpreter, executed natively on the host;
3. the guest — the RISC Zero ELF, executed in the zkVM.

All three must agree exactly, at full fixed-point precision, on:

- every conformance vector: Appendix B, overvoltage, secondary, complex-good
  aggregation and the two-supplier vector of §11;
- seeded pseudo-random invocation records generated by the host harness, so a
  disagreement is reproducible from its seed.

A disagreement fails CI. Pull requests execute the guest without proving;
proving runs on `main`, and its measured cycles, proving time and image ID are
recorded with the v0.2 release metadata.

## 7. Public outputs and witness

- Public (journal): `h_B`, `y` (the scaled SEE) and the context — sector,
  route, CN code, reporting period. The byte layout is fixed by the
  interpreter schema.
- Private witness: the pinned root, one inclusion proof per file the
  evaluation reads, and the invocation record.
- The guest verifies every opening against the claimed `h_B` before
  evaluating; the verifier checks the journal against the `h_B` it accepts.
- No attestations, installation identifiers or upstream proof hashes are
  handled in v0.2; the relation proved is
  `h_B = root(B) ∧ F(opened(B), w) = y` over the fields above.

## 8. Consequences and risks

- Drift between `rules.json` and the Rust rules is the principal correctness
  risk; the differential gate is the mitigation, and §13.6 already states its
  limits.
- Interpreter upgrades, including RISC Zero toolchain bumps, move the image
  ID; they are explicit units that regenerate the pinned constant, never
  incidental changes.
- The full file set stays committed, source included, but the witness carries
  only the openings the evaluation reads, so proving cost no longer scales
  with the source tree; the linear construction's measurement motivated this
  and §4's hash fallback remains available.
- Anything touching the guest is unavailable natively on Windows; WSL2 or CI
  is the local workflow.
