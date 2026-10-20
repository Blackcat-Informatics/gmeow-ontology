# Requirement candidates — issue #1746

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 18 | modal | Accept either spelling in the discovery query (a `UNION` over `a` / `logic:instanceOf`), or normalize `logic:instanceOf logic:NonEntailmentObligation` into the store view the checker queries. A count of discovered obligations in the report would also make this class of silent miss visible ("0 obligations found" should look suspicious to a caller who authored 353). |
| B2 | body | 26 | modal | This report remains current implementation scope. Preserve the fixes on `agent/fol-tptp-export`; its branch still lacks the obligation-loader repair described here. The preserved C1 integration now implements native obligation and candidate discovery through both structural typing surfaces, deduplicates identical subjects while retaining document-scoped blank identity, discovers declarations before validating their required fields, rejects malformed/conflicting values, and reports the selected default-graph inventory including zero. All five obligation-discovery controls passed in the recorded 804/804 shared-typing batch. |
| B3 | body | 28 | modal | That local implementation is unpublished and unmerged; complete producer/gate acceptance remains pending. Exact authored/admitted/discharged coverage and context-bound receipts remain required under #1748/#1747. Foundation-only reachability is still open under #1747. The acceptance requirements below remain in force. |
| B4 | body | 31 | modal | - Report authored, discovered, admitted and discharged obligation identities/counts. Discover declarations before validating required fields, so malformed or missed obligations cannot disappear into a successful empty result. Apply the same admitted structural typing to candidate attribution. |
| B5 | body | 34 | modal | Blocks #1747. Full gate evidence remains required for the integrated batch; tests never build the corpus. Principles 4, 7, 9, 17–19. |
| C1.1 | comment 1 (paudley, 2026-09-08T21:48:50Z) | 1 | modal | Implementation progress under the approved plan (Principles 4, 7, 9, 17–19): obligation and candidate discovery now read the native dataset indexes and accept structural `rdf:type` and canonical `logic:instanceOf` declarations, deduplicating identical native subjects while retaining document-scoped blank identities. Declarations are discovered before required fields: missing, conflicting, non-IRI and malformed forbidden predicates fail instead of disappearing or choosing a first value. Candidate coverage and source-hash checks use the same admitted structural typing. A visible inventory finding reports the selected graph's discovered identities/count, including zero. |

## Comments that matched no rule

None.
