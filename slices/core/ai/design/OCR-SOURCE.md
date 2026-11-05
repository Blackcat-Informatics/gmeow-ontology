<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# OCR source-image digest evidence

`ocrSourceImageDigest` relates an actual Documents `ContentBlock` to the
source-supplied algorithm-typed digest of the exact identified image byte
representation from which the block's text was read by OCR. AI owns the
extraction-specific relation; Sources owns exact-byte content digest semantics;
Math owns the three existing algorithm datatypes. The definition records the
source's interpretation rather than independently certifying OCR execution
(Principles 2, 4, 9, 17 and 19).

## Literal identity and supplied evidence

The range is exactly the existing union of `blake3_256Digest`, `sha256Digest`
and `md5Digest`. Plain unkeyed default-length BLAKE3 and SHA-256 have sixty-four
bare lowercase hexadecimal digits. MD5 has thirty-two. Preserve the complete
supplied RDF term, including its original spelling and algorithm datatype;
there is no language or direction. Equal hexadecimal text under different
algorithms does not establish equal values. MD5 retention does not assert
cryptographic security. Keyed or derive-mode BLAKE3, different output lengths,
HMAC, truncated or perceptual hashes and scheme identifiers are outside these
selected contracts.

Each selected assertion requires actual source evidence for the block, the
OCR-input relationship, the algorithm construction and the identified byte
preimage. Preserve that evidence with the exact statement and its source and
viewpoint. A hash-shaped string does not establish any of those meanings. A
consumer selecting this relation must fail on missing or invalid required
evidence; it must not silently select an algorithm, normalize an invalid value,
retype a historical value, or retain a weaker string as the selected digest.
Original raw evidence may remain under its actual meaning, without being
promoted to this relation.

## Byte representation and image resources

The byte preimage is part of the supplied contract. An original encoded image,
a stored transformed or cropped image and a serialized decoded pixel buffer
are different representations. A crop digest identifies the crop's bytes rather
than the full image's bytes. Equal pixels do not imply equal encoded bytes;
matching digest spelling does not establish a common algorithm, image resource
or pixel interpretation.

For decoded pixels, the source must identify the actual byte serialization,
including whatever layout, channel, stride, dimension, colour-model, bit-depth
and endianness choices determine those bytes. This vocabulary chooses no
encoding, decoder, raster canonicalizer or pixel-equivalence rule. It accepts
neither an unspecified pixel preimage nor an inferred original-file preimage.

The literal-only relation needs no independently identified image resource.
It does not mint one or derive one from the digest. If actual image identity
is supplied, preserve it separately. Only that artifact's own exact identified
bytes justify `contentDigest` on that artifact. A stored concrete image file
may have Documents' `MediaObject` type; a decoded pixel buffer does not imply a
concrete media file. Existing `wasDerivedFrom` can retain actual derivation,
without inventing a generating activity or an OCR receipt.

The block's own digest and the OCR image digest have different preimages.
Consequently `ocrSourceImageDigest` is not a subproperty of `contentDigest`.
The new relation is optional and nonfunctional, without a maximum or a
one-value-per-algorithm rule. Coexisting attributed values remain coequal.
Neither this relation nor generic derivation links pairs multiple named image
resources with their respective digests; no Cartesian pairing or identity
inference is licensed. Resource-specific pairing requires its own explicit
association contract.

## Canonical supplied-value admission

One property-global `logic:ClosureEntry` opts this property's ContentBlock
domain and exact datatype-union range into supplied-value endpoint checks.
It has no `logic:onClass` and imposes no missing-path requirement on every
ContentBlock. No class restriction, maximum or functional carrier is added.

`OcrSourceImageDigestValueConstraint` guards the actual binary
`ocrSourceImageDigest(ocrBlock,ocrDigest)` relation. Its first universal variable
is the subject, so the validation target is SubjectsOf that exact property.
The same bound digest must satisfy one of three complete branches:

```text
forall ocrBlock, ocrDigest.
  ocrSourceImageDigest(ocrBlock,ocrDigest) ->
    (datatype(ocrDigest) = blake3_256Digest and regex(ocrDigest, "^[0-9a-f]{64}$"))
    or (datatype(ocrDigest) = sha256Digest and regex(ocrDigest, "^[0-9a-f]{64}$"))
    or (datatype(ocrDigest) = md5Digest and regex(ocrDigest, "^[0-9a-f]{32}$"))
```

The actual Formula uses `termSameComponent` with constant `componentDatatype`
and `componentWhole` selectors, and `termRegex` with explicit patterns. Every
operand is bound positively by the original property relation. Algorithm and
width are conjoined within each alternative; independent selections cannot
mix an MD5 datatype with a longer algorithm's width. The law's `formalizes`
reference names this property. Sources' existing contentDigest law guards a
different predicate and does not automatically check these statements.

This is canonical supplied-value validation, with a generated projection rather
than a hand-authored shape. Regex retains its target-dialect projection limits.
No native failure class, hash computation, byte-verification procedure or
consumer implementation is introduced. Type and lexical admission prove no
source-content claim, actual byte preimage, OCR execution, resource identity
or digest agreement; those remain source evidence and execution obligations.
See the [owning guide](../docs.md#ocr-source-image-digest-evidence),
[Sources](../../sources/docs.md#gmeowcontentdigest) and the
[Math digest contracts](../../../grounding/math/docs.md#algorithm-tagged-digest-literals).
