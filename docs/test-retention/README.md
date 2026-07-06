# Pytest retention dossier

This project is rust-first / python-surface. The pytest suite is being culled to
the smallest possible residue: a test survives **only** if it exercises something
that has no Rust home today and cannot be expressed as a slicetest cell or
`crates/validate` conformance test.

The burden of proof is on *keeping*. Every surviving pytest test (or coherent
file of tests) has a dossier here that states:

1. **What it tests** — the concrete surface under test.
2. **Retained dynamic tests** — the exact tests that remain and a one-line
   reason each one is dynamic.
3. **Why it cannot be deleted or moved to Rust today** — the specific blocker
   (e.g. whole-merged-graph sweep, Python-only algorithm, filesystem/repo guard,
   CLI surface, PyO3 seam, Docker oracle).

A test with no dossier is a deletion candidate. A dossier whose tests have all
found a Rust or slicetest home is a deletion order.

## Retention categories

| Category | Why it survives | Migration that retires it |
|---|---|---|
| Python CLI surface | `gmeow`/`gmeow-dev` are Typer apps; `CliRunner`/subprocess behavior is Python-only | Port the CLI to Rust (`clap`) with integration tests |
| PyO3 seam | tests the binding's marshalling/error-surfacing, which Rust cannot test from the inside | Delete when the Python surface that owns the seam is removed |
| Python tool algorithm | the implementation under test is still live Python (up-projection, transform, projections, mappings, language-tags, gts views/producer) | Port the tool to a Rust crate; cover with crate tests |
| Oracle / Docker orchestration | drove external OWL 2 DL reasoners or the rdflib trust-anchor; no Rust twin by design | Reimplement the harness in Rust — the reasoning oracle is now the in-process `purrdf::entail` cross-check — or retire with its external lane |
| Static repo guard | AST / filesystem / workflow assertions about the repo itself | Port the static check into a Rust gate |

**Projection cluster — one migration, not many ports.** The up-projection,
projection (FnO/EDOAL/SPARQL), transform/transpile, mappings, and alignment tests
are all retired by the **Correspondence Calculus**
(`docs/APPLIED_CATEGORY_THEORY/take1.md`): `dsl/mappings` becomes a frontend into
`logic:Correspondence`; SSSOM/EDOAL/FnO/CONSTRUCT/up-lift become *lowerings* of one
`get`/`put` leg pair; up-projection becomes the **derived** `put` leg
(mnemomorphism); and the `conformance/correspondence` round-trip + overclaim gates
replace the Python projection/alignment checks. Their dossiers name that migration
rather than a per-tool port.

## Deletion ledger (this branch)

Removed because a Rust artifact already asserts the same behavior:

- Parity goldens (8): coverage, dsl_provenance, fno, language_tags, lint,
  normalize, reasoning, sssom → `crates/validate` (coverage/lint/language_tags/
  gufo/py_dsl), `crates/rdf-core` (fno/canon/sssom), `crates/rdf` (py_sssom).
- `test_logic_counterfactual` → `crates/conformance` runs the worlds-C corpus +
  answer goldens through the same engine; `crates/logic` counterfactual unit tests.
- `test_reasoning_lint` → `crates/validate/src/gufo.rs` (literal twins) + the
  `make validate` reasoning-invariants gate.
- Migration stubs (`test_reference_frames`, `test_profiles`, `test_accessibility`,
  `test_lexicon`) → slicetest cells + `conformance_*.rs`.
- `test_feedback_bundle` → `crates/gmeow-dev-cli/src/feedback_bundle.rs` +
  `crates/gmeow-dev-cli/tests/feedback_bundle.rs` (self-describing GTS feedback
  bundle with findings RDF snapshot, SARIF/JSON blobs, snapshot-content-id
  self-attestation, and robust verifier).
- `test_diagnostics_config` → `crates/cli-core/tests/diagnostics_config.rs`
  (resolved diagnostics output policy: console mode, artifact kinds, directory,
  stem, category, with flag > env > default precedence).
- `test_constitution` → `crates/validate/tests/constitution.rs` (all 16 cases:
  granular codes, real-manifest pass, principle/heading sync, P18 RDF-1.2
  enforcement, honor-system visibility, and every failure mode the gate exists
  to catch: zero enforcement, stale artifact/symbol/make-target/CLI, orphaned
  enforcement, title drift, undeclared enforcement, practice-only warning,
  supersession/extends drift). The manifest parser data model is subsumed by
  `gmeow_validate::constitution::{Principle, Enforcement, collect_principles}`.
- `test_mcp_server`, `test_mcp_server_consumer`, `test_mcp_memory`: the MCP
  read-surface, stdio server loop, startup-lang validation, and grounded-memory
  triad are Rust — `crates/pipeline/src/mcp.rs` plus `export.rs`, asserted by
  `lookup_envelope_matches_consumer_contract`,
  `consumer_llms_txt_uses_standard_format`, `consumer_llms_full_inlines_terms`,
  and the native MCP memory/server tests. The Python CLI now only launches the
  native server.
- `test_narrow_waist`, `test_lane_purity`, `test_no_rdflib_in_runtime`: the
  repository static guards now live in `crates/validate/src/repo_static.rs` and
  run through `make crate-check`, covering the narrow-waist, Java/Docker
  lane-purity, and first-party upstream-`rdflib` import seals.
- `test_validate`: syntax checking, structural lint, annotation-completeness
  gate, `owl:sameAs` ban (internal/external/allowlist/empty-paths), cache
  read/write, and mapping/statement/test DSL SHACL are now asserted by
  `crates/validate` (`store.rs`, `lint.rs`, `cache.rs`, and
  `tests/validate_all.rs`). The consumer-facing `gmeow validate <data>` surface
  is covered by `crates/validate/tests/data_validate.rs`. `src/gmeow_tools/validate.py`
  is retained as the Python orchestration wrapper until its remaining consumers
  are migrated.

Relocated out of the mainline test tree (dossier removed with the test):

- Retired reasoning-oracle lane (8): `test_reasoning_entailments`,
  `test_rl_agreement`, `test_classic_cross_check`, `test_reason_verify_chain`,
  `test_reason_native`, `test_logic_foundation_cases`, `test_statements`,
  `test_runner` → these drove the external OWL 2 DL / Jena / owlrl oracle cross-check
  (the "Oracle / Docker orchestration" retention category). That external Docker/Java
  lane has since been **removed** and replaced by a native, in-process, Docker-free
  reasoning oracle over `purrdf::entail` (OWL-RL subsumption + OWL-Direct-tableau
  consistency, 70/70 W3C-entailment conformance-tested) —
  `crates/logic/src/entail_oracle.rs` + `crates/logic/src/entail_crosscheck.rs`,
  exposed as `gmeow-dev reason-crosscheck` and run **on-gate** as part of
  `make reason-verify`. The native authorities that made the relocation lossless are
  already in `make check` (native EL/DL reasoning + RL closure in `crates/logic`, the
  RDF-1.2 statement round-trip in `crates/pipeline`, and the foundation-discipline
  goldens hand-verified in `crates/logic/src/foundation`). A retention dossier
  justifies a *kept mainline* pytest, so it does not outlive the test's departure from
  the mainline tree.

Constitution `meta:artifact` citations of deleted tests were redirected to the
Rust artifact that now proves the principle.
