# Review of issue 2727 at 5eb1cf255

Branch `paudley/2727-displayable-is-not-scoped-to-a`, head
`5eb1cf255046998a037995273c0f8e5863e7591d`, nine commits on `origin/main`
(`279ce7e40`, which is also the merge base). Worktree clean. Review only; nothing in the
worktree was changed.

## Scope

Read in full: issue 2727 with its three comments (blocker note, maintainer decision with
four acceptance points, scope comment listing slices); the whole diff
`origin/main...HEAD` (39 files); the worktree `AGENTS.md`; the changed constraints and
their neighbours in the names, lifecycle, events, observations, standpoint, gender,
epistemics and accessibility modules; `docs/claim-lifecycle.md`; the standpoint Rust case
file and the `Case` harness (`crates/validate/tests/conformance_support/mod.rs`); the
constraint target derivation (`crates/logic-compile/src/ir/constraint.rs:714-830`); the
structural-assertion executor (`crates/slicetest/src/exec.rs:600-700`); the coder's
`hand-eval.md`, `checks.sh`, `checks.log`, `floors.log` and the unused
`msg-observations.txt`; `/home/paudley/.claude/skills/rust-work/SKILL.md`.

## Evidence

- No gate, producer, reasoning or test run exists for this head. The coder's
  `hand-eval.md` says nothing was executed. `checks.log` holds `rapper -c` (20 files,
  all 0), a line-based msgid scan, `prose-check` (exit 1) and `make lint` (exit 0).
  Every behavioural claim below is from reading source, not from execution.
- Commands I ran: read-only `git`, `grep`, `jq`, `rapper` to standard output, and
  `prose-check` (exit 1, one finding, see item 10).

## Findings

### REQUIRED

**R1. The two deletions do not change what the validator reports; the gender constraint
still prompts `displayable false` on every node whose validity ended.**
`gmeow:SupersededGenderIdentitySuppressionConstraint`
(`slices/core/gender/module.ttl:425-442`) has the guard `validUntil(this, u)` and no type
atom. `target_from_integrity` (`crates/logic-compile/src/ir/constraint.rs:806-814`) lowers
a guard with no `rdf:type` atom to `sh:targetSubjectsOf gmeow:validUntil`. Its focus set
is therefore every subject of `gmeow:validUntil`.

- Deleted `SupersededObservationSuppressionConstraint` fired on
  `type(this, Observation) and validUntil(this, u)` without `displayable(this, false)`.
- Deleted `SupersededParticipationSuppressionConstraint` fired on
  `type(this, Participation)` with a `validUntil` and without `displayable(this, false)`.
- Both focus sets are subsets of the gender focus set, and the consequent is the same.
  The coder's coverage argument is therefore true: no wanted case is lost. It is true
  for the same reason the change has no effect: every unwanted case is kept as well. A
  measurement whose validity ended, or a participation another party corrected and
  closed, still draws a warning, now worded "A superseded identity facet ... should set
  gmeow:displayable false".
- The coder saw this (`hand-eval.md`: "the gender constraint still reaches those nodes")
  and left it. Acceptance point 1 ("the observation constraint no longer fires merely
  because a validity interval has ended"; "None may fire because one claim disputes or
  corrects another") is met by name only.
- The gender formula is also wider than its own definition, which names four facet
  kinds. All four are subclasses of `gmeow:IdentityFacet`, so a typed guard exists; the
  existing data-only case `suppression-warning-only` types its node `gmeow:GenderIdentity`
  and must keep its warning.
- Text made false by this: `slices/core/events/tests/structural.ttl:873` and
  `slices/core/observations/tests/structural.ttl:475` (rationales), and
  `tests/fixtures/shapes/participation-wellformed.ttl:3-4` ("displayable false is
  allowed but never required"). The same fixture, lines 22-29, still models a
  "superseded (corrected)" participation named `ex:correctedPart` carrying
  `displayable false`, which is the reading the decision bans; the new comment calls it
  one "its source withdrew".

**R2. The new Rust case cannot fail on the behaviour it names, and acceptance point 3
has no validator-level witness.**
`crates/validate/tests/conformance_cases/conformance_standpoint.rs:56-59` adds
`Case::file("shapes", "standpoint-ended-tenure-conforms")` with no expectation.
`Case::run` (`conformance_support/mod.rs:1482-1486`) then asserts only that there are no
Violations; the harness documentation (lines 1302-1305) says an empty list asserts
nothing about warnings. The removed behaviour was a Warning, so this case passes on
`origin/main` unchanged and would pass if the narrowing were reverted. The fixture's own
header says "Validation must conform with no warning". `Case::no_warning` exists (line
1459). The unchanged case (`standpoint-withdrawn-warning`, expecting "displayable false")
still means something: the fixture now carries the retraction the guard needs and the
new message still contains the substring.
Acceptance point 3 asks for a counter-example that "is not warned about" and a deadname
case that "still is". The names and lifecycle counter-examples are structural assertions
over example data and formula shape; none runs the validator. No case in the tree
asserts the deadname warning fires.

**R3. Changed English literals have no French or Chinese entries.**
`AGENTS.md:512-514` on this base: a slice's catalogs "must have an entry whose `msgid`
equals every English literal you add or change". Of 27 changed literals, 5 have a
catalog context and all 5 are identical in fr and zh (see item 7). The other 22 have
none in either language, including all four notes of `gmeow:displayable`
(`slices/core/names/module.ttl:310-317`), which acceptance point 2 is about:

- names: `displayable` definition, useWhen, howToUse, avoidWhen; module comment;
  `SupersededNameSuppressionConstraint` definition.
- lifecycle: `hasDestructionEvent` howToUse; `supersededBy` howToUse, avoidWhen;
  `SupersededEntitySuppressionConstraint` definition.
- epistemics: `ClaimEvaluation` howToUse; `ClaimToken` definition, howToUse;
  `DoxasticTenure` definition, howToUse; `defeatedBy` avoidWhen; `hasDefeatStatus`
  avoidWhen; `WithdrawnDoxasticTenureSuppressionConstraint` definition.
- events: `Participation` definition, howToUse; module comment.
- standpoint: `WithdrawnStandpointSuppressionConstraint` definition.
- dreaming: `DreamReport` comment.

`checks.log` records these as `NOENTRY`. The review brief words the rule more narrowly
(only literals that already have an entry); the repository text does not, and acceptance
point 4 says translations are revised with the definitions. If the narrower reading is
intended, that is a maintainer ruling to record.

**R4. The narrowed standpoint constraint does not test whose withdrawal it is, misses
the canonical encoding, and contradicts the class it governs.**
Formula at `slices/core/standpoint/module.ttl:963-977`: guard is
`type(this, StandpointTenure) and duringInterval(this, iv) and endedAtTime(iv, end) and
retractsClaim(act, this)`. Well-formed: four universal variables, all bound by positive
guard atoms; target stays `sh:targetClass gmeow:StandpointTenure`.

- Holder's own case, as the fixture writes it (activity retracts the tenure): fires.
- Holder's own case, as `docs/claim-lifecycle.md:15,46` and
  `slices/core/standpoint/examples/claim-lifecycle.ttl:24-27` write it (activity retracts
  the claim): the tenure's claim is reached through `gmeow:standpointClaim`, the guard
  wants the retraction on the tenure itself, so nothing fires.
- Other-party dispute or `correctsClaim`: does not fire. Correct.
- Other-party retraction (an activity by another agent that `retractsClaim` the
  tenure): fires. The formula reads no `wasAssociatedWith`, no reifier `accordingTo`
  and no `tenureStandpoint`. The scope comment names exactly this link "made by ANOTHER
  party". The warning fixture's added activity names no actor at all
  (`tests/fixtures/shapes/standpoint-withdrawn-warning.ttl:29-30`), while its header and
  the definition (`module.ttl:958`) say the holder withdrew its own position.
- `gmeow:StandpointTenure` is unrevised and says the opposite of the new rule:
  definition "withdrawn in 2030 is an opened-then-closed tenure" (`module.ttl:72`) and
  avoidWhen "close the tenure with gmeow:displayable false" (`module.ttl:74`). Neither
  tells an author to record a retraction on the tenure.
  `tests/fixtures/coverage/organization-contested.ttl:61-68` likewise calls a closed
  tenure with no retraction "withdrawn".

**R5. Scope-comment items with no change and no recorded ruling.**

- Standpoint: "a `gmeow:succeedsClaim` or `gmeow:retractsClaim` link made by ANOTHER
  party still ends a claim's liveness for a ranker". Untouched;
  `slices/core/standpoint/module.ttl:1231` still states it as intended behaviour.
- "One pull request per slice." This is one branch over ten slices, with the
  observations deletion inside the events commit and `docs/location-mapping.md` inside
  the attestation commit. The scope comment (2026-11-15T02:37Z) is later than the
  `AGENTS.md` grouping rule (2026-11-14), so the general rule does not settle it.
- The `gmeow:correctsClaim` domain and range point ("to settle here or separately") has
  no disposition on the branch or in the stage directory.

Each needs the work or a maintainer ruling recorded verbatim; a coordinator's summary is
not one.

### OPTIONAL

- O1. Lifecycle residual. `supersededBy(this, s)` on a `gmeow:Appellation` fires whoever
  asserted the link, including another party's `gmeow:succeedsClaim` (sub-property of
  `gmeow:supersedes`) once inverses are materialized. Narrow, since succession is
  normally stated on name usages, which are not appellations. No fixture, example or
  case anywhere has an appellation carrying `gmeow:supersededBy`, so the firing case is
  unexercised. The IRI, label and header comment still say "entity"
  (`slices/core/lifecycle/module.ttl:167-174`).
- O2. Epistemics. `WithdrawnDoxasticTenureSuppressionConstraint` keeps the bare
  ended-interval guard that the standpoint constraint just lost. The scope comment left
  this to judgment; the coder judged it a holder's own act. The asymmetry deserves the
  maintainer's confirmation.
- O3. `slices/core/attestation/docs.md:220-221` states "A corrected issuer name is a
  correction by another party" as a universal; an issuer correcting its own label is
  the other case.
- O4. `tests/fixtures/shapes/participation-wellformed.ttl:1` keeps "(#41)"; the branch
  removed the matching "(#43)" from the standpoint fixture and edited this header.
- O5. None of the four added `mustNot` assertions declares `gmeow:saFailWitness`. That
  matches these files (0 of 60 existing bans use one).
- O6. `docs/location-mapping.md:19` fails `prose-check` on `WGS84` before and after
  this branch; marking it as code would let the touched file pass.
- O7. Old-reading text outside the scope list remains, for example
  `queries/competency/music-gharana-memberships.rq:5`,
  `slices/core/inference/module.ttl:196`, `slices/core/places/module.ttl` ("may carry
  gmeow:displayable false"), and ended-validity warnings in accessibility, finance,
  email, tags, calendar and rights. Not required of this branch.

## Answers to the ten points

1. **Slices.** Scope-comment slices: events (another party; constraint deleted, text
   revised; see R1), attestation (another party; O3), dreaming (both acts separated;
   matches), epistemics (holder's own kept, defeat and evaluation by others no longer
   prompt; matches, O2), gender, names, software (scope notes; match), standpoint
   (holder's own; R4, R5). From the issue body: lifecycle (succession of non-names no
   longer prompts; matches, O1), observations (ended validity; R1), names constraint
   (holder's own; formula unchanged, correct). Outside both lists:
   `docs/location-mapping.md:517`, one sentence that stated the banned reading; in the
   decision's spirit and minimal.
2. **Deleted constraints.** See R1. No global-safety case is uncovered; no unwanted case
   is removed either.
3. **Narrowed constraints.** Lifecycle: one `termIri` changed, well-formed, target
   becomes `sh:targetClass gmeow:Appellation` (`gmeow:PersonName` is a subclass; the
   slice manifest already depends on names). Holder's own superseded name fires; a
   dissolved organization does not; O1 for the residual. Standpoint: R4. Neither is an
   EL axiom; no cardinality form was added. Contradicting text and fixtures: R1 and R4.
4. **Rust case.** R2.
5. **Assertions.** Nine added cells, checked mechanically: every prefix used is
   declared, none unused. Example namespaces match the example files. `scopeModule`
   loads only the slice's own module, so the two "no constraint prompts displayable"
   bans are not tripped by other slices. Each can fail: reverting the guard class trips
   both lifecycle formula cells; adding a flag trips the two example cells; restoring
   either deleted constraint trips its ban; removing the retraction atom trips the
   standpoint cell. The names cells pin an unchanged formula. Polarities are right.
6. **Dangling references.** None at head for either IRI, either shape name, labels,
   messages or formula node names, tracked or untracked. On main they occurred only in
   the two modules and one fixture comment.
7. **Translations.** Five literals with catalog entries, all `msgid`-identical in fr
   and zh (names, gender and software scope notes; lifecycle `supersededBy` definition;
   dreaming module comment). No fuzzy flag and no empty `msgstr` added. The coder's
   "STALE dreaming" line is an artifact of matching an escaped `msgid` against a
   multi-line literal. R3 for the rest.
8. **Ledgers.** Table below.
9. **Hygiene.** No issue or pull request number and no history wording in added lines
   (O4 is pre-existing). No Python. Commit `f4ff0df39` (events) carries both
   observations files, and no commit message on the branch mentions the observation
   constraint; the prepared `msg-observations.txt` was never used. The squash keeps
   `main` bisectable, but the deletion has no description unless the squash message
   states it, and it blocks a per-slice split (R5).
10. **prose-check.** The only hunk in `docs/location-mapping.md` is line 517. Line 19
    is untouched and fails on main too. The other seven documents pass.

## Ledger table

| Ledger or pinned list | Change needed | Basis |
|---|---|---|
| `crates/docs/tests/fixtures/reviewed_coverage_golden.json` | No | Keys are catalog path to subject and predicate, not `msgid`. No entry added or removed in any of the ten catalogs; the pinned `supersededBy` definition key stays non-fuzzy; the deleted constraints never had entries. Fixing R3 adds entries and reopens this row. |
| `crates/logic-compile/src/frontend/functional_carrier_ledger.txt` | No | No functional property added or removed; no deleted IRI listed. |
| Projection-vocabulary ratchet | No | Module diffs add no `rdfs:subClassOf`, `rdfs:subPropertyOf`, SHACL, bridge or alignment construct; two grounded constraints removed. Not run. |
| insta snapshots (`*.snap`) | No | No snapshot contains a changed or removed string or a deleted IRI. Not run. |
| Conformance or constraint-catalog goldens | No | Zero hits at head for both IRIs, both shape names, labels and messages. |
| `docs/term-authority.toml` | No | Lists `DeadnameSuppressionShape` and `SupersededFacetSuppressionShape`; never listed the two removed shapes. |
| `tests/fixtures/lint-golden/coverage.json` | No | Names `supersededBy`, which is unchanged as a term. |
| `gmeow:CeilingRelocation` | No | Nothing moved between slices. |
| `.deficiencies` | No | Unchanged; no entries. |
| Slice-quality floors | Unproven | `floors.log` is a hand-written proxy, not the gate's tool. It shows no fall: translation ratio equal in seven slices and higher in observations and events; documentation lengths grew. |
| Producer-selected conformance shapes and fixture manifest | Yes, by the gate | The Rust cases read producer output, which predates the changed formulas until `make check` runs. |

## Not verified

- Anything requiring execution: `make check`, structural assertions, the two standpoint
  Rust cases, reasoning, drift, i18n lint, the real slice-quality floors, the ratchet.
- The generated shape for the gender constraint. No materialized shapes file exists in
  any worktree; R1 rests on the target-derivation source and on the existing
  `suppression-warning-only` case that expects this shape under the plain `validate`.
- Whether inverse and sub-property triples are materialized before validation (O1).
- The direction in which the reviewed-coverage golden pins keys.
- The coder's "STALE extensions/software" line (not in a literal this branch changed).
- French and Chinese translation quality; only `msgid` identity was checked.
- The house Rust guide named by the skill and the skill's `references/` directory:
  outside the repository, not read, per the coordinator's correction.
- A context-checkpoint hook asked for a script under the stage tree; not run for the
  same reason.

VERDICT: BLOCKED at 5eb1cf255046998a037995273c0f8e5863e7591d
