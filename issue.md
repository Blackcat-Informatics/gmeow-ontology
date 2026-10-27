# Issue #2007: Use native CBOR for complete GMN codebook transport

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, quality, dependencies, rust, complexity-hard

## Body

Move the complete prepared GMN codebook transport from Ciborium/Serde to PurRDF's existing native CBOR codec. This operation includes the actual borrowed producer and bounded consumer, all four wire records and every alias/glyph/signature table. Principles 4 and 7 require one codec owner and verified wire identity.

Acceptance:

- Preserve the exact producer packet bytes: `GMNCBK01` header, SHA-256 checksum, declaration-order maps, ordered definite arrays, field names, optional nulls, unsigned values, profile, source pins and existing Merkle identity. Before replacing the encoder, capture a complete independently authored packet using the original implementation, including absent signatures and an unsigned-width boundary. Retain its exact bytes as a frozen compatibility fixture and verify the replacement against it.
- Stream borrowed inventories through native CBOR head/text APIs. Do not build a copied dictionary, registry, RDF view or writer-side Value tree. Do not use canonical map sorting, which would alter field order. Retire the obsolete Serde reference records and serializers.
- Decode once with native CBOR and an explicit depth limit of 32, then consume native values into the four domain records. Require named text-key maps, every field exactly once, no unknown members, exact pair/binding widths and unsigned `u32` values. Optional fields accept null or their declared type only. Keep well-formed nonpreferred heads and indefinite containers; refuse tag wrappers, tagged bignums, undefined optional values and byte strings masquerading as table sequences. These stricter domain-admission rules are intentional; producer bytes remain unchanged. Retain the original native decode cause on a domain failure.
- Preserve checksum/header/size/source/profile/Merkle checks, strict table ordering, all reader aliases, glyph scope/security rules and dialect acceptance. Hydration never parses source RDF or produces the corpus.
- Extend the three existing authored transport controls and add the frozen complete-packet parity control. Preserve their existing semantic assertions; use native values and recomputed checksums for malformed packet inputs. Cover the four record/row boundaries without duplicating generic RFC CBOR conformance. Keep the repository source-pin control unchanged and excluded locally.
- Remove the lang-bridge production Ciborium edge and its corresponding lockfile entry; do not retain the legacy codec as a test-only alternate. Compile the actual host producer and consumer callers and use normal hooks. Public decode APIs, artifact paths and source pins stay unchanged. Actual embedded/browser/corpus acceptance remains a separate required qualification, not a claim derived from component tests.

Remaining contracts: #1777, #1776, #1336 and #1962. This complete transport owner does not retire the remaining workspace binary owners. Standing exception: CI waived; no local optimized producer, canonical corpus/fixture construction, synchronization or full gate. Report unverified installed-consumer, browser and release acceptance explicitly.


## Comments (0)

