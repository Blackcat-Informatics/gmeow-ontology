<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Breakthrough readings

The existing `CharacterArc` integrates standpoint-scoped readings; `ArcSample`
is already an observation with a narrative position and interpreting vantage.
The extension reuses those records and provenance rather than introducing an
arc detector or a second observation model (Principles 4 and 5).

`sampleState` is functional and already required by the owner’s constraints.
Progress therefore has a separate `sampleProgress` relation. This preserves a
sample’s simultaneous affect and progress readings without changing the old
state carrier or its validation. The new progress and signal vocabularies are
open quality values, not subclasses of emotions or diagnoses.

`arcThread` ranges over `logic:Thing`: questions, intentions, working loops and
records need not all be information objects. The teller asserts the exact
common resource; the ontology does not supply an executor to certify the
assertion. Arc evidence and sample derivation retain
the underlying source records. Chronological succession does not imply cause.

Canonical `logic:` domains, ranges and class-local value restrictions describe
the new properties. Two realized `logic:Constraint` formula trees add only
intrinsic consistency: explicit none forbids a supplied thread, and each of the
four initial positive kinds and the conversation kind requires a thread. Both are guarded by supplied
classification assertions. Unclassified arcs remain legal; silence is unknown,
not none. Additional kinds are permitted and gain no undeclared obligations.
There are no new global functional carriers. Qualified count and confidence
records add their own scoped required paths without changing unrelated arcs.

Core inquiry vocabulary supplies the suspended inquiry signal. The qualified
extension also consumes existing messaging, record-time, note/tag, version,
source and math owners. Affect values remain soft references as in the
existing owner; there is no extension-to-extension dependency. Existing legacy
shapes are unchanged. Validation surfaces remain projections of the new
canonical axioms and integrity formulas (Principle 17).

[Qualified breakthrough readings](QUALIFIED-BREAKTHROUGHS.md) specify the narrower
types, conversation closure, next-step signals, reviewed rule settings and scoped
record counts, with the original standpoint and noncausal boundaries preserved.
