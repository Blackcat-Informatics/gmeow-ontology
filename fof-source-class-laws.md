# Source-bound class laws

The full logic issue/PR objective remains active and incomplete. This change adds
actual source-class semantics to the external proof path; it does not establish
complete-canon admission, prepared-base production, installed proof acceptance or
merge readiness.

## Implemented path

`frontend/source_admission/classes.rs` builds restriction laws in the existing
canonical Formula IR. Selected source class objects retain their original named
or scoped blank identity through explicit ground-term bindings. They are not
replaced with the restriction skolemizer's content-hash IRI. The live admission
commitment binds the new formula and bindings just like every other admission
field. The FOF writer consumes that typed law through its existing formula renderer.

Supported restrictions have one IRI-valued relation and any combination of
someValuesFrom, allValuesFrom and resource-valued hasValue. A definition is a
universally quantified membership biconditional, not just a one-way subclass
implication. Some/all use separate scoped quantifiers; hasValue retains its exact
source individual. Multiple distinct values for a single-valued field are malformed;
physical duplicates of the same field/value do not invent ambiguity.

Every consumed definition statement must belong to the exact selected documents.
Class dependencies require their own admitted definitions. Cycles are simultaneous
first-order laws, not recursively expanded syntax. Refusals propagate over reverse
dependency edges without rescanning every definition on each chain step. Source
grammar is structural support only after its class law is admitted. Existing
compiler metadata is accounted as support only when its class-source contributors
are certified or positively outside selection; a selected unsupported contributor
still blocks the complete proof problem.

The writer now permits a source blank class as a property domain/range while still
requiring an IRI property and rejecting literal class objects. Membership has one
uniform renderer. The semantic profile contributes four quantified formula-IR
laws giving logic:/OWL Thing and Nothing universal/empty extensions, including
through denotational class aliases. These laws have no source-unit IDs; they are
profile meanings, not fabricated selected source claims. They are added only after
source admission and nonempty source projection succeed.

The new ambiguity control reproduced an existing restriction-compiler panic for
multiple onProperty values. The compiler now records the hard error
AMBIGUOUS_RESTRICTION_PROPERTY and emits no lowering for that malformed root.
Both the compiler diagnostic and the class admission refusal block proof
publication. It never chooses one property. Native term-ID deduplication prevents
repeated representations of the same property from creating false ambiguity.

## Explicit remaining class boundaries

Cardinality, enumeration/boolean constructors, datatype interpretations, literal
hasValue, procedural restrictions, unselected support and scoped definitions have
not been certified by this change and remain typed refusals. Mixed external class
constructors are refused rather than ignored. This is implementation progress,
not a zero-blocker claim or permission to exclude those units from the complete canon.

The previous document-selection controls deliberately now include a qualified
cardinality alongside their someValuesFrom constraint. SomeValuesFrom itself is
newly supported; the additional unsupported constraint preserves the controls'
original requirement that explicit exclusion works and selected unsupported
contributors cannot disappear, including after canonical deduplication. Ignoring
the additional constraint would fail those controls.

## Verification

Final source:

- `raw/fof-class-laws-runtime-v4.log`: terminal PASS, 31/31; nextest
  c6b5a687-f03e-4dae-a34f-08ecf3de1a89, build 5m46s, run 0.075s. This covers
  all 27 TPTP controls plus four synthetic native restriction preparation,
  source-emission and projection controls. No corpus producer or authenticated
  corpus consumer is selected.
- `raw/fof-class-laws-clippy-v4.log`: terminal PASS, 1m04s, strict
  producer-independent workspace/all-target Clippy with warnings denied. The four
  corpus-dependent consumer packages remain excluded by that existing diagnostic
  target; this is not consumer compilation or full gate qualification.
- `raw/fof-class-laws-lint-v5.log`: terminal PASS, including formatting,
  source/corpus-purity seals and issue-reference lint.
- `raw/fof-class-laws-fmt-v8.log` and final `git diff --check`: PASS.

The new controls cover source blank identity; existential, universal and fixed-value
laws; source syntax exclusion; blank range classes with literal rejection; cyclic
class definitions; transitive and document-excluded dependency refusal; unsupported
context, datatype, procedural, schema and mixed-constructor cases; primitive class
laws through aliases; and refusal to authorize an empty source selection from
profile laws alone. Ambiguity controls require both the hard compiler diagnostic
and malformed source admission, while identical repetitions remain admissible.
The original literal, quotation, document-selection and live-evidence integrity
controls remain in the passing selection.

Attributable intermediate results remain in raw logs:

- runtime-v1: 23/24 passed; the new test incorrectly expected readable variable
  tokens instead of the renderer's hexadecimal alpha-renaming. Corrected the
  quantifier-structure control, not the implementation's valid encoding.
- runtime-v2: 25/25 passed before the final refusal representation and profile-law
  refinements; nextest 73518b16-a4cf-4185-93c0-99bf2aeab02b, build 5m10s,
  run 1.275s. This is historical coverage, not final-source qualification.
- clippy-v2: FAIL on using the large admission enum as a Result error. The compact
  ClassRefusal enum retains blocked/malformed kind, code and reason without
  admitted payloads; clippy-v3 passed in 53.67s before the final source changes.
- runtime-v3: 25/26 passed, reproducing the native compiler panic; nextest
  c281f8f6-ba7f-434c-9703-d7852b5cf4d2, build 5m45s, run 0.083s. The final
  implementation records a fatal diagnostic and preserves the malformed input for
  inspection; it does not choose a property or authorize partial execution.

Complete proof qualification must also audit the canonical relation-object
reflection contract. The existing writer uses per-IRI predicate symbols; these
controls do not establish congruence of role applications when source property
objects are denotationally equal. That requirement is not waived by the class
object identity controls or their focused results.

No commit, push, PR creation, issue closure, review-thread resolution, external
message or merge occurred. Producer fingerprinting remains unauthorized and was
not run. Consumer compilation, complete canonical semantics and prepared-base
embedding, actual installed/native/external proof behavior, corpus/browser/full
gate qualification, independent review and ghprsq merges remain required.
