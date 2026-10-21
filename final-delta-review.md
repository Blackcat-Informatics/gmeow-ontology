# Independent final delta source review

Read `validation.md`, `legacy-repair-review.md` and `git-fixture-repair-review.md` before inspecting the current Domain/typed-error changes and the new selected base-control setup. No source changes, compilation, tests, helpers, host probes or corpus/producer operation were performed. This verdict is bounded source review, not a final gate or whole-unit acceptance claim.

## Identity bytes

No byte-identity change found. The three profile-program domains retain their original literal text. `identity` still hashes domain bytes, one explicit zero byte and the same native record bytes with SHA-256; its URN carries the same domain text and lowercase hexadecimal digest. The get-leg selected-template domain retains its original explicit trailing NUL and the following native JSON bytes. Inspected the pinned `purrdf_hash::Domain` implementation: construction and `as_bytes`/`as_str` add no framing, terminator or normalization. The constant substitution therefore preserves each existing preimage and identifier spelling.

## Typed failures

Retiring `projection_execution`'s thin error constructor expands the same `Diag::of_kind(crate::error::Projection { message })` at its actual callers; the domain, grade and message computations are unchanged. Required queries-archive and authored-scoring admission errors mutate only the original diagnostic's head message and append a context frame. `DiagInner` stores the grade, code and typed source separately; `inner_mut` exposes that same value and `with_context` only appends a frame. Neither operation replaces the typed cause or causal chain. This also makes the operation visible in ordinary `Diag` formatting, which prints only the head; alternate formatting still retains contexts and sources. The DCAT execution caller now uses `?` on the original prepared execution result rather than string-wrapping it. No new cause loss found in this delta.

## Immutable Git-tree control

The new selected control no longer invokes the legacy Git helper, creates commits/branches, writes user identity configuration, disables signing, changes HOME, or overrides global/system configuration. Its local closure runs only `init`, `add -A` and `write-tree`, explicitly scoped to the private fixture's git-dir/work-tree. It removes inherited Git path-selection variables only for those fixture-writing commands. Original and missing-carrier measurements receive the returned immutable tree OIDs, not HEAD or a mutable reference. The original target/source-ownership, absent-directory, excluded-example/PO/design and missing-carrier assertions remain present.

Normal Git configuration remains in effect, as required. The production base-reader commands retain their existing environment behavior; this fixture change is not a general Git-environment hardening claim. No signing or host-configuration workaround is introduced by the inspected control.

## Disposition and limits

No blocking source defect found in this final bounded delta. The earlier runtime/helper results in Stage are historical evidence for their exact sources, not execution evidence for the repaired immutable-tree control or final Domain edits. Final runtime and strict refresh remain pending: root reports a kache process-output/fingerprint failure followed by a dangling managed-target-symlink refusal on normal retry. Those are unresolved qualification blockers; this review does not authorize host changes or infer a pass. User instruction to leave host configuration untouched remains controlling. Installed/embedded consumers, actual authenticated production, the exact hosted golden and full parent acceptance remain outside this review.

## Subsequent bounded source-policy repairs

Inspected the two additional frozen deltas. `lang_docs_rendering.selected.tests.rs` changes only the authored French fixture values from `classe rédigée`/`formule rédigée` to `entité rédigée`/`expression rédigée`. English sources, term identities, documentable-page selection and retained-translation-unit assertions are unchanged. This fixture exercises page selection and translation retention, not lexical terminology equivalence; no assertion or source boundary was weakened.

The `independent_grounding_catalog` function's exact body and documentation move from inline `lib.rs` into `lib.test_support.rs`. Both the new module declaration and its crate-private re-export remain under `cfg(test)`. Existing `crate::independent_grounding_catalog()` callers in independently authored selected-template, residue and projection-debt tests retain the same path, visibility and empty-authority behavior. The body still parses only an explicitly empty dataset and calls `GroundingCatalog::from_sources` with no owned artifacts; it cannot manufacture a selected bridge. No production consumer gains access to the test-only authority, and no caller changes are required by the move.

No new source finding in these two repairs. The source-recipe retry and runtime status are root-owned pending evidence; neither repair establishes a successful build or resolves the managed-target blockage. No source changes, checks or probes were performed by this reviewer.
