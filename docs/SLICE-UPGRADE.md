<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Slice upgrade — from measured gaps to maximal quality

This guide explains how to improve one GMEOW slice without gaming the
`slice-quality` measures. The advisor is a detector and prioritizer, not the
specification of a good ontology. Its findings identify where to look; the slice's
semantic contract, consumers, executable paths, examples, and adversarial tests
determine what a complete repair is.

The standing queue and floor-ratchet mechanics remain normative in the
[`gmeow-slice-uplift`](../.agents/skills/gmeow-slice-uplift/SKILL.md) skill. The
general authoring rules live in [`SLICE_GUIDE.md`](SLICE_GUIDE.md), and the test
cell taxonomy lives in [`SLICE_QA.md`](SLICE_QA.md). This document is the curation
playbook between them: how to turn a scored deficiency into a coherent semantic
uplift.

The governing commitments are:

- model the strongest correct concept rather than optimizing for an available
  tool or a score (Principle 1);
- edit canonical sources and regenerate every projection (Principles 4 and 17);
- remove inferior or conflated concepts rather than retaining compatibility debt
  in the canon (Principle 6);
- prove the authored contract through executable, drift-gated evidence
  (Principle 7).

## The quality target

Maximal quality is not a large term inventory, a green score, or a high test
count. A maximally curated slice has one connected account of:

1. **meaning** — each distinction is explicit, non-conflated, and useful to a
   named consumer;
2. **behaviour** — examples and queries demonstrate what the model permits,
   derives, rejects, lowers, or preserves;
3. **proof** — every important claim is tested at the cheapest layer that can
   actually falsify it;
4. **explanation** — annotations and design prose teach the same model that the
   executable paths implement;
5. **language access** — translations convey the meaning naturally rather than
   wrapping an English token or copying English syntax;
6. **projection honesty** — generated views and their losses agree with the
   canonical model;
7. **held gains** — measured improvements are regenerated, checked, and pinned by
   raise-only floors when they have genuinely been earned.

The unit of work is therefore a **semantic packet**, not an axis score. A packet
may include a concept distinction, its annotations, a worked example, a positive
and negative validation pair, a competency proof, a production-path test, and the
translations needed to keep all surfaces coherent. Keep that packet within one
slice even when it improves several tightly coupled axes.

## Pass types

Use two deliberately different passes. Repeating the same shallow checklist does
not create depth.

### First pass: make the slice coherent and executable

A first pass establishes a trustworthy baseline:

- read the whole slice, its design set, mappings, shapes, examples, tests, and
  production consumers;
- resolve obvious conflations and missing ownership distinctions;
- complete the substantive annotation coat on the concepts that carry the
  slice's central behaviour;
- replace inventory-only examples with at least one end-to-end worked example;
- replace term-presence tests with behavioural competency, structural, and
  conformance evidence;
- connect at least one canonical example to the real Rust producer, adapter,
  validator, reasoner, or projection path where such a path exists;
- translate all new or changed localizable prose with natural target-language
  definitions;
- regenerate, remeasure, and run the full gate.

The exit condition is not “all terms mentioned.” It is “the slice's central
promise can be explained, executed, falsified, and reproduced from its canonical
sources.”

### Deep sweep: attack the model's remaining weak seams

A deep sweep assumes the first-pass baseline is sound and becomes adversarial:

- audit each major distinction for overclaim, overlap, missing disjointness, and
  accidental equivalence;
- trace every important concept through ingest, internal representation,
  reasoning, validation, projection, loss recording, and export;
- compare prose, examples, constraints, and implementation behaviour for semantic
  drift;
- test ordering, identity, multiplicity, optionality, recursion, and malformed
  partial structures rather than only happy paths;
- prove round trips and preservation claims on the strongest shared fragment;
- inspect negative space: plausible incorrect graphs that the current suite still
  accepts;
- review translations as explanations in their own languages, including formal
  variables and terms of art;
- remove dead vocabulary, disconnected fixture inventory, duplicate diagnostics,
  and assertions that no consumer or proof path can reach;
- finish every remaining advisor item whose remediation improves the model rather
  than merely its presentation.

The deep-sweep exit condition is `advice=0` **and** no untested semantic claim found
by the end-to-end audit. `advice=0` alone is necessary but not sufficient.

## End-to-end workflow

### 1. Establish an uncontended baseline

Before editing, inspect local worktrees, remote branches, active pull requests,
and the target slice's ownership. The uplift lane yields to any active issue lane
or in-flight slice branch. Record the current commit and run:

```bash
make slice-quality
make slice-quality SLICE=slices/<group>/<slice>
make slice-quality-gate
```

Capture the per-axis scores, grades, capping-axis antichain, ranked advice, and
current floors. Never lower a floor, alter a weight, or weaken a threshold to make
the profile look better.

### 2. Read the slice as a system

Do not begin by editing the advisor's term list. Build a small semantic map:

```text
design claim
    ↓
canonical term or axiom
    ↓
worked example
    ↓
producer / reasoner / validator / projection
    ↓
observable result and loss record
    ↓
test that fails when the claim is broken
```

For each central concept, ask:

- What distinction does it preserve?
- Which consumer needs that distinction?
- What valid graph demonstrates it?
- What tempting invalid graph must be rejected?
- Which production path interprets it?
- What derived or projected result should be observable?
- Which test would fail if the implementation ignored it?

Unanswered questions identify semantic gaps more reliably than raw term counts.

### 3. Choose a semantic packet around the capping axis

Start with the advisor's capping axis and its `axisAdviceTemplate`, then inspect
the per-site findings beneath it. Group only findings that share one semantic
seam. Examples include “typed intermediate representation construction,” “unit
conversion with reference frames,” or “form–sense–reference separation.”

Write the packet's acceptance claim in plain language before touching Turtle.
For example:

> A source ternary assertion is accepted, lowered through the production
> relational adapter, and preserves argument order under one stable reifier.

That claim determines the ontology change and the evidence layers. It prevents a
test from degenerating into “these seven terms exist.”

### 4. Repair the canonical semantics first

Fix the strongest source of truth before its examples or tests:

- split conflated roles into explicit classes or properties;
- add the domains, ranges, disjointness, cardinalities, or guarded constraints
  that the distinction requires;
- write labels, definitions, usage guidance, examples, and consumer advice that
  explain consequences rather than restating the local name;
- update normative design prose when the contract changes;
- delete superseded constructs when the stronger model replaces them.

Do not hand-author a generated shape, mapping, bundle member, or documentation
projection to satisfy a finding. Principle 4 requires source correction followed
by regeneration.

### 5. Build one behaviour-connected worked example

A good example is a small scenario whose nodes participate in the behaviour being
claimed. It should be possible to point from each important fixture fact to a
query result, validation finding, lowering result, or projection.

Remove disconnected inventories added only to mention vocabulary. If a term needs
an example, give it a role in the scenario or create a separate focused example.
Do not hand-author the result that a producer is supposed to compute and then
claim that the fixture proves the producer.

### 6. Prove the contract at the right layers

Use complementary evidence; do not make one test notation impersonate another.

| Claim | Primary evidence |
|---|---|
| A term has a required axiom or two concepts remain distinct | Structural cell |
| A modeled scenario answers a domain question | Competency cell with exact rows or an ASK result |
| A valid graph is accepted | Positive example-conformance fixture |
| One malformed graph is rejected for a specific rule | Isolated negative fixture with the pinned violation code |
| A compiler, adapter, reasoner, or projector performs a transformation | Rust test through the production API |
| A projection preserves or loses a feature | Projection/round-trip test plus loss-ledger assertion |

Prefer **twins**: one smallest valid fixture and one smallest invalid fixture for
each important constraint. For guarded constraints, test both directions when
both partial states are forbidden. Keep each negative fixture isolated enough
that its expected violation code identifies the intended rule.

Structural queries that enumerate required items must prove **all** enumerated
items. An existential query such as `VALUES ?item { ... } ?item a owl:Class`
passes when only one item exists. Use universal double-negation or an exact-count
construction so one missing member makes the test fail.

When an RDF fixture describes a transform, add a Rust test that loads that
canonical fixture and invokes the real transform. This closes the gap between
“the expected output was written down” and “the implementation produced it.”

### 7. Translate meaning, not identifiers

A coat and its fr/zh translations are **one indivisible term-batch**: never land a
term's annotation coat in one pass and defer its translations to a later one. A new
or expanded coat grows the slice's localizable-literal denominator, so landing it
untranslated *dilutes* measured translation coverage; pairing the coat with its
translations keeps the measure honest. This is not a convention to remember — it is
**mechanically enforced**: a coat landed without its translation drops the slice's
measured `axisTranslationCoverage` below its committed raise-only floor, and
`make slice-quality-gate` reds. Author each term's coat, its fr and zh renderings,
and any earned floor ratchet as one packet.

For every changed localizable literal:

- write a complete, idiomatic definition in French and Mandarin;
- preserve IRIs, operators, and formal symbols only where they are part of the
  described formalism;
- use target-language bound-variable names or neutral mathematical variables;
- ensure formula examples still bind and refer to the same variables;
- avoid templates equivalent to “In English this is ‹the term IRI›”;
- avoid copying English prose inside a translated definition merely to satisfy
  non-empty coverage.

Run `make i18n-lint`. A translation is evidence of access to the concept, not a
filled cell in a coverage table.

### 8. Regenerate and inspect the consequences

```bash
make fmt
make check          # materializes generated/ through the single producer, then gates
```

Do not follow `make check` with its component targets: the gate already ran
validation, the cached declarative slice verdict, and the authenticated Rust suite.
Repeating them adds burden without adding evidence.

Review generated diffs as projections of the canonical change. Unexpected fanout
usually reveals an ownership mistake, a generator defect, or a broader semantic
effect that needs explicit review. Fix the source or generator; never patch the
projection.

Run focused crate tests for changed executable paths before the full gate. Inspect
snapshot changes byte-for-byte and accept only intentional output changes.

### 9. Remeasure without chasing the number

```bash
make slice-quality SLICE=slices/<group>/<slice>
make slice-quality-gate
```

Compare the profile with the recorded baseline:

- Did the targeted axis rise for the intended reason?
- Did advice disappear because the semantic gap was closed?
- Did another axis fall because new prose, terms, or examples expanded its
  denominator?
- Is the roll-up still capped by another member of the minimum-rank antichain?
- Does the generated evidence match the acceptance claim?

If an axis genuinely rises, **ratchet its ontology-resident floor to the measured
value in the same change** — the raise-only floor is set at the freshly measured
score on each landing, so the gain is held and can never silently regress. If the
measured score falls, repair the regression; never lower its floor. A score increase
caused by deleting useful content is not an uplift.

### 10. Land one held slice

Run the complete gate:

```bash
make check
```

Commit the canonical sources, tests, translations, documentation, generated
projections, and earned floor changes as one reviewable semantic packet. Push one
slice-local branch and keep bundle-touching changes serialized with other slice
uplifts. The handoff reports the before/after evidence, the production path
exercised, focused gates, full-gate result, and any remaining advisor findings.

## Logic first-pass pattern

The first `logic` pass established the reusable pattern for grounding slices:

1. It distinguished a **conversion target** from a presentation-only projection
   target, so the relational core's role was represented rather than implied.
2. It completed substantive annotation coats for the constraint-sugar classes,
   explaining their validation and lowering behaviour instead of merely naming
   them.
3. It repaired the typed-IR example so its variables participate in the asserted
   formula, and replaced a disconnected hand-authored lowered tuple with a source
   ternary assertion.
4. It added behaviour-connected constraint examples and competency results rather
   than a vocabulary inventory.
5. It added positive/negative conformance twins for constructor exclusivity,
   guarded implication, carrier exclusivity/indexing, and literal datatype
   dependency.
6. It loaded the canonical Turtle fixture from Rust and drove the production
   relational adapter, proving reifier identity, `instanceOf`, argument order, and
   value preservation.
7. It made structural enumeration universal, closing the false-green pattern in
   which one member of a `VALUES` list satisfied a purported all-members test.
8. It translated the changed concepts into idiomatic French and Mandarin and used
   neutral formal variables rather than leaking English placeholder prose.

The important strategy is the sequence: **semantic distinction → connected
example → layered falsification → production execution → natural-language
access → regeneration and measurement**. The quality axes confirm the result;
they do not define it.

## Grounding-slice sweep order

The grounding sweep proceeds in widening circles:

1. **`logic` first pass** — establish the maximal-quality pattern on the reasoning
   substrate.
2. **`lang` first pass** — apply the pattern to form, sense, reference,
   interpretation, translation, grammar, and the registered seams into `logic:`
   and `math:`.
3. **`math` first pass** — apply it to expression, quantity, operation, measure,
   dimension, frame-sensitive values, and the registered seams into `logic:` and
   `lang:`.
4. **`logic` deep sweep** — return with evidence from both peer layers and attack
   cross-layer denotation, typed IR, conversion, proof, projection, and loss seams
   at greater depth.
5. **Iterate outward** — select dependent slices by the live prioritization and
   dependency graph, carrying the same semantic-packet method into each slice.

`lang` precedes `math` in this sequence by deliberate curation order, not by
foundational rank. The three grounding slices remain co-foundational. Each pass is
still slice-local and must yield to any active issue lane.

## Review checklist

Before calling a slice pass complete, verify:

- [ ] The advisor baseline and floor state were recorded before editing.
- [ ] The change states one semantic acceptance claim.
- [ ] Every new distinction names a consumer and observable consequence.
- [ ] Examples are connected scenarios, not term lists.
- [ ] Required-set tests prove every member, not merely one member.
- [ ] Important constraints have minimal positive and negative evidence.
- [ ] Production transforms are invoked through production APIs.
- [ ] No expected producer output was substituted for executing the producer.
- [ ] Translation conveys meaning naturally in each target language.
- [ ] Each landed coat and its fr/zh translations travelled together in this batch
      (no coat left untranslated to dilute coverage — `make slice-quality-gate` holds
      the translation floor).
- [ ] `make i18n-lint` is green, including cross-batch glossary consistency (one term
      never reads back as two divergent translations, absent a declared homograph).
- [ ] Only canonical sources and intentional Rust tests were hand-edited.
- [ ] Generated diffs were regenerated and inspected.
- [ ] Measured gains are explained by the semantic change.
- [ ] Floors were raised only when earned, ratcheted to the measured value, and were
      never lowered.
- [ ] Focused gates and `make check` pass.
- [ ] Remaining advice, if any, is explicit evidence for the next pass rather than
      a hidden quality claim.
