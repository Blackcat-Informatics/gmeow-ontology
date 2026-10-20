# Issue #1746: logic: non-entailment obligation discharge misses logic:instanceOf-typed obligations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic

## Body

Found while wiring a consistency gate over the Principia corpus (lillith_soul/principia), which authors pure `logic:` GMEOW.

## Problem

`crates/logic/src/obligations.rs` discovers obligations with:

```sparql
SELECT ?o ?pred ?cond WHERE {
  ?o a logic:NonEntailmentObligation ;
     logic:obligationForbiddenPredicate ?pred .
  OPTIONAL { ?o logic:obligationDischargeCondition ?cond } }
```

But a `logic:`-native corpus types individuals with `logic:instanceOf`, not `rdf:type` — and the grounding module deliberately keeps the two distinct (HiLog deep instantiation; `gmeow:avoidWhen "Do not replace ordinary rdf:type indiscriminately"`). Principia authors 353 obligation individuals, all but 2 via `logic:instanceOf`; the discharge checker sees almost none of them. The check silently passes with ~99% of the corpus's obligations invisible.

## Proposed direction

Accept either spelling in the discovery query (a `UNION` over `a` / `logic:instanceOf`), or normalize `logic:instanceOf logic:NonEntailmentObligation` into the store view the checker queries. A count of discovered obligations in the report would also make this class of silent miss visible ("0 obligations found" should look suspicious to a caller who authored 353).

## Workaround in use

None good — the consuming corpus would have to double-type every obligation with `a`, which fights the grounding doctrine.

## Approved integration under #1704

This report remains current implementation scope. Preserve the fixes on `agent/fol-tptp-export`; its branch still lacks the obligation-loader repair described here. The preserved C1 integration now implements native obligation and candidate discovery through both structural typing surfaces, deduplicates identical subjects while retaining document-scoped blank identity, discovers declarations before validating their required fields, rejects malformed/conflicting values, and reports the selected default-graph inventory including zero. All five obligation-discovery controls passed in the recorded 804/804 shared-typing batch.

That local implementation is unpublished and unmerged; complete producer/gate acceptance remains pending. Exact authored/admitted/discharged coverage and context-bound receipts remain required under #1748/#1747. Foundation-only reachability is still open under #1747. The acceptance requirements below remain in force.

- Discover canonical `logic:instanceOf` and supported `rdf:type` spellings through the shared native reader, deduplicating genuinely identical declarations while preserving graph/standpoint/source identity. Do not equate the two predicates globally.
- Report authored, discovered, admitted and discharged obligation identities/counts. Discover declarations before validating required fields, so malformed or missed obligations cannot disappear into a successful empty result. Apply the same admitted structural typing to candidate attribution.
- Verify logic-only, RDF-typed, dual-typed and mixed-document obligations, malformed declarations and context isolation with GMEOW product fixtures. Preserve existing discharge policy.

Blocks #1747. Full gate evidence remains required for the integrated batch; tests never build the corpus. Principles 4, 7, 9, 17–19.


## Comments (1)

### paudley — 2026-09-08T21:48:50Z

Implementation progress under the approved plan (Principles 4, 7, 9, 17–19): obligation and candidate discovery now read the native dataset indexes and accept structural `rdf:type` and canonical `logic:instanceOf` declarations, deduplicating identical native subjects while retaining document-scoped blank identities. Declarations are discovered before required fields: missing, conflicting, non-IRI and malformed forbidden predicates fail instead of disappearing or choosing a first value. Candidate coverage and source-hash checks use the same admitted structural typing. A visible inventory finding reports the selected graph's discovered identities/count, including zero.

Five focused obligation regressions and the related compiler extraction tests passed as part of an 804-test selection. Repository lint passed. All fixtures are synthetic, authored inputs; tests produced no corpus and PurRDF conformance was not rerun.

This is progress, not full closure: exact authored/admitted/discharged coverage and context-bound receipts still need the shared coverage/reachability integration under #1748/#1747. The current foundation-only reachability check is being replaced; a reachable head will no longer be presented as a witnessed violation. Full gate evidence remains pending for the integrated batch.


