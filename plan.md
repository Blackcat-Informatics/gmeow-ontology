# Preserve source lexicals at the native datatype boundary

Issue 1803 requires one owner for datatype mechanisms and deletion of the inferior copy (Principles 4, 7 and 17). The current consumer trims arbitrary Unicode whitespace before native XSD parsing except for String/AnyURI. This changes an invalid numeric, boolean or temporal source into a valid native input. PurRDF numeric lexical readers explicitly accept only their grammar and perform no trim. The consumer must pass original XSD lexicals to that authority.

## Task 1: Delete generic XSD preprocessing

Remove the String/AnyURI exception and generic trim branch in LiteralValue::from_parts. Pass original lexical_form directly to native parse_by_iri. Leave the separately owned OWL rational/real parser unchanged; it is not an XSD fallback. Add actual NativeValues consumer regressions for NBSP-wrapped decimal/integer/boolean/dateTime, unchanged valid strings, and malformed string XML characters. Verify invalid values cannot acquire numeric membership or equality by lexical repair. Add an enumeration consumer check so range evaluation cannot silently admit repaired values.

## Task 2: Consume the merged string-domain fix

The initial pin f1ad5dfb634fc20622daa082a6bc2238d9a92789 exposed a hosted SHACL singleLine conformance regression: XML 1.0-only String validation rejects form feed and vertical tab. Native issue 577 corrects the common domain to the XSD-permitted XML 1.1 selection, including finite-length arithmetic, and must pass the actual failing SHACL suite before integration. Update root/fuzz immutable pins and only corresponding lock entries to that qualified correction; the initial pin is not the final deliverable. String acceptance tests must preserve XML 1.1 controls while refusing NUL/U+FFFE/U+FFFF. Refresh both tiny native attestations through the existing explicit verified producer. No corpus production, hardware/environment discovery, profile weakening, semantic aliases or new dependencies are permitted.

## Task 3: Validate and merge this ownership unit

Use make nextest-synthetic with explicit datatype/value/witness selectors, affected-package all-target clippy, normal hooks and push. Retrieve all CodeRabbit feedback and merge with ghprsq under existing authorization for pending hosted CI, clearly distinguishing pending acceptance. The broad class-model/chase migration and complete corpus qualification remain open in 1803; this unit does not redefine or close that larger contract. Preserve all sibling work and archive Stage evidence before cleanup.

No independent dispatch capability is available; do not fabricate subagent verdicts. Attributable hosted review and implementer assessment must stay distinct. No comparative speedup is claimed; this removes a redundant scan and, more importantly, prevents input alteration.

## Task 4: Qualify the landed synthetic slicetest repair

The standing all-logic-issues objective also includes 1764. Its implementation is already in this base, but its latest issue record still says uncommitted. Read the complete issue and current soleness/source-cache implementation, run the complete gmeow-slicetest synthetic selection with all four fixture/bundle selector variables unset, and retain the existing no-producer seal. The actual handler must retain unbound, pinned-multiple-law refusal, unpinned configuration refusal and pinned-sole positive controls. Its parser cache must retain unchanged Arc reuse, changed source, original immutability, malformed-source and missing-source refusal. Do not add another implementation or weaken missing-selector admission. Close the issue only when the landed code and fresh acceptance evidence are established; full corpus qualification remains separate.

## Task 5: Address the verified rational review finding

CodeRabbit review 5557761150 correctly identified the remaining rational helper trim. OWL 2 section 4.1 requires the fraction lexical grammar without whitespace, and owl:real provides no direct lexical forms. The native Rational FromStr implementation already owns that exact grammar. Route owl:rational directly to it; retain owl:real as a value-space membership target while declining to invent lexical values for that IRI. Delete the now-unused local rational helper, decimal retry and production decimal constant. Add positive exact cross-datatype equality/membership controls, malformed/whitespace/decimal-spelling refusals and real-literal refusals, including the actual enumeration consumer. Regenerate the source-bound tiny attestations and re-run the affected synthetic datatype/program/identity/witness checks, the complete selector-free slicetest selection, strict clippy and normal hooks. No parallel parser or compatibility alias remains.
