<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# DSL / Example SHACL Coverage — where each `validate` phase runs, and why

> **Genre.** A decision record, not a tutorial. It states, per validation phase,
> **which gate runs it** (`make check` vs `make heavy`) and **which surface owns
> it**, each with a `file:line` citation, so a reader of `make help` — or of the
> `validate` target's help string — can trust exactly what runs.
>
> **Why this document exists.** `crates/validate/src/validate_all.rs`'s
> `ValidationRun::run` has phases (per-example SHACL, and mapping/statement/test
> **DSL SHACL**) that only execute when the caller supplies their inputs. The
> live `make validate` entrypoint historically supplied none of them, so those
> phases were **dark** while the target's help advertised "DSL SHACL". This is
> the defect class [`docs/GATE-AND-PIPELINE.md`](./GATE-AND-PIPELINE.md) names:
> "a false claim in help text or a comment is itself a defect." This record is
> the human-readable half of the fix; the
> machine-checkable halves are the help⟺registry test in
> `crates/gmeow-dev-cli/tests/make_gate_contract.rs` and the liveness test in
> `crates/validate/tests/dsl_shacl_live.rs`.

## The single source of truth

The phase→(surface, home) mapping is a declarative registry,
`VALIDATE_PHASE_COVERAGE` in `crates/validate/src/dsl_coverage.rs`. The `validate`
help string, the CLI wiring in `crates/gmeow-dev-cli/src/dev_validate.rs`, and the
tests all read that registry; this document is its prose companion. If the two
disagree, the `make_gate_contract.rs` help⟺registry assertion fails — the drift
cannot pass silently.

## Per-phase decision

| Phase | Surface | Gate | Home (who executes it) | Citation |
|---|---|---|---|---|
| 9 — example **coverage** (every slice ships ≥1 example) | `slices/*/*/examples/` | `make check` | The live `validate` gate, via Phase 5c | `crates/validate/src/validate_all.rs:749` (`check_example_coverage`, `:1819`) |
| 11 — **mapping** DSL SHACL | `dsl/mappings/**/*.ttl` vs `shapes/mapping-dsl-shapes.ttl` | `make check` | The live `validate` gate | `validate_all.rs:910`; `dev_validate.rs` (authored-source path) |
| 12 — **statement** DSL SHACL | `dsl/statements/**/*.ttl` vs `shapes/statement-dsl-shapes.ttl` | `make check` | The live `validate` gate | `validate_all.rs:930`; `dev_validate.rs` |
| 13 — **test** DSL SHACL, central | `dsl/tests/**/*.ttl` vs `shapes/test-dsl-shapes.ttl` | `make check` | The live `validate` gate | `validate_all.rs:950`; `dev_validate.rs` |

### Why these placements

- **All on `make check`, none on `make heavy`.** Each is fast and deterministic
  on this branch's own edits, so P6 (`GATE-AND-PIPELINE.md:260`) keeps them on
  `make check`. The three central DSL trees are tiny (the whole `dsl/` tree is a
  few dozen `.ttl` files) and `validate_dsl` merges them standalone (no TBox
  fan-out), so runtime is set by the edit, not by corpus breadth.
- **No Rust test rebuilds the example corpus.** The former `example_sweep` test
  walked all authored examples and materialized `TBox + example` corpora inside
  nextest. That producer-reachable lane was removed under the repository's absolute
  no-test-corpus-production policy. Example-directory coverage remains on the live
  validation gate; focused projection semantics remain covered by synthetic tests.
- **Slice-local test DSL is not materialized by a Rust test.** The former
  `datatest-stable` corpus walker was removed under the no-test-corpus-production
  rule. Its 143 fixed-name files plus three flagship manifests now execute in the
  explicit, cache-keyed pre-test producer. Every spec yields an independently reusable
  receipt, and the producer binds those into an authenticated all-specs verdict; warm
  runs verify that receipt without executing cells. The live validation entry continues to cover
  the central `dsl/tests/` surface, and focused slice-engine behavior remains synthetic.

## Scope: authored-source entry only

DSL SHACL is a property of the **authored working tree** (`dsl/` + `shapes/`). The
`gmeow-dev validate --gts <bundle>` path validates a folded bundle that is **not**
the authored tree, so it carries **no DSL surface by design** — exactly as it
carries no `examples/` surface. The help⟺registry check and the liveness test
therefore assert DSL witnesses on the authored-source entry only; nothing demands
them on the `--gts` path. This is feature scoping, not capability degradation.
