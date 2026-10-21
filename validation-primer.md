# Primer source handoff

Source-only implementation; no formatter, build, tests, checks, lock mutation,
producer/corpus/model execution or publication was performed by this lane.
The original primitive reader plan.md and validation.md are preserved verbatim.

## Exact seven-file inventory

1. crates/docs-model/src/gmn1_primer.rs — native codecs for source enum, private row, primer.
2. crates/docs-model/src/gmn1_primer/teachability.rs — native observation/task codecs.
3. crates/docs-model/src/gmn1_primer.json.tests.rs — nine new external pure tests.
4. crates/docs-model/PRIMER-JSON.md — complete record/tag/admission wire contract.
5. crates/gmeow-dev-cli/src/dev_test_fixtures/bundle_artifacts.rs — direct compact native writer.
6. crates/docs/tests/gmn1_primer_teachability.rs — authenticated native decode.
7. crates/bundle-view/src/export.tests.rs — authenticated native decode.

No manifest or lock change. No primitive reader source touched. Current-source
caller audit found no template, CachedSite, receipt, schema or other typed Serde
embedding of these records. MCP and bundle-view production use text/accessors.

## Pure qualification selected for root

Filter: package(gmeow-docs-model) and test(json_gmn_primer_)

Nine independently authored controls cover complete literal declaration-order
bytes for all five records/three external tags; Unicode/escaping/null/set order;
roundtrip and rendering/resource/provenance/sigil/operator accessors; sorted set
deduplication; unknown records; known duplicates at every depth; missing/wrong
types and positional forms; exact bounded unsigned counters including signed
zero/fraction/exponent/overflow refusal; malformed/trailing/non-UTF8 documents.
They construct plain private rows/tasks only. None calls a graph, dictionary,
build_primer, observe, fixture loader or producer.

Actual authenticated artifact writer/readers, bundled rendering, installed MCP,
full parent scope and full gates remain unrun. Codec byte expectations are
literal source expectations; tests have not been executed by this lane.

## Focused root qualification and narrow source repair

Root's combined first run executed18 controls:17passed, one failed because
the generic native usize decoder admits signed zero. The unchanged strict
counter regression exposed this real contract defect. Its original log is
native-tests-initial.log. This lane did not execute that run.

Both counter fields now use Record.required_with and native Value.as_u64,
followed by checked usize conversion. Pinned number.rs400 rejects every signed
lexeme (including -0), fractions, exponents and overflow; record.rs804–816
attaches the field pointer to the native DecodeError. No lexeme parser or
generic integer decoder is substituted. Test expectations remain unchanged.

HeldoutTask now directly uses the native BTreeSet codecs (record.rs1375–1388),
removing unnecessary Vec intermediates while retaining sorting/deduplication.
The authenticated docs reader maps native DecodeError directly through Diag's
blanket From<E> (errors/diag.rs671), preserving the native cause instead of
wrapping it in an invented I/O error. Authentication and primitive files remain
unchanged. No checks or formatting were run by this lane after the repair.
Source is refrozen for root's combined rerun.
