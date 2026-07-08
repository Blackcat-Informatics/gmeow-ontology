<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# The slice-quality rubric

Every slice in GMEOW can be lifted — more information moved into the ontology,
more grounding on the triad, more linkage and projection, richer examples and
counter-examples, honest translations. What was missing was a **measuring
instrument**: a single, opinionated, deterministic way to score a slice against
the richness bar and say, in ranked order, what to fix next. This slice is the
**rubric** that instrument reads.

Its thesis is *rubric-as-data*. The quality axes, the tier ladder, the per-tier
score thresholds, the uplift advice text, each axis's read scope, and every dated
exemption are **ontology-resident individuals** here — not constants buried in
Rust. The `gmeow-dev slice-quality` advisor ships only a small, closed set of
measurement primitives; this slice tells it which axes exist, how they combine,
and what to say when a slice falls short. Tuning a threshold, minting a new tier,
or adding an axis is a slice edit and a regenerate, never a code fork. The single
`gmeow:sliceQualityRubric` Profile names the six descriptor properties every axis
carries and the three open value vocabularies — axes, tiers, and context scopes —
they draw from.

## Grades form a lattice

The tier ladder — Registered, Grounded, Linked, Exemplified, Maximal — is a
bounded lattice grounded in `logic:QualityValue`, ordered by `gmeow:tierRank`. A
slice does not get one score; it gets a **grade vector**, one grade per axis. The
familiar single roll-up tier is the **unweighted lattice meet** of that vector:
the weakest axis caps the slice. That is deliberate. A slice with maximal
grounding but no translations and a slice with full translations but no grounding
demand opposite uplift work; collapsing either to an average would hide exactly
the gap the advisor exists to surface. The scalar tier is therefore a *lossy
projection* of the vector, and — like every lossy projection in GMEOW — it carries
a preservation judgment rather than pretending nothing was lost.

`gmeow:axisWeight` never enters that meet. It orders **advice**: which weak axis
to fix first. The score stays honest; the priorities stay useful.

## Read cross-slice, advise single-slice

Some axes must read widely — inferential centrality needs the merged closure,
domain-twin detection needs the whole reference graph. Others need only the slice
itself. This is declared as data with `gmeow:axisContextScope`, a coeffect grade
from slice-local through the merged closure. Whatever an axis *reads*, its advice
is always about the **one target slice**. The scope declaration makes that rule
checkable instead of a matter of trust: a slice-local axis that reaches into the
merged graph is a gated violation.

## Honest gaps, not silent ones

Where a measurement genuinely depends on a producer that has not yet landed, the
rubric carries a dated `gmeow:AxisExemption` rather than quietly dropping the axis.
Every exemption names the axis it covers, states its reason in doctrine terms,
stamps the date it was minted, and names the Rust symbol whose appearance makes it
**stale** — so an exemption can never harden into permanent optionality.

## Its own capstone

This slice is the exemplar of the standard it defines. It must itself score
**Maximal** on its own rubric — full annotation coats, tri-language translations,
and rationales that state the ontological reason an axiom holds and name no test
artifact. The check of the rubric against its own rubric is the standing
regression test that keeps the instrument honest.
