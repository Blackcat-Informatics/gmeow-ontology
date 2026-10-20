# Projection debt scope, triage and consumer measurement

Complete all four requests in issue 1652, using the current 137 commitment cells rather than the historical 124-cell count. Principles 4, 7, 13, 17 and 19 apply. The relocation decision remains unchanged: only witnessed, funded transfers adjust a ceiling; no new headroom, rebaseline, floor lowering or legacy deletion is authorized.

## Task 1: State the counting and authoring boundaries

Keep the existing RDFS ratchet's unit as subsumption triples: adding signatures to the same committed numeric budget would reinterpret its existing commitments. This is not an authoring exception: domain/range semantics belong in logic:, and the old assertion that signatures are not migration debt is removed. Declarative OWL remains outside this quantitative metric because the current unified purity gate already rejects unbacked OWL constructs; reason-verify is not that authoring gate. Explain these as explicit decisions, not bootstrap accidents or permanent carve-outs.

## Task 2: Triage every current commitment

Inspect the canonical registry and authored surfaces. Publish a dated complete cell-by-cell remediation inventory: in-place canonical grounding, grounding plus owner-boundary reconciliation, or an already sealed zero budget. No cell gets a permanent ungrounded exemption. Do not mistake a committed ceiling for measured remaining debt. Domain terms stay in their own slices; external grounding correspondences are reconciled to the declared grounding owner. Preserve equivalence-before-deletion and the existing relocation law. Verify the inventory covers each current commitment exactly once; do not copy measured counts or manufacture a measurement.

## Task 3: Ship a source-backed consumer debt report

Add `gmeow slice projection-debt <slice-dir>` using the embedded registry/ceilings and exactly the existing native residue counter over the external slice's authored module, optional shapes, and mappings. It works outside a checkout and never invokes production, synchronization or a fixture loader. Resolve one manifest identity; reject missing/ambiguous inputs, malformed Turtle and unreadable selected paths. Use exact signed headroom, including deficits and counts above the signed-64-bit boundary. Reuse the same arithmetic in the developer report instead of saturating counts. Accept named output formats explicitly and fail on unknown formats. Report-only budget deficits are data, not a false gate pass.

The installed bundle alone cannot reveal a later source edit: commitment and live measurement remain distinct. This command requires the explicitly selected authored slice input and consumes no repo-local generated files. Whole-repository maintenance, shared DSL surfaces and ceiling edits stay in gmeow-dev.

## Task 4: Qualify and integrate

Use independently authored tiny source directories for source discovery, exact counts, default/committed budgets, deficits, nested mappings, exclusion of example/test inputs, malformed/missing/ambiguous inputs and deterministic output. Verify the scope decisions through existing guard/counter controls. Run strict prebuild Clippy and normal signed hooks on settled changes. No local optimized producer, corpus/fixture production, synchronization or full gate. The installed bundle-backed consumer build/runtime remains a hosted qualification limit if its authenticated bundle is unavailable locally. No independent dispatcher is exposed; label self-review honestly. Publish the complete result, merge through ghprsq while ignoring CI under standing authority, verify archive/landing and clean only this owned branch/worktree. Preserve the unfinished GMN sibling.
