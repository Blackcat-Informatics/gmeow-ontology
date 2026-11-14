Closes #2777. Part of #2748 (its item 6; item 7 is not in this change).

## What this changes

The logic frontend reads the scope of a claim or rule from the node that carries it: a statement's
reifier, a statement node or a rule node. A carrier with more than one value of a scope coordinate was
refused whole with `UNSUPPORTED_SCOPE_MULTIPLICITY`, a message that named a lowering the compiler did not
have. The vocabulary allows the shape: a claim that rests on several sources carries several
`logic:provenance` values.

Now:

- **Several values of exactly one coordinate** (`standpoint`, `time`, `confidence`, `modality`,
  `provenance`, `inModule` alike): one scoped claim or rule per value, in the order of the values'
  canonical term text, with every other coordinate shared. The result does not depend on storage order.
- **Several values of two or more coordinates**: still refused whole, with `AMBIGUOUS_SCOPE_COORDINATES`
  naming the node and the fields. The node gives no pairing between the values of different coordinates,
  and their product would assert combinations no author wrote. One reifier per intended combination is the
  authored form.
- **An atom of a rule** keeps a single scope. Several values on an atom are refused with
  `AMBIGUOUS_RULE_ATOM_SCOPE`: lowering them would turn the rule into a different rule. Several values on
  the rule node lower to one rule per value.
- **Every other refusal stays.** An invalid value among several (a confidence out of range, an unknown
  modality, a proposition-valued term) withholds the whole claim or rule, and a node's claims are emitted
  all together or not at all.
- The old diagnostic code is removed.

## A decision about source admission

Each lowered claim or rule keeps the authored node as its source. In source admission a unit is identified
by a digest of its kind, anchor, documents and disposition, and a rule's unit is anchored by its source
node. Two rules lowered from one rule node that receive the same disposition would therefore be two
byte-identical units. The ledger describes authored units, so such an exact duplicate is recorded once:
it is one admission fact about that node. Unit identifiers are formed as before, so nothing changes for a
source that has no multi-valued carrier. Rules whose dispositions differ (two admitted rules carry
different rule indices) stay separate units. Axioms are anchored by their index, so two axioms lowered from
one reifier are two units.

Two tests pin this together: `a_rule_node_with_two_provenance_values_lowers_to_two_rules_one_source` (two
rules in the program) and `rules_lowered_from_one_rule_node_are_one_admission_fact_with_unchanged_id` (one
unit, equal in every field to the unit the same rule yields with one provenance value, and no two units in
the admission share an identifier).

## Corrected after review

An independent review of the first commit found three defects, fixed in the second:

- The meta-findings stage of the pipeline collected rule lowerings into a map keyed by their source node,
  so a diagnostic meta-rule node that lowers to several rules was folded as the last of them. Such a node
  is now refused: a meta-rule is folded as one rule, and choosing among several would be silent. Every other
  consumer of the owner lowerings and every map keyed by a source node in the workspace was read; none of
  the others picks one of several.
- Values were counted by their text. Two spellings of one confidence or of one modality were two values.
  Confidence and modality are counted by their parsed value, after every spelling is validated.
- The requirement's named behaviours that had no test now have one: the source of every lowered claim and
  rule is the authored node; the classic statement carrier; the rule-node cases; values held in different
  physical tables; the default modality beside another.

A second review round and a second reading found two more, fixed in the third commit:

- A scope holds standpoint, time, provenance and module as text, but an IRI and a literal with the same text
  were counted as two values: one node lowered to two identical scopes, and a second multi-valued coordinate
  beside it was refused as ambiguous although the scopes would have been the same. Values of a coordinate
  are now distinct exactly when the scopes they produce are distinct: for those four coordinates one term is
  kept per distinct text, the first in the total term order, before the count and the fan-out.
- The pipeline document still said the lowered claims are ordered by the terms' text. It now states the
  order the code keeps: by term for the four text coordinates, by number for confidence, by declared order
  for modality.

The fourth commit is text only and answers the review bot: the usage note of `logic:provenance` told an
author whose rule atom carries several values to state them on the rule node, which scopes the whole rule
and leaves the atom without the scope that was written. It now says to write one rule per intended atom
value. The French and Chinese entries carry the same sentence.

The fifth commit follows from the fourth, on review: the diagnostic for such an atom still offered the
rule node as a remedy, so the compiler and the note disagreed. Its message now gives the same advice as the
note. Only the message text changes; no test asserts it, and the code that raises it is untouched.

One behaviour to know: a carrier whose only scope is the default stays an unasserted quotation, so a claim
with the modality `none` beside another modality yields only the other claim; a rule node with the same two
values yields both rules, one of them unscoped.

## What this does not do

- It does not make the classical profile decide a scoped claim. An axiom that comes from a reification is
  still blocked there ("contextual reification requires typed translation"), and a rule with a scope is
  still blocked for context erasure. That is item 7 of #2748.
- It does not fan out a rule's atoms.

## Verification

Run in a bare checkout (no `generated/` tree, no producer-made fixtures). The tests of
`gmeow-logic-compile`, the producer recipe, the format check and strict clippy on that crate, and `msgfmt -c`
on the French catalog were run on the tree of the fifth commit. The pipeline crate's tests, its clippy and
`make helpers-hygiene` were last run at the second and third commits; nothing in that crate changed
afterwards. The slice scorer was run after the fourth commit:

- The tests of `gmeow-logic-compile`: 1,094 run, 1,091 passed, 3 failed. The three are the same tests, by
  name, that fail at the base commit in the same checkout (1,074 run, 3 failed). The twenty tests this
  change adds pass.
- The tests of `gmeow-pipeline`: 1,334 run, 1,077 passed, 257 failed, the same 257 by name as before the
  change to that crate in the same checkout (they need a `generated/` tree or producer-made fixtures). The
  test this change adds there passes. It was not shown to fail without the fix; that is argued from the
  code it replaces.
- `make producer-recipe`, format check on both crates, strict clippy on `gmeow-logic-compile` and on
  `gmeow-pipeline` (all targets), `make helpers-hygiene`, `msgfmt -c` on both catalogs: each exit 0.
- The slice scorer of the consumer CLI over the logic slice of `main` and of this branch: the same sixteen
  axis scores and the same 6,440 findings.

Not run:

- A producer pass. A full pass of `main` reports the old diagnostic zero times, so no claim of this
  repository's own slices takes the new path; the acceptance line about an unchanged finding set is to be
  shown by the next full pass that includes this change.
- The fixture-backed tests, and the crates that depend on this one (no public signature changed).

## Hosted checks

`generation (a)`, `generation (b)`, `fixture-prefix` and `quality` fail on `main` because the producer
exhausts the memory of the hosted runner. When every check has finished, each failure will be quoted here
beside `main`'s.
