# Issue #1987: Use native JSON for abduction, projection and correspondence observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Move three complete CompileLogicStage observation channels to PurRDF native JSON under Principles 4 and 7: abductive schema observations, selected projection fixture observations and correspondence reconstruction outcomes. Their nested Formula, Diagnostic and RecordedDiag types already have native owners, so these channels can share one bounded qualification without copying compiler semantics.

Acceptance:

- Migrate the complete five-field abduction Observation, the complete projection observation map and nested record, and the complete correspondence recorded unit outcome. Wire the actual publishers, authored readers and authenticated corpus readers. Remove obsolete local DTO Serde ownership while preserving genuine shared binary owners.
- Preserve all diagnostics, root and predicate sets, formula structure and order, terminal projection strings, map keys, null unit successes and complete causal refusals at every outcome level. Require named records and all declared fields; tolerate unknown fields; refuse duplicate known fields, positional records, malformed outcomes and malformed shadowed map values. Preserve typed last-wins map behavior and set sorting/deduplication.
- Keep source selection, reconstruction, projection, canonicalization, authentication and every existing corpus assertion unchanged. No compiler-program or prepared-program replacement is included.
- Reuse the authored correspondence fidelity and seven-output projection controls. Add focused complete-wire, causal-outcome and malformed-admission coverage for these actual transports, including abduction. Compile actual pipeline and developer CLI callers and run normal hooks.

Remaining contracts: #1776, #1336 and #1962. Full compiler migration, authenticated corpus execution, shipped assets, browser parity and installed acceptance remain with their existing owners. The user retains the explicit restriction against local optimized producer, corpus production, synchronization and full gate execution, and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

