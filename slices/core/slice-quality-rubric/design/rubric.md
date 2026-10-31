<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Design: the slice-quality rubric as a graded lattice

This is the canonical design for the slice-quality rubric. It records the
formalism the rubric commits to, so that later edits — new axes, adjusted
thresholds, new tiers — stay inside the structure rather than drifting from it.

## The object is a graded assessment, not a scalar

Scoring a slice yields a **grade profile vector** `g : Axis → Tier`, one grade per
quality axis. The vector is the primary object of the rubric; every downstream
artifact (the roll-up tier, the ranked advice, the RDF assessment graph) is
derived from it. Treating the scalar tier as primary would discard the
distinguishing information between two slices that meet to the same tier for
opposite reasons, which is precisely the information an uplift advisor needs.

## Tiers form a bounded lattice; the roll-up is the meet

The tiers `Registered ⊏ Grounded ⊏ Linked ⊏ Exemplified ⊏ Maximal` are totally
ordered by `gmeow:tierRank`, so they form a bounded lattice (a chain). The roll-up
tier of a slice is the **meet** (greatest lower bound = least rank) of its grade
vector:

    rollup(s) = ⊓ { g(a) : a ∈ axes }

The weakest axis caps the slice. The meet is **unweighted** on purpose. Meet is a
lattice operation and takes no weights; introducing weights would turn it into a
weighted average — an ad-hoc scalarization — and would break the identity that
makes the ratchet sound (below). `gmeow:axisWeight` is used **only** to order the
advice list, answering "which weak axis first", and is provably absent from the
score: a structural cell forbids wiring the weight into the tier order.

## The ratchet is the lattice order

A slice opts in by declaring `gmeow:sliceQualityTier t` in its manifest (the sole
tier truth). The gate fails when

    rollup(s) ⊏ t

which is exactly the lattice strict-order relation — no bespoke severity
arithmetic. The declaration is a ratchet: it may only be raised, enforced against
a committed floor artifact so that lowering is detectable without git archaeology.

## The scalar roll-up is a lossy projection

Because `rollup` collapses a vector to a scalar, it is a lossy projection in the
same sense as every OWL/SHACL/SSSOM projection in GMEOW. It therefore carries a
preservation judgment in the loss ledger rather than presenting itself as the
whole truth. Consumers that need the full picture read the grade vector; consumers
that need a single gate read the meet.

## Context scope is a coeffect

Each axis declares, as data, how much graph its primitive may **read**:
slice-local, the dependency closure, or the merged closure
(`gmeow:axisContextScope`). This is a coeffect grade on the measurement: it bounds
the input context, never the output scope. Advice is single-slice at every grade.
The registry groups axes by scope for transform-once batching and gates that a
narrower-scope axis reads no more than it declared — turning the informal
"read cross-slice, advise single-slice" rule into a checkable invariant.

## Composition laws

Two laws keep the scoring deterministic and honest, and are cashed out as fixture
tests rather than left as prose:

- **Monotonicity under composition.** Scoring `A`, `B`, and their merge `A ∪ B`,
  the shared-axis grades in the merge do not fall below their standalone meet.
  This dogfoods the pipeline superset law and catches scan-order non-determinism.
- **Idempotence of self-assessment.** The assessment graph is itself an
  observation graph living in a slice, so re-scoring the advisor's own output is
  stable. This guards the self-application capstone against oscillation: the
  fixpoint is real, not a single blessed byte-string.

## Self-application capstone

The rubric slice must score **Maximal** on the rubric it defines. This is not
decoration: it forces the slice to a full annotation coat, tri-language
translations, worked examples including the epistemically hard case, and
rationales that state ontological reasons and name no test artifact. The advisor
run over this slice is the standing regression test for the instrument itself.

## Version-one quality record fingerprint

`gmeow:qualityRecordFingerprint` is the corpus’s grade-reconstruction integrity
witness. Its value is a literal of the locally owned
`gmeow:qualityRecordFingerprintV1` scheme datatype: exactly 64 lowercase
hexadecimal digits with no prefix, language tag or base direction. The scheme
uses plain BLAKE3 with its default 32-byte output, but its identity includes the
following versioned reconstruction contract. It is not the generic
`gmeow:blake3_256Digest` of the serialized corpus bytes.

The preimage is the existing byte sequence, unchanged by the witness migration:

1. Begin with the ASCII bytes `gmeow-slice-quality-record-v1`, followed by the
   single byte `0x1e`.
2. Visit assessments in ascending slice-IRI order, using Rust string ordering.
   Append the slice IRI’s UTF-8 bytes and the single byte `0x1f`.
3. Within each assessment, visit axis grades in ascending axis-IRI order.
   For each grade append the axis IRI, `0x1f`, its score, `0x1f`, its tier IRI,
   and `0x1f`. IRI fields use UTF-8. The score uses the producer’s `exact_score`
   conversion: Rust Display’s shortest round-tripping decimal spelling of the
   finite stored score, also encoded as UTF-8. It is not a fixed-width display
   score or the RDF literal’s original lexical spelling.
4. Append the ASCII bytes `rollup`, `0x1f`, the roll-up tier IRI’s UTF-8 bytes,
   and `0x1e`. Continue with the next assessment.
5. Apply BLAKE3 to that complete preimage. Encode the resulting 32 bytes as
   64 lowercase hexadecimal digits, in output-byte order, and retain the
   `gmeow:qualityRecordFingerprintV1` datatype IRI.

There is no additional prefix, suffix, whitespace or normalization. The scheme
retains the existing framing, sorting and numerical conversion. Re-serializing
the same reconstructed grades does not change the witness; changing a score,
axis, tier, slice roster or roll-up changes the input to the hash.

The producer returns a complete native typed literal. The reader admits exactly
one distinct native object term under `qualityRecordFingerprint`; identical
terms repeated across graph labels remain one witness. It rejects absent or
conflicting witnesses, nonliterals, the wrong datatype, language or direction,
and any lexical form outside the lowercase 64-hex contract. It then compares
the complete admitted typed literal with a fresh reconstruction. These are the
owning reader’s concrete scheme checks, not a generic custom-datatype parser or
a claim that the numerical grades were correctly measured.

`versionFingerprint` continues to witness which authored sources were scored.
It is unchanged and remains separate from what was recorded. The new property
is not its subproperty: such a bridge would place two different witnesses in
the source-fingerprint field. The existing corpus-witness law still requires
both witnesses, now referencing `qualityRecordFingerprint`; the new field’s
class-scoped maximum is one. Neither witness substitutes for the other
(Principles 4 and 17).
