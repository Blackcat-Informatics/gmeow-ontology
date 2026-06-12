<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The claim-extraction prompt — v1

The published, versioned extraction prompt (#55 deliverable 2). This is
**data, not code**: it ships so extractors emit the hallucination-resistant
shape, and so the eval suite (#298) can score any model against the same
contract. The emission format is `evals/claim-emission.schema.json`; the
scoring gates are `queries/audit/` + the SHACL shapes in
`slices/core/ai/shapes.ttl`.

## The template

```text
You are a claim extractor. Read the source document below and emit the
factual claims it supports, as JSON Lines — one JSON object per claim,
conforming to the claim-emission schema.

For EVERY claim you emit:
1. "text":       state the claim in one self-contained sentence.
2. "evidence":   one or more spans, each with:
   - "quote":  the EXACT supporting passage, copied verbatim from the source
   - "start":  the character offset (unicode code points, zero-based,
               inclusive) where the quote begins in the source
   - "end":    the offset where it ends (exclusive)
   - "polarity": "supports" | "refutes" | "neutral"
3. "confidence": your credence in the claim, 0.0–1.0. Calibrate honestly:
   confidence is scored against measured grounding, not rewarded for size.
4. "method":     "llm-extraction"

ABSTAIN rather than fabricate: if the document does not support a claim,
DO NOT emit it. Emitting nothing for an unsupported topic is scored as
correct abstention; emitting an ungrounded claim is scored as a
hallucination. Never invent quotes; never adjust offsets to fit.

If the document CONTRADICTS a commonly believed statement, you may emit the
contradicting claim grounded in the refuting passage with
"polarity": "refutes".

SOURCE DOCUMENT ({source_id}, content digest {content_digest}):
---
{document_text}
---
```

## Versioning

The template's identity is its content digest, recorded on the
`gmeow:PromptTemplate` individual that cites this file. Edits create
`claim-extraction-v2.md`; versions are immutable (P6 — releases fix forward).

## Why the contract is shaped this way

- **Quote + offsets are the dual selectors**: offsets make AIS checking
  mechanical against a digest-pinned source; quotes re-anchor when the
  source moves. The verifier accepts a span when the quote matches the
  source text — and additionally checks offsets exactly when the digest is
  current.
- **Abstention is first-class** because the eval corpus contains
  unsupported-bait: the constitution's own text refutes several plausible
  claims, and emitting them is measured, not forgiven.
- **Confidence is calibrated, not decorative**: stated credence is binned
  against measured grounding (#298's calibration metric).
