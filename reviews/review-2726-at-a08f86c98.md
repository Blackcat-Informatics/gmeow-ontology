# Delivery review: evaluating a claim that exists as a reified statement

- Head reviewed: `a08f86c98bc5e03d890b8674087cba47e8327526` (`git rev-parse HEAD` in
  `.worktrees/2726-evaluate-a-reified-claim`, branch `paudley/2726-evaluate-a-reified-claim`)
- Base: `0ee2d5479b612d708a7474d9668f10ad096234ba` (`git merge-base origin/main HEAD`); one commit.
- Review started 2026-11-14T20:36:48-06:00; written 2026-11-14T20:52:47-06:00 (`date -Is`).
- Mode: read-only. No make, cargo, build or test runs. Ran `prose-check` on the docs, example,
  counter-example and commit message: exit 0, no output
  (`.stage/reviews/prose-2726-docs.log`, `.stage/reviews/prose-2726-other.log`).

## Scope

Requirement: the last comment on the issue (maintainer decision), checked against the issue body
and the last comments on the two related issues (correction is not suppression; display is reader
policy). Rules: AGENTS.md, CONSTITUTION.md Principles 2 to 6, 9, 10 and 12 (read in full);
15, 16 and 17 read through AGENTS.md section 2 and the derive code, not the constitution text.

## Coverage

| File | Lines | Commit | Read |
|---|---|---|---|
| slices/core/epistemics/module.ttl | diff hunks; 34-60, 493-510, 684-697, 855-936, 1189-1203, 1264-1278 | a08f86c98 | yes |
| slices/core/epistemics/module.ttl | everything else (most of about 1,300 lines) | a08f86c98 | NOT read |
| slices/core/epistemics/examples/evaluating-a-reified-statement.ttl | all | a08f86c98 | yes |
| slices/core/epistemics/tests/counter-examples/evaluates-statement-reifier.ttl | all | a08f86c98 | yes |
| slices/core/epistemics/tests/example-conformance.ttl | all | a08f86c98 | yes |
| slices/core/epistemics/tests/competency.ttl | 1-20, new block 384-414 | a08f86c98 | yes; older cells NOT read |
| slices/core/epistemics/tests/structural.ttl | 1-20, 420-468 | a08f86c98 | yes; older cells NOT read |
| slices/core/epistemics/docs.md | new section 185-243 | a08f86c98 | yes; the rest NOT read |
| slices/core/epistemics/i18n/fr.po, zh.po | new entries 507-575 | a08f86c98 | yes (French in full; Chinese checked for structure and terms) |
| slices/core/epistemics/manifest.ttl | all | a08f86c98 | yes |
| slices/core/standpoint/module.ttl | 80-100, 122-130, 806-810, 1138-1230 (definitions), 1455-1500 | a08f86c98 | yes |
| slices/core/record-metadata/module.ttl | 610-622 | a08f86c98 | yes |
| slices/core/refinement-cues/module.ttl | 1595-1650 | a08f86c98 | yes |
| crates/logic-compile/src/frontend.rs | 1510-1560, 2365-2424, 2676-2860, 3020-3060, 3633-3690, 3805-3990, 4150-4215 | a08f86c98 | yes; the rest NOT read |
| crates/slicetest/src/exec.rs | 140-300, 1380-1520 | a08f86c98 | yes; `run_conformance_cell` body, `NativeChannelBaseline` NOT read |
| crates/slicetest/src/stores.rs | 1-68 | a08f86c98 | yes |
| crates/slicetest/src/paths.rs (`conformance_module_files`) | none | a08f86c98 | NOT read |
| crates/pipeline/src/stages/statements.rs | 320-390 | a08f86c98 | yes |
| crates/logic/src/provenance.rs | 420-490 | a08f86c98 | yes |
| crates/gmeow-dev-cli/src/dev_shapes.rs | 2525-2564 | a08f86c98 | yes |
| purrdf-slice 4.0.0 `claim_view.rs`, `ownership.rs` (cargo registry) | claim_view 1-70; ownership 15-190, 395-470, 735-756 | crate 4.0.0 (Cargo.lock) | yes |
| CONSTITUTION.md | 35-124, 155-266 | a08f86c98 | yes; 15, 16, 17 sections NOT read in the constitution text |
| .stage/issue-2726/*.log | all | n/a | yes (lint, msgfmt, rapper, commit hook logs) |

## Evidence the author recorded

`.stage/issue-2726/`: `lint.log` EXIT=0 (pre-commit over all files), `msgfmt` fr and zh EXIT=0,
rapper parse of module and tests EXIT=0, and of the two triple-term files only after replacing the
triple term with a placeholder IRI. No producer pass, no `make validate`, no slice-spec run,
no conformance or competency execution. Nothing has run the example, the counter-example, the
competency question or the structural cells.

## Findings

### F1 REQUIRED: the functionality rationale is false for reifiers GMEOW itself mints, and the guidance omits the precondition

`slices/core/epistemics/module.ttl:690` (definition), `:1269` (functionality record definition),
`docs.md:208-209` (table), and the matching fr/zh msgstr entries.

The definition says "a reifier names one occurrence of a statement, attributed to one source on one
occasion ... the same triple put forward by another source or on another occasion has its own
reifier". This is stated as fact, but it does not hold for the repository's own reifiers:

- `crates/pipeline/src/stages/statements.rs:361-364`: a `gmeow:StatementMetadata` cell with no
  authored `gmeow:reifier` gets `mint_reifier(&triple)`. That reifier depends on the triple alone
  (`crates/logic/src/provenance.rs:444-450`, `sha1(s p o)`). Two cells for the same triple from
  two sources therefore share one reifier, which carries both sources' annotations.
- The explanation and provenance engine works the same way (`provenance.rs:470`): the reifier
  is content-addressed, and the MCP surface (`crates/mcp/src/lib.rs:7053-7060`) notes that one
  reifier is shared across worlds.

Consequence: a user who follows the howToUse on such a reifier and links each source's token
gets two `putForwardByClaim` values on one reifier. The derived property-scoped
`gmeow:putForwardByClaim-domain-shape` (maxCount 1) rejects that, or the native coherence gate
reports it. The user can link only one source's act, so the other source's claim cannot be
evaluated by this route. The text never says how to avoid this.

The functionality record's reason is also inverted: "so an evaluation of the token judges one
identifiable claim". Functionality runs from reifier to token. Because the property is
deliberately not inverse functional, one token may stand behind several reifiers, so evaluating
that token judges the whole act and every statement linked to it, not one statement. The same
point affects the competency rationale ("which evaluation judges this reified statement"), which
would return one row for each statement of a multi-statement act.

Required correction: state the precondition, not a false universal. The property applies to a
reifier that names one attributed occurrence; give each assertion act its own authored reifier, and
never bridge a content-addressed or shared reifier that carries several sources. Say that
evaluating a token judges the whole act, every statement linked to it. Fix the record's rationale
to match. Update the fr and zh entries with the English. Functionality itself stays defensible:
the statement compiler's own doc comment (`statements.rs:331-336`, "one proposition ... may be put
forward by several claim tokens. Each authored gmeow:reifier becomes its own owl:Axiom node") and
`docs/PIPELINE_SPINE.md:233` ("Separate reifiers preserve coexisting standpoints") support
reading a reifier as one occurrence. The problem is only the text and the missing precondition.

### F2 REQUIRED: the functional record plus `logic:domain rdf:Statement` mints a derived shape in the W3C RDF namespace

`slices/core/epistemics/module.ttl:686` and `:1264-1270`, read with
`crates/logic-compile/src/frontend.rs:3843-3857`.

FAMILY 3 of `derive_validation_shapes` puts the functional cap both on the property-scoped
`SubjectsOf(P)` shape and on the class node shape of each `logic:domain` class.
`classify("rdf:Statement")` returns `ConstraintComponent::Class` (frontend.rs:2796-2810), and that
loop has no `is_authoring_ns` guard. Every other family applies one (for example 3075, 3111, 3163,
3213, 3953-3958). `entry_for` names the shape `{c}-shape` (3047). The derive therefore emits
`<http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement-shape>`, with `sh:targetClass rdf:Statement`
and `sh:maxCount 1` on `gmeow:putForwardByClaim`, into the shipped SHACL projection. Nothing
downstream filters it (`acc` consumption at 4187-4215).

This is the first functional property in the repository whose domain is outside the authoring
namespaces. The seven `rdf:Statement`-domain properties in refinement-cues have no characteristic
records, which I checked one by one. So this commit is what first triggers the gap. It breaks the
invariant the code documents ("the dogfooding guard every family applies", frontend.rs:3953;
"The derive only emits validation shapes for classes/properties in an authoring namespace",
dev_shapes.rs:2550-2554). It also mints a term in a namespace GMEOW does not own (Principle 5:
never rewrite anyone else's data).

Required correction: guard the class-scoped functional and inverse-functional caps
(frontend.rs:3845 and :3866) with `is_authoring_ns`. The property-scoped
`putForwardByClaim-domain-shape` already enforces the bound on every subject, typed or not. This is
in-scope ownership of a bug the change exposes, not scope growth. Dropping the domain would lose
the "what it relates" acceptance item.

### F3 OPTIONAL: the retained "suppress the old evaluation with displayable false" sentence

`module.ttl:45` (ClaimEvaluation howToUse) and the new fr and zh entries, which re-translate it.
The related display decision says no data may set `displayable false` because a claim is
disputed, corrected or evaluated, and it assigns display consequences to that work, including
"the guidance on the claim relations". An evaluator superseding its own evaluation is a self-revision
(Principle 10 revision), not another party's dispute, so the sentence does not plainly contradict
that decision. The decision for this issue says "Display consequences belong to" the display
work. Judgment: this sentence belongs to the display work, and this pull request need not change
it. The display work must pick it up, together with the ClaimToken howToUse ("Retract by
suppression (gmeow:displayable false)", module.ttl:57). The new avoidWhen and docs text
(evaluation never suppresses, ranks or replaces the evaluated claim; display is reader policy at
projection) is true and does not conflict with it.

### F4 OPTIONAL: refusal is closed-world only

`gmeow:evaluates` keeps `rdfs:range gmeow:ClaimToken` (module.ttl:498). On asserted data the
counter-example is refused by `ClaimEvaluation-shape`. A reasoner instead types a reifier that
`evaluates` points at as `ClaimToken` and `Activity`, which is the retyping the decision forbids,
and no disjointness axiom flags it. The acceptance asks only that validation refuse it, so this
does not block. The docs could say the refusal is a validation outcome.

### F5 OPTIONAL: `rdf:Statement` as the type of native RDF 1.2 reifiers

The domain axiom types every native `rdf:reifies` reifier that uses the property as an
`rdf:Statement` (the classic reification class) by inference. This follows refinement-cues
precedent (seven properties). It does not reach the two recognizers keyed on `rdf:Statement`. The
classic scoped-axiom reader (frontend.rs:1519-1545) and the FAMILY 5 reifier-obligation reader
(3949-3976) both read the authored source store. FAMILY 5 also needs asserted `rdf:predicate` and
`rdf:subject` plus a typed GMEOW class. So the inference is harmless, apart from F2.

Findings: two REQUIRED (F1, F2), three OPTIONAL.

## Answers

1. **The property.** It is authored in canonical form: `logic:ObjectProperty`, `logic:domain`,
   `logic:range`, a `logic:PropertyCharacteristicAssertion` for functionality, and label,
   definition, isDefinedBy, example, scopeNote, useWhen, avoidWhen, howToUse and graphBoxRole.
   Neighbours still use legacy `rdfs:domain`; the new term correctly does not.
   - Meaning: it relates a reifier to the assertion act (ClaimToken), keeps `gmeow:expresses`
     separate (scopeNote), and does not retype the reifier. It agrees with ClaimToken ("one source,
     on one occasion"), with the standpoint slice (lifecycle relations act on reifiers directly with
     domain `gmeow:Entity`, so retyping a reifier as an Activity would be the wrong move), and with
     the observations union view (purrdf claim_view builds StandpointClaim from tokens that
     `expresses`).
   - Duplication: none found. I searched every slice for ClaimToken use (none outside
     epistemics), for properties with domain `rdf:Statement` (refinement-cues measurements only),
     for standpoint reifier vocabulary (`accordingTo`, `heldIn`, lifecycle relations), and for
     `statedRationale`. Nothing else relates a reifier to an assertion act.
   - Functionality: right in substance, wrong in its stated justification (F1). A violation in data
     looks like two `putForwardByClaim` values on one reifier, which the property-scoped shape
     rejects with maxCount 1. The likeliest real cause is a content-addressed reifier shared by two
     sources.
   - Domain: see F5 (harmless inference) and F2 (the foreign-namespace shape).
   - Name: `putForwardByClaim` reads correctly from reifier to token and matches the slice's
     `*Claim` naming for ClaimToken objects (`producesClaim`, `attacksClaim`, `supportsClaim`). No
     inverse name is minted, which is fine.
2. **Reachability.** Following the example: `ex:skyStatement rdf:reifies <<( ex:sky ex:hasColour
   ex:blue )>>`, then `ex:skyStatement putForwardByClaim ex:aliceSaysBlue`, then `ex:bobEvaluation
   evaluates ex:aliceSaysBlue`, with `statedRationale "No, it is red. I checked at sunset."@en` on
   the evaluation. The competency question runs on the asserted lane (modules only,
   stores.rs:6-9) with the example overlaid. Hand evaluation gives exactly one row, matching the
   expected cells. No module triple has `putForwardByClaim` as a predicate, and the counter-example
   is not overlaid, so no extra rows appear. The `rdf:reifies <<( ?s ?p ?o )>>` pattern matches
   working queries in places/tests/competency.ttl:144,172. Lang-tagged expected literals match
   precedent (`"..."@en` cells in other slices). The `logic:` prefix is declared
   (competency.ttl:14).
3. **Counter-example.** `ClaimEvaluation logic:subClassOf [allValuesFrom ClaimToken on
   evaluates]` (module.ttl:898-899) derives `sh:class gmeow:ClaimToken` on
   `gmeow:ClaimEvaluation-shape` (FAMILY 1 plus `{c}-shape`, frontend.rs:3047). The ClosureEntry at
   :900 is class-scoped, so it adds minCount 1 and no `evaluates-range-shape`
   (`closure_keys_with_value` skips entries with `onClass`, frontend.rs:2377). The pinned code and
   shape name are the ones the derive produces. The file is isolated: `evaluates` is present (once),
   the epistemics-owned shapes do not constrain the Standpoint or Proposition nodes, and the reifier
   is untyped. I did not read the full cell-matching body or the native-channel baseline. Only an
   executed run proves that nothing else fires.
4. **Guidance and text.** "Where the reason goes", "verdicts are open" and "an evaluation acts on
   nothing" are true of the module and the decisions. The false or inverted statements are in F1.
   The retained displayable sentence is judged in F3: it belongs to the display work.
5. **Cross-slice use.** The manifest does not list standpoint or record-metadata. The ownership
   analyzer reconciles only semantic edges: Ontology (module.ttl), Shape, Mapping, and Query (the
   `queries/` trees). Test DSL, examples and documentation are non-semantic
   (purrdf-slice ownership.rs:140-187, 740-756). The new module triples reference only
   epistemics terms and `rdf:Statement`. `statedRationale` and `accordingTo` appear only inside
   literals, in the example, the docs and the test DSL. No gate rejects this and no manifest change
   is needed.
6. **Catalogs.** 17 new entries in each of fr and zh: 7 for the property, 7 for the record, 3 for
   changed literals. All 17 msgids match the module literals byte for byte (checked with `grep -F`
   on each msgid against the `"…"@x-gmeow-english` literal). The base catalogs had no entries for
   the three changed literals, so no orphans exist. The French carries every clause. The Chinese
   carries every clause and uses 具体化者 for reifier, the majority catalog term. Two boilerplate
   defects are inherited from the 18 neighbour records and are not new: the useWhen says "names
   exactly one value" where functionality means at most one, and the French has "lorsque une"
   where it should elide to "lorsqu'une". The F1 text changes must flow into both catalogs.
7. **Structural and conformance cells.** The positive structural cell holds by hand: the
   `(logic:domain|rdfs:domain)` alternation, and the record joins `characterizes` and
   `characteristicSort functionalProperty`. The negative cell holds. These cells mostly mirror the
   declaration, but they follow the slice's structural convention, and the negative cell pins a
   real decision. The cells use `gmeow:StructuralAssertion` and `gmeow:ExampleConformance` with
   slice-relative paths, the forms `run_structural_file` and `run_conformance_file` discover.
8. **Commit message.** Its claims are accurate apart from inheriting F1's rationale ("a reifier is
   one attributed occurrence of a statement, so one assertion act stands behind it"). It contains
   no issue numbers. The diff adds no process references (grep of added lines).

## What only a producer pass can prove

- Both conformance cells: that the example validates with no findings, and that the
  counter-example raises only `shacl.ClassConstraintComponent` from `gmeow:ClaimEvaluation-shape`.
- The competency question's exact row and the structural verdicts under the real harness.
- Purrdf parsing of the two triple-term files (the author's rapper run substituted a placeholder).
- Whether `rdf:Statement-shape` (F2) actually appears in generated SHACL, ShEx and
  model projections, and whether any gate trips on it.
- `make validate` annotation completeness and the native coherence gate over the merged ontology.

VERDICT: BLOCKED at a08f86c98bc5e03d890b8674087cba47e8327526
