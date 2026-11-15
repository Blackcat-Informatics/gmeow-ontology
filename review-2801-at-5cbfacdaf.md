PASS at 5cbfacdaf

# Delivery review: issue 2801, source snapshot (recheck)

## Scope

- Head `5cbfacdaf` on branch `paudley/2801-term-source-snapshot-a-retrieved-copy` (worktree
  `.worktrees/2801-term-source-snapshot-a-retrieved-copy`). Its parent is `3eba5c130`, which I reviewed
  in `review-2801-at-3eba5c130.md`.
- I re-fetched `origin/main` for this recheck. It is still `862638ad8`, and the merge-base is still
  `1198be05c`. The one commit main gained since the base touches no file in sources, creative-works,
  citations, documents, organization or kernel.
- The branch diff against `origin/main` is 8 files, 374 insertions and 1 deletion, all under
  `slices/core/sources/`. It is exactly the `3eba5c130` diff I reviewed plus this delta: 6 files,
  11 lines changed each way.
- This verdict covers the whole branch diff.

## Evidence

- `git diff 3eba5c130 5cbfacdaf`, read in full.
- `gmeow:foundedBy` (organization slice) re-read. Its rdfs:domain is `gmeow:Organization` and its
  rdfs:range is `gmeow:Agent`, defined as "Relates an organization to an agent who founded it". Its
  howToUse says contested claims are carried with `gmeow:accordingTo`.
- `gmeow:Agent` (kernel slice) re-read. It is a Category. Its avoidWhen says to avoid "asserting bare
  gmeow:Agent when Person or Organization is meant".
- msgid check against module.ttl by subject and predicate, run on both catalogs: zero mismatches, and
  28 entries in each. The fr and zh SourceSnapshot skos:example msgid and msgstr are byte-identical to
  each other and to the new module literal.
- `rapper -q -i turtle -c`, serialized with `flock .stage/build.lock`: module.ttl EXIT=0,
  tests/competency.ttl EXIT=0. The example parses with EXIT=0 once its one RDF 1.2 triple term is
  substituted, because raptor does not support RDF 1.2.
- Catalog scope for test rationales: 18 `cqRationale` literals tagged `@x-gmeow-english` already exist
  across slices, none has a catalog entry, and the two existing tagged rationales in this same file
  have none either. Tagging the new rationale therefore matches repository practice.
- The coder's `r2-2801-checks.log` (rapper, msgid count, prose-check, `make lint` all EXIT=0) and
  `r2-2801-commit.log` (commit EXIT=0) were read. I compared `r2-hand-eval-2801.md` only after doing
  my own evaluation.

## Prior required findings

- **R1: resolved.** The claim is now `ex:acme gmeow:foundedBy ex:founder`.
  - `ex:acme a gmeow:Organization` satisfies the domain, and `ex:founder a gmeow:Agent` satisfies the
    range.
  - The use matches the definition: the flat founder fact, with no date claimed, which is what the
    term's avoidWhen reserves the Event form for.
  - The quote is now "Acme was founded by Alice Moreau". The claim states the founding-by relation the
    quote supports.
  - `gmeow:accordingTo` carries the standpoint, as foundedBy's howToUse prescribes.
  - The expected quote cell in `tests/competency.ttl` was changed to the same literal.
- **R2: resolved.** The SourceSnapshot `skos:example` (module.ttl:59) now carries `contentDigest`
  (64 lowercase hex digits, typed blake3), `retrievedAt`, `sourceLocation` and `immutableAddress`,
  plus `exemplifies`. That covers all four closed-world required paths, and the digest passes
  `ContentDigestValueConstraint`'s pattern. The fr and zh entries match the new literal byte for byte.

## Competency cell, re-worked by hand

- The example still holds one CitationAct with `citingEntity case:foundedClaim`, `citedEntity
  case:pageSnapshot` and `viaSelector case:foundedQuote`.
- The quote is the plain literal "Acme was founded by Alice Moreau".
- The snapshot's digest, retrievedAt, live address and immutable address are unchanged, one value
  each.
- The query therefore returns exactly 1 row, and its six cells equal the expected row term for term.
- The three structural assertions read module.ttl only. The delta changes no subclass, domain or
  closure triple, so their results are unchanged: must holds, mustNot does not hold, must holds.
- My results agree with the coder's r2 hand evaluation.

## Optional corrections taken

- **O2: correct.** docs.md now reads "Depends on `documents` and `creative-works` (the WEMI carrier tiers
  …)". documents owns the CreativeWork umbrella; creative-works owns Manifestation and Item. Like the
  original sentence, it still names only the carrier-tier dependencies, not math, logic, kernel or
  teleology.
- **O7: correct.** The new `cqRationale` now carries `@x-gmeow-english`, consistent with the two
  existing rationales in the file and with practice across the repository.

## Findings

Required: none.

Optional findings left open, or new. I re-judged each one for truth, and none states anything false:

- O1. `docs/import-provenance.md` and `docs/temporal-queries.md` still list four clocks. The new term
  calls retrieval time a fifth. This is incomplete, not false: those documents do not claim the list
  is exhaustive against carrier-level times, and the slice docs.md frames retrieval time as outside
  the four-clock model.
- O3. The `retrievedAt` definition says record time is "as in gmeow:recordedNoLaterThan or
  gmeow:ingestedAt". That is loose wording, since recordedNoLaterThan is a derived upper bound on
  record time, but it is not false. The definition also names no properties for valid time or
  assertion time.
- O4 and O5 stand unchanged:
  - The structural assertions do not pin the ranges or the allValuesFrom halves.
  - The ban has no fail-witness.
  - The competency question projects neither the citation node nor the exemplified page.
- O6 stands unchanged. The `immutableAddress` text does not say that a content-addressed address is no
  identity key for a snapshot, and does not distinguish the property from `gmeow:storedIn`.
- O9 (new). The worked example types `ex:founder` as bare `gmeow:Agent`, but the quote names a person.
  `gmeow:Agent`'s avoidWhen asks for `gmeow:Person` when a person is meant. Nothing stated is false,
  since a Person is an Agent. Typing `ex:founder` as `gmeow:Person`, and naming the node after the
  founder, would follow the term's own guidance and make the link to the quoted name evident.
- O10 (new). With a full digest now in the SourceSnapshot `skos:example`, its content-addressed
  `immutableAddress` path (`…/blake3/5f8d1e2a`) visibly uses a truncated digest. A path derived from
  the full digest would read more consistently with "must never resolve to other bytes".
- O8 is outside this branch. The 2802 example's `gmeow:foundedOn` use should be checked for the misuse
  R1 found here.

## Not verified

- No execution evidence exists for the competency cell, the structural assertions, the ownership and
  dependency analysis, the annotation and catalog gates, or the derived shapes of the four required
  paths. Per the operator's ruling, no producer, gate, harness or reasoning run is permitted in this
  session, and hosted CI cannot complete the producer pass. All of the above was checked by reading
  and parsing only.
- The production RDF 1.2 parser was not run on the example's triple term. Its form matches 977
  existing uses.
- I did not confirm from code which slice files the i18n catalog coverage gate scans for test-DSL
  literals. I relied on repository precedent: tagged rationales with no catalog entries are already on
  main.
- French and Chinese fidelity of the changed example entries was judged by reading. Those entries are
  Turtle notation, kept verbatim.
