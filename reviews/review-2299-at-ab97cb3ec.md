PASS at ab97cb3ec

# Review: issue 2299 acting-group controls, recheck

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2299-identify-the-e8-group-action-actor`,
  branch `paudley/2299-identify-the-e8-group-action-actor`, head `ab97cb3ec08d5666eb083e25f56a8969ab0392a9`
  (parent `04bd9a39f`), tree clean.
- `origin/main` is `c0a3734f8a2447a1289ab1a8babb1e7babd4f436`, unchanged since the first review and
  still the merge base. Whole branch: 2 commits, 13 files, 471 insertions, all under
  `slices/grounding/math`.
- This verdict stands for the whole branch diff against `origin/main`. It rests on the first review
  (`review-2299-at-04bd9a39f.md`) for everything outside the delta, and on the delta read here.
- Read-only. No producer, gate, harness, reasoning or mutation run.

## Delta 04bd9a39f..ab97cb3ec

Three files, 4 insertions and 3 deletions, nothing else:

- `tests/structural.ttl`: one line added to each of `ex:saGroupActionActorIsAGroup` and
  `ex:saGroupActionIsNotItsActor`, `PREFIX l: <https://blackcatinformatics.ca/logic/>`.
- `tests/counter-examples/acting-group-functional.ttl`: the record's subject changes from
  `math:actingGroupFunctionality` to `[]`.
- `design/MATHEMATICS-ALGEBRA.md`: the two-line sentence under the controls table is replaced.

## Open findings

### R1. Closed

Both patterns now declare `l:` before `ASK`, so every prefixed name in them resolves
(`purrdf-sparql-algebra-4.0.0/src/parser.rs` 1630-1656). Worked by hand with the declaration present:

- `saGroupActionActorIsAGroup`: over the module and examples every authored actor has a declared type
  that reaches `math:Group` (`math:WeylGroup logic:subClassOf math:Group`, `math:AutomorphismGroup
  rdfs:subClassOf math:Group`, or `math:Group` by the zero-length path), so the ban does not hold. Over
  module plus `group-action-actor-wrong-kind.ttl` the actor is typed `math:RootSystem` only, whose
  subclass links lead to restriction nodes, so the ban holds and the witness trips it.
- `saGroupActionIsNotItsActor`: no authored descriptor is typed under `math:Group` or names itself, and
  `math:GroupAction` has no named superclass. Over module plus `group-action-typed-as-group.ttl` the
  descriptor is also a `math:WeylGroup` and names itself, so two branches hold.

### R2. Closed

The sentence now reads: the four actor conformance cells cover the generated validation view for a
missing, a repeated and a non-Group actor; a different actor on the named E8 record and a law named by
one link only are well formed to that view by design; none of the controls proves an action law. Each
part is true of the files: the three violating cells and one conforming cell in
`tests/example-conformance.ttl`; `math:GroupAction` carries only the all-values, at-most-one and
closed-path actor obligations (module 12896-12901) and no law-link obligation; a framed subgroup
satisfies `logic:allValuesFrom math:Group`. See O6 for one word.

### O3. Taken

The functional counter-example is a blank-node record of the form the repository authors
(`[] a logic:PropertyCharacteristicAssertion ; logic:characterizes … ; logic:characteristicSort
logic:functionalProperty`). The first branch of `saActingGroupHasNoGlobalAxiom`
(`?record l:characterizes m:actingGroup ; l:characteristicSort ?sort`) matches a blank-node subject, so
the ban still holds over module plus fixture.

### O4. Taken

`run-checks.sh` line 34 now derives the floors status from the two per-revision status lines, and
`r2-floors.log` records it.

## Syntax of the thirteen patterns

Prefixes. I extracted declared and used prefixes for all eleven structural patterns and both `.rq`
files by reading them; each uses only what it declares, which agrees with `r2-prefix.log`.
`prefix-check.sh` reaches the right answer for these thirteen but has a blind spot: its IRI-stripping
`sed 's/<[^>]*>//g'` also removes the inside of a triple term, so the `m:` in
`<<( ?s m:actingGroup ?o )>>` is not seen as a use. `m:` is declared in that pattern, so the result is
unaffected.

Grammar, read against the pinned parser and existing patterns that run on main:

- Bracketed alternatives with `*` and `+` (`(rdfs:subClassOf|l:subClassOf)*`, `…+`): the parser's own
  tests build `?s (a|b|…)+ ?o` and `rdfs:subClassOf*` (parser.rs 5929-5990, 6204-6212). An unquantified
  bracketed alternative with a constant subject (`m:actingGroup (rdfs:domain|l:domain|rdfs:range|l:range)
  ?kind`) has the same form as `(logic:body|logic:negatedBody) ?atom` in
  `slices/core/inquiry/tests/structural.ttl` 395.
- Triple-term pattern `?atom rdf:reifies <<( ?s m:actingGroup ?o )>>`: parser.rs 6099-6112 tests
  `<<( ?s ?p ?o )>>` in object position; a constant predicate inside is used at
  `slices/core/inquiry/tests/structural.ttl` 525.
- `VALUES ?link { … }` as a group element followed by a braced union: VALUES is a group element
  (parser.rs 3029, 3205); the union arms that do not mention `?link` join with it harmlessly for an ASK.
- `FILTER NOT EXISTS { … }` and `FILTER(?a != ?b)` after a terminated triple block; consecutive filters
  without dots. The parser is strict about dots (7980-7996); every triple block in the thirteen is
  terminated before the next block, filter or brace.
- Blank-node property lists in object position and `[]` as a subject, object lists with variables,
  `OPTIONAL`, ASK without WHERE: ordinary forms, the same as patterns already on main in this file.
- No BIND, sub-select, labelled blank node, HAVING or MINUS, so the parser's scope rules are not engaged.

I found no construct in the thirteen that the pinned parser would refuse.

## Evidence

`r2-lint.log`, `r2-rapper.log`, `r2-msgid.log`, `r2-prose.log` and `r2-floors.log` are headed with the
worktree path, `ab97cb3ec…` and an empty porcelain, written after the commit time, each with exit 0.
`r2-prefix.log` has the path and head but no porcelain line. `r2-commit.log` is the hook output for the
commit, all hooks passed or skipped for want of files.

## Required findings

None.

## Optional findings

- O1 (unchanged). `saE8WeylActionKeepsBothLawLinks` and `cqE8ActingGroup` overlap
  `saE8WeylActionRecord` on the E8 record's actor and law links.
- O2 (unchanged). The rule branch of `saActingGroupIsNeverDerived` matches `?rule a l:Rule` only and
  has no witness of its own; the fixture exercises the chain branch.
- O5 (unchanged). The failure cases named in the two competency rationales and the table's "What fails
  it" column are hand-worked, not exercised by a cell.
- O6. "so only these declared-data controls catch them" follows the wording I proposed, and "only" is
  loose: the native question in `structure-preserving-maps.rq` also returns no row for a different actor
  or a dropped law link, as the charter's own lines 126-130 say. The contrast intended is with the
  validation view. "so the validation view does not catch them" would be exact.
- O7. The replaced sentence is one unwrapped line in a file wrapped at 100 columns; the lint passes.
  `prefix-check.sh` writes its scratch table under `/tmp` rather than `.stage/`, and see the blind spot
  above.

## Pull request 2550

Not open: merged 2026-11-05 as `c441bfce55033cf9c9ab66e92299ef74de20350d`, an ancestor of
`origin/main`, and the commit the issue's addendum cites as the source delivery. It carries the
representation (`math:actingGroup`, the three class-local obligations, `math:MalformedGroupActionActor`,
the actor link on `exa:e8WeylAction`, the four actor cells, the catalog entries). This branch builds on
it and neither duplicates, conflicts with nor is superseded by it. No term or data change is owed.

## Closing question

The issue stays open by its own text "for actual actor-boundary and exact-E8/law-link controls over
permitted authenticated inputs" and asks to "Verify" them; its addendum says no successful control is
claimed. This delivery supplies the controls and, by reading, they are well formed. It does not supply a
run. The issue should not close on the merge alone: it closes when a producer pass records the math
slice's structural, competency and conformance specifications green with these cells. It does not need
the validation view to reject the wrong-actor or law-link inputs, which it accepts by design.

## Not verified

- No execution of any query, assertion, fail-witness or conformance cell; parsing and evaluation are by
  reading the patterns and the pinned parser's source and tests. Execution evidence is not obtainable in
  this session by the operator's ruling.
- That `examples/a2-acting-groups.ttl` validates with no findings against the generated shapes, and that
  a plain integer satisfies the rank obligation; by precedent only.
- GMN-1 coverage of the new example through the codec.
- That the reader-view projection accepts the two axiom-bearing fixtures without error.
- `prose-check` rerun by me; the coder's `r2-prose.log` records exit 0 at this head.
- Hosted CI state for the branch.
