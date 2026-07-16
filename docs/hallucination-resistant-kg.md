<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The hallucination-resistant KG pattern

The difference between *"the model said X"* and *"claim X, asserted by model M
at time T, grounded in this exact span of this source, true to confidence 0.7,
contradicted by a higher-confidence claim, source now stale"* — as one
runnable, copy-pasteable pattern. No new vocabulary: everything here is
the unified observation stance (CONSTITUTION P9) plus the thin seams of
`slices/core/ai`.

## The spine

```text
Source document  (gmeow:Document + contentDigest + sourceLocation)
  → Chunk        (chunkOf + spanStart/spanEnd into the source; own digest)
    → EvidenceSpan  (spanOfChunk + spanStart/spanEnd + selectorTextQuote —
      │              the DUAL selectors: offsets bind to the digest, the
      │              quote re-anchors when the source moves)
      │  supportPolarity: supports / refutes / neutral
      └─ groundedIn⁻¹ ─ the claim
           = gmeow:StandpointClaim  (vantage = the model SoftwareAgent)
             gmeow:wasGeneratedBy   → the ModelInvocation (prompt, params)
             gmeow:confidence / assertedAt / accordingTo / validFrom-Until
             gmeow:claimModality    (incl. gmeow:bullshit — Frankfurt)
             gmeow:claimVeridicality (untrue / licensed-falsehood)
  Contradiction  (contradictsClaim ≥2, kind, detectedBy — surfaced, NEVER ranked)
```

The worked fixture is `tests/fixtures/coverage/hallucination-kg.ttl` —
**dogfooded on GMEOW's own README and CONSTITUTION** with real digests and
real offsets, engineered to contain all four audit cases.

## The four cases and their gates

| Case | The data | The gate that catches it |
|---|---|---|
| Grounded | `groundedIn` → span with chunk + offsets + quote + polarity | clean (the control) |
| Hallucination | LLM-extracted claim, **no** `groundedIn` | `ClaimNeedsEvidenceShape` (WARNING — flagged, never deleted, P10) + `claims-without-evidence.rq` |
| Contradicted | `Contradiction` over claims with confidence 0.4 vs 0.9 | `claims-contradicted-by-higher-confidence.rq` (reports; ranks nothing — P9) |
| Stale source | source `sourceModifiedAt` > claim `assertedAt` | `StaleSourceShape` (WARNING) + `stale-source-claims.rq` |

Run them all with one command:

```bash
gmeow audit your-claims.ttl            # human summary
gmeow audit your-claims.ttl --json     # the flat-JSON projection (below)
gmeow audit your-claims.ttl --strict   # exit non-zero on any flag (CI)
```

`make audit` runs the gates over the fixture and is part of `make check`
(verified by construction, P7). Every audit query has expectation-bearing
product coverage; generic RDF 1.2 / RDF\* and SPARQL engine compliance belongs
to PurRDF's own suite.

## The extraction prompt (published data)

`docs/prompts/claim-extraction-v1.md` — instructs a model to emit claims
**with** an exact quote + character offsets + polarity + calibrated
confidence, and to **abstain** rather than fabricate. The emission format is
`evals/claim-emission.schema.json`; the eval suite scores any model
against this exact contract with these exact gates.

## The flat-JSON projection

`gmeow audit --json` emits one object per LLM-extracted claim — the "simple
JSON API" shape (no RDF knowledge required of consumers, P13):

```json
{
  "claim": "…iri…",
  "text": "GMEOW authors every fact once; all other forms are generated.",
  "model": "…the model agent iri…",
  "method": "llm-extraction",
  "confidence": 0.95,
  "evidence": [
    {"span": "…the span iri…", "source": "…", "chunk": "…", "start": 60, "end": 141, "polarity": "polaritySupports"}
  ],
  "flags": {"ungrounded": false, "contradicted": false, "stale": false},
  "contradicts": []
}
```

## Projections (all generated, P4)

- **W3C Web Annotation**: `groundedIn` → `oa:Annotation` (body = the claim,
  target = the chunk via `oa:TextPositionSelector` from the typed offsets) —
  the generated `web-annotation` profile.
- **PROV-O**: claims are StandpointClaims, so the existing standpoint→PROV
  projection (`standpoint-prov.rq`) already emits `prov:Entity` /
  `wasAttributedTo` — **reuse, not new cells**.
- **schema.org**: `schema:Claim` with `text`/`appearance`/`author` — and
  **structurally no `reviewRating`**.

## `schema:ClaimReview` — the corrected translation

The original blanket refusal of `ClaimReview` was a **translation error**:
`ClaimReview` is per-REVIEW, not per-claim — each review node carries its
own `schema:author` and its own `reviewRating`. N competing assessments of
a claim therefore translate faithfully as **N coexisting ClaimReview
nodes, each authored by its vantage** (`mapSchemaClaimReview`): nothing is
aggregated, no winner is picked, and no verdict is dropped — the
standpoint structure expressed in schema.org's own idiom.

There is no remaining ClaimReview-specific refusal. The complete rule:

- N individual verdicts → N ClaimReviews, each authored by its vantage.
- An **asserted aggregate** (some agent — a consortium, an algorithm, the
  eval harness — performed the aggregation and asserted the result) is
  itself just another vantage-indexed claim, with the aggregator as its
  vantage: it coexists with the individual verdicts, contestable like any
  of them, and translates as one more review authored by that agent.
- No aggregate asserted → none emitted. That is the UNIVERSAL never-invent
  rule (P4/P5 — the transpiler emits no fact absent from its input, for
  any term), not a ClaimReview doctrine.

The verdicts themselves remain vantage-indexed claims (`claimModality
gmeow:bullshit`, `claimVeridicality gmeow:veridicalityUntrue`) that
another standpoint may contest. Declared fiction is
`veridicalityLicensedFalsehood` — the licensed-falsehood safety property,
applied to model output exactly as to human output.
