# Independent source review: quality-profile identity JSON

Disposition: no outstanding actionable finding after root's narrow source-admission repair below. Reviewed Stage plan, `profile.rs`, its external test module, `PROFILE-IDENTITY-JSON.md`, and the two `bench.rs` comment corrections. No tests, checks, builds, host probes, producer/corpus operations or source edits were run by this reviewer.

## Actual owner and preserved identity

The production change is the actual `ProfileAssessment::new` hash input: native `record::to_vec(profile)` replaces Serde serialization. The four record fields remain `iri`, `key`, `target_tier`, `axes` in declaration order. Applicability preserves the original internally tagged Serde representation: status first, then reason only for `not_applicable` and `unsupported`. The native BTreeMap codec iterates key order. All selected fields are strings, enums and maps, so float/number identity and nonfinite handling do not arise in this contract. Empty strings/maps remain explicit, compact escaping preserves UTF-8/control spelling, and no newline is added. The unchanged plain BLAKE3 computation and `blake3:` spelling consume these exact bytes.

`score_external_slice_from_files` still calls `ProfileAssessment::new` after preparing inputs and scoring the selected standard. Unknown target tier, missing measurement and nonfinite measurement refusals remain before the hash construction. Removing the old serialization-error mapping introduces no fallback: the selected native string/map writer is infallible for this model. Corpus/rubric identities, input framing, denominator and completeness logic remain unchanged.

Serde derives remain intentionally required for the separate report metadata exit (`report.rs` inserts `serde_json::json!(profile)`). This unit neither removes that dependency nor bridges the native bytes through Serde. The bench comment corrections accurately distinguish generated projections from their tracked baseline without changing paths or renderer behavior.

## Independent controls and limits

The three new `profile_native_json_` controls compare complete literal bytes for all applicability variants and sorted axis keys; empty fields, escaped controls and Unicode; and the actual standalone assessment path plus a fixed BLAKE3 golden. The golden's expected bytes are literal authored content and the test states its digest was independently obtained with `b3sum`; the test does not derive its expected digest through the new serializer. This reviewer inspected that structure but did not rerun the independent digest tool. The reused helpers construct only tiny in-memory standards and supplied file maps; they do not construct the repository corpus or call a producer.

## Source-admission follow-up

At inspection, the new record macro is spelled `purrdf_lex::json_record!`. The previously established producer source-capability contract uses external-prelude-qualified `::purrdf_lex::...!` to avoid local-module shadowing. This was reported promptly to root for confirmation/repair before qualification; this review does not claim the current macro spelling passes the source-input seal. The new wire document exists on disk but was absent from ordinary tracked diff/status output; root must include it intentionally in the final source inventory.

Root repaired the macro to `::purrdf_lex::json_record!`; the reviewer re-read that exact source line and confirmed the qualification. Root also confirmed explicit staging of the new wire document. These dispositions resolve the source findings; actual source-seal execution remains root-owned and is not inferred from the repair.

## Follow-up: direct native dependency

Root's first compilation caught the missing direct `purrdf-lex` dependency. The initial review did not inspect the crate manifest and missed this integration requirement; the earlier statement that the unit added no dependency was therefore incomplete. Root added only `purrdf-lex.workspace = true` to `crates/slice-quality/Cargo.toml`. Reviewed this narrow delta: it selects the existing workspace package pinned to `017f552dbdd8f4bc284121b2b95c51814f38ee0a`, adds the actual codec owner directly, and introduces no new external package/version or bridge. The wire document now explicitly describes selection through the existing pinned workspace dependency. No remaining source finding in this repair. Lock resolution and corrected compilation remain root-owned and unverified by this review.

Actual focused test execution, source-input recipe, helpers, lint/Clippy, hooks and final staging are owned by root and remain unverified by this source review. Full producer, installed, corpus and complete dependency-migration acceptance remain outstanding. Review frozen after writing this artifact.
