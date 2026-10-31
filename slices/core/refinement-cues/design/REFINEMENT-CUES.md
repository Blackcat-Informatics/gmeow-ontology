<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Refinement cue source design

The AI owner supplies `Prompt`; observations supplies `Refinement`.
`echoesPremiseOf` adds an attributed dependence reading, with `accordingTo`
on the statement or reifier and source authorship preserved separately. It has
no global truth, contradiction or independent-evidence consequence.

The reviewed `CueLexicon` remains a versioned `InformationObject`. Its
`versionOf`, `versionLabel`, `contentDigest`, `wasDerivedFrom`,
`wasAttributedTo` and `cueReview` paths retain their original requiredness.
The realized review law still requires reviewer association and sign-off time;
the version law still distinguishes release from lineage. Multiple sources,
reviewers and digest algorithms remain possible. A newly released declaration
payload receives a new version identity and digest, not a mutation of old bytes.

`Cue` replaces a literal-list entry with an addressable information object.
Its language-bearing properties preserve both `rdf:langString` and
`rdf:dirLangString`. The existing language-presence law is carried forward over
phrases, guidance, examples and counterexamples. `writtenInLanguage` reuses
the language owner's registry-independent resources; no flat language-code alias
is introduced. `lang:Lexeme`, `lang:SurfaceForm`, `Glossary` and
`GmnDictionary` have stronger or different identity obligations and are not
invented for a textual cue.

## Grounding and verified version pins

The generic `gmeow:linearRegex` datatype is defined in the lang grounding slice,
not the downstream application. `rdfs:Datatype` is the actual datatype
declaration form; canonical `logic:` axioms carry its use as a range.
`logic:termRegex` remains a validation-formula relation with dialect residue,
not this detector's parser. No downstream external alignment or false equivalence
to SPARQL, SHACL, NIF or lexical concepts is added (Principles 17 and 19).

The [lang regex contract](../../../grounding/lang/design/LANG-REGEX.md) records
the standard references and execution boundary. The following pins were checked
against the installed source corresponding to Cargo.lock, without running a
matcher:

| Contract | Inspected source |
| --- | --- |
| Regex syntax | regex 1.13.1 and regex-syntax 0.8.11 entries in Cargo.lock. |
| Unicode word categories | regex-syntax 0.8.11, src/unicode_tables/general_category.rs, generated from ucd-16.0.0. |
| Simple fold pairs | regex-syntax 0.8.11, src/unicode_tables/case_folding_simple.rs, Unicode 16.0.0, C/S rather than full F mappings. |
| Sentence segmentation | unicode-segmentation 1.13.3, src/tables.rs, UNICODE_VERSION = (17, 0, 0). |
| Search complexity | regex 1.13.1, src/lib.rs, individual O(m*n) searches versus O(m*n*n) iterator worst case. |

These are distinct pins. Nothing silently treats Unicode 16 and 17 as the same
segmentation or case-fold release. The phrase word categories are Lu/Ll/Lt/Lm/Lo,
Nd and apostrophe; they are not every Unicode Number category or the regex
engine's broader word class.

## Explicit choices and enforcement

[The declaration contract](../docs.md) specifies contiguous token matching,
simple-fold equivalence, comparison-only apostrophe substitution, no other
normalization, phrase/pattern union and deduplication only for identical
cue/source/span identity. Those are deliberate source design choices. They do
not imply a matcher ran or that a typed regex literal has been parsed.

Polarity uses exactly one of the supporting or opposing evidence relations,
each bounded by a maximum-one restriction on the owning Cue class. CueNegation
likewise bounds its negating cue and scope within that class. These restrictions
introduce no global property-characteristic carrier. A maximum-one restriction
can identify two resource targets on one instance; shared targets do not identify
distinct instances. Existing universals type every value, while requiredness
remains with the polarity choice and the negation's closed-world required paths.
Evidence targets name readings, not asserted facts or verdicts.

`CueNegation` reifies one negator and one finite scope. A positive window is
present exactly for word-window scope. Comparison is symmetric, inclusive and
counts complete intervening words. Detected negators are considered before
suppression, including suppressed or coincident occurrences; no recursive
priority or subject-truth model is invented. Part/message identities come from
the source's `BodyPart`/`Message` structure. Source turn identity is explicit,
never equated with a thread or inferred from timestamps.

Required-path closures, finite values and record predicate closure are distinct
conditions. Class-guarded `linkVia`/`termIn` formulas provide predicate
allowlists for lexicon, cue and negation records. The canonical source admits
the relevant attribution/provenance fields and permits richer metadata on
separate statement reifiers. No hand-authored SHACL surface or compiler change
is needed.

Intrinsic constraints check declaration structure only. A selected reading lane
must supply conforming matching, zero-width rejection, offset recovery,
segmentation and source grouping or fail explicitly. Source constraints do not
authenticate a reviewer, recompute a digest, prove regex lexical validity, execute
a suppression or establish a reading about the author. These limitations remain
visible rather than hidden behind a successful structural check.

## Match identity and judgement grounding

`CueMatch` and `CueEvalCase` are information objects, while `CueJudgement`
specializes the observations owner's `Observation`. The reused `spanStart` and
`spanEnd` have no imposed global domain. The new selected-literal relation
specializes the lang owner's domain-free `lang:literalValue`; it preserves the
complete literal without forcing a Document, SurfaceForm or invented anchor.
The existing ai, lang and observations dependencies already own these seams.
No external vocabulary equivalence or new global functionality carrier is added.

Source membership uses `logic:linkVia(record, field, literal)`. Case agreement
uses the same bound literal variable on `evalText` and `matchSourceLiteral`,
preserving RDF term identity through a graph join rather than string conversion
or numeric value equality. Cases retain a reported inventory and do not require
source record identity to equal case identity. Input artifact and cue-release
provenance remain separate from report identity when supplied.

The norms owner's generic `evaluationVerdict` domain would introduce
ConditionEvaluation semantics, so the two dedicated verdict properties reuse
the observations owner's `EvaluationVerdict` and values without specializing
that property. Grounding admits held/not-held;
judgement admits all three outcomes. Observation laws guarded by direct type
do not supply a SubKind's required roles, so the local judgement contract requires
actual `vantage`, a verdict and one explicit target branch. Multiple vantages
remain legal; the judging standpoint is not the source person. Local maxima
and required paths introduce no property-characteristic ledger changes.
Every supplied `observedFeature` target must also be the selected `judgedMatch`
or `judgedCase` target through a shared-variable graph join. This excludes an
unrelated observed person without demanding a duplicate generic target assertion
or using functionality to identify that person with a detector report.
An inner class guard independently anchors the branch condition for lowering,
while the outer class guard continues to scope the whole contract.

Suppression references the applicable `CueNegation` declaration. Supplied actual
negator-occurrence evidence remains on its RDF 1.2 assertion or reifier through
existing `wasDerivedFrom`; no parallel suppression mechanism is invented.
The formula checks membership in the matched cue's declared rules. It neither
validates occurrence evidence nor executes the sentence or word-window rule.

Supported numeric comparison checks start below end, with nonnegative integer
types supplied by canonical restrictions. The available term algebra does not
provide the required literal code-point length operation, so the upper bound
and substring authentication are explicit consumer contracts. Predicate closure
and complete shared-literal joins do not silently claim that missing execution
capability, detector completeness or correct grounding has been proved.

## Selected grounding and reader-specific operational state

The agency owner supplies actual attempt and intention-transition event kinds,
their `intentionTransitionOf` relation, artifact-operation activities and read
links. The existing provenance owner supplies `wasGeneratedBy`; the events
owner supplies transitive `subEventOf`. A dependency on agency-records makes
this reuse explicit. No local attempt class, external-dispatch substitute,
automation classifier or duplicated activity model is introduced.

`MachineryRecord` is a Mixin across event/activity and information-record sorts.
Roots require explicit `machineryAdmittedBy` facts supplied by the reader, plus
the actual owning event kinds. Descendant and generated-record rules require
the same reader's admission at both endpoints. Reading, use and receipt-target
links do not transfer classification to input artifacts. Generation does not
erase human authorship or replace a separate output admission.

There are two independent scope protections in every productive policy clause:
positive `groundingSelectedBy` activation and the clause's own `groundingRule`
membership. Native compilation discovers Rule declarations globally; declaration
membership alone cannot serve as activation. Recursive state is binary
`machineryForReader(record, reader)`, not unary type membership. The same reader
variable flows through root admission, every propagation step and the negative
grounding guard. Unary type publication is descriptive and never drives a later
classification or exclusion. This prevents cross-reader contamination in one
selected fact model.

Each selected operation still owns its exact input/world boundary. These
relations are declared scope inputs, not a new execution framework, an access
grant or a warrant to merge independent evaluations. A CueMatch binds its
reader conditionally when its cue declares a grounding set; separate reports
retain separate interpretations of one occurrence. No generic vantage or global
property-functionality contract is strengthened.

Native Rule ASTs use RDF 1.2 `rdf:reifies` statement targets and explicit
`logic:termVariable` carriers. The grounding set retains all ten rules: six
admitted roots, two finite propagation forms, descriptive type publication and
one stratified negative eligibility rule. This policy has no existential output
terms, reductions or hidden rule-order priority. The negative rule binds the
actual match cue, record and field before testing same-reader state absence.
An evaluator must complete the selected positive closure before treating that
absence as a negative; unsupported capabilities or an unfinished operation
cannot masquerade as a grounding result.
Owner type closure is an explicit prerequisite of this policy input. A caller
using only graph-free facts must supply those type consequences or selected
rules that derive them; the converter alone supplies no ontology closure.

The declaration laws require rule-head and body reifier evidence, conditional
reader presence, grounded-result binding and allowed set predicates. They do
not replace the compiler's full Rule parser, evaluate rules or authenticate
EDB-versus-IDB provenance. Derived-only `cueGrounded`, `machineryForReader` and
MachineryRecord publication remain explicit source/output ownership contracts.
No union-graph formula is falsely presented as such an origin checker, and no
new reasoning engine, test suite or external equivalence is added.

## Parameter releases and ranking annotations

Reading parameter artifacts reuse existing information-object, version,
provenance and content-digest owners. They do not inherit CueLexicon merely to
reuse its review predicate: a parameter rule is not a cue inventory. Accountable
source provenance and normal vocabulary review remain distinct from an
authenticated review execution. Generic numeric thresholds are optional
declarations; the RestatementRule profile requires only its own two integer
thresholds. Every supplied parameter keeps its declared meaning.

Datatype restrictions retain integer/decimal admission. Existing
ValueRangeConstraint primitives state open-zero, closed-one share bounds
without adding a new numeric solver. Numeric parameter maxima are local to
their declaring rule classes. Measurement slots are constrained on their RDF
statement reifiers, without globally functional reader or provenance fields.

The reifier contracts use a class guard and actual property-presence guard,
then the established lowered statement components. Run/share pairs, source
bindings, attribution and restatement possibility remain independent of
confidence. Digest evidence uses the identical RDF literal in the annotation
and actual parameter release, with a separate distinct-term count. It does not
parse a digest under an invented datatype or claim algorithm verification.

Receipt bases describe reported reader choices, not an executable absence test
or globally ranked evidence. A reader unable to support its ordering must retain
that limit rather than manufacture a receipt or silently weaken its operation.
These declarations leave lexical computation and temporal selection with the
consumer while preserving their input and version evidence (Principles 4, 7,
9, 11, 17 and 19).
