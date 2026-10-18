# Complete diagnostic preservation and qualification

The original issue requires one shared Diag error substrate, stable codes and grades, typed operation/locus and original cause evidence, deterministic CLI reporting and exit behavior, failure-path controls and a clean string-error source seal. Most implementation is already merged through PRs 1802 and 1835 and subsequent source-diagnostic repairs. Do not duplicate those implementations. Principles 4, 7, 12 and 18 apply.

The current issue and all five comments were read. The brief found two earlier attempts, four citing PRs, six observed defect classes and two comments not covered by lexical requirement extraction. Those comments were read manually. There is no available independent local reviewer dispatcher; local assessments must be labeled self-review. CodeRabbit review is requested after publication.

## Task 1: Remove the remaining reduced prover diagnostic projection

The prover has a separate diagnostic_snapshot that omits failure class, native cause detail and shared report fields. Delete it and serialize the existing ReportDiagnostic conversion in caused_diagnostic, matching the existing reasoning-input adapter. Preserve the outer operation reason, original live diagnostic and source identities. Strengthen real failure-path controls for parser refusal, typed machine evidence and deterministic serialization. No compatibility projection, new crate or fallback.

## Task 2: Verify the original complete contract

Current source-seal execution found an additional concrete defect: logic.native-builtin lacks its canonical failure class. Repair that binding in the owning logic slice, include the enforcing operational boundary and all existing French/Chinese annotation coats, and strengthen the existing typed cause-owner control. Never add the new kind to the shrink-only ungrounded census. The failed 29-case run is retained; re-demonstrate its correction on the actual issue source with the 30-case selection.

Use the original implementation and review archives to map numeric, external model and process receipts to existing controls. Run affected current consumer tests when authenticated embedded products are available, and the current source-only repository seal. Do not substitute an old unqualified artifact or invent a synthetic production bundle. No test may construct corpus. Local optimized producer, full make check/sync, host probes or resource wrappers remain prohibited. Inspect normal hooks and current hosted jobs, reusing unchanged evidence rather than starting redundant broad builds.

The current hosted producer and Rust prebuild are live; previous generation ended in runner shutdown/cancellation, not a terminal semantic defect. Current local default codebook and fixture selector are absent. An older development worktree has embedded products but no current authenticated selector; presence alone is not acceptable qualification. Preserve that worktree.

## Task 3: Publish and integrate the coherent fix

Commit and push only the seven reviewed source files through normal signed hooks: the prover adapter/control and the discovered native builtin class/binding/control/translations. Publish the concrete corrections and honest pending consumer acceptance to the issue and PR. Existing user authority permits publication and integration with broader CI pending; it does not justify claiming unrun original diagnostic tests passed or closing the issue early. Address CodeRabbit findings and use ghprsq exclusively for any final merge. Keep the full issue and goal active until all original requirements have authoritative evidence.

| Requirement | Evidence to inspect |
| --- | --- |
| Sole production Diag errors | Named source surfaces plus current live repository-static seal |
| Process command, artifact, version, status, both streams and paths | Existing prover failure controls and process receipts |
| Numeric and model operation/locus/details | Existing external-evidence and numeric failure controls; prior archive plus current-input applicability |
| Deterministic CLI output/exit behavior | Shared report tests and affected real consumer prover controls |
| Stable codes and evidence | Native parser failure through caused_diagnostic and full shared report serialization |
| String-error gate clean | Current source-only seal and crate-check evidence without starting a prohibited producer |
