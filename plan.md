# Authored finance callable-slot metadata

## Actual source chain and defect

Stage-mappings discovers the source SliceCatalog in mappings.rs:180 and calls
correspondence_lower::lower_all. Its merge_ontology:382 reads ontology/gmeow.ttl
plus Module artifacts; merge_slice_artifacts:335 parses their resident source
bytes directly. Pinned substrate crates/slice/src/catalog.rs:419 reads those
artifact bytes from disk. No source metadata enrichment occurs before
lower_all:147 invokes FnO on that ontology view.

The existing five finance CallParameter URNs and two CallResult URNs had no
label, definition or owner. FnO signatures.rs requires each slot's label and
definition and subsequently its owner; generated annotation completion occurs
after admission and cannot repair the authored gap. The prior hosted failure
was the selected extension refusal in run39585407453, generation(a)119190698964
and fixture-prefix119190699094; the metadata refusal is independently established
from the current source chain, not a new observed hosted execution.

## Bounded repair

Add rdfs:label and skos:definition in x-gmeow-english plus the finance
rdfs:isDefinedBy IRI to these existing identities only:
simple:principal, simple:rate, simple:count, return:opening, return:closing,
simple:result, return:result (all under urn:gmeow:finance:).

Definitions distinguish parameter definitions, mathematical symbols, actual
bound quantities and derived result roles. Preserve every computational, ABI,
AST, list and identity triple. Pair the fourteen English literals with French
and Chinese PO entries using exact URN|predicate contexts and matching msgids.
No floor is edited without actual measurement. Principles4,6,7,17,19 apply.

Add pure FnO fixture controls for missing label/definition/owner on one input
and output (six cases), and actual-path preservation of existing authored slot
metadata from each source view. Tests use only the independently authored
CALLABLE_FIXTURE; they never load, compile or produce canonical modules/corpus.
No projection fallback, metadata generator or skipped gate is added.

## Acceptance limits

Source-only handoff. Root owns formatting, focused tests, i18n checks, strict
checks and final hooks. Actual slice metric/floor measurement, generated FnO,
hosted pipeline, full required gates and installed finance acceptance remain
pending under the campaign exception. All original1894/1717 acceptance remains
mandatory; this repair does not establish full finance completion.
