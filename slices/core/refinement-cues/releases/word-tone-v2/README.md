# Word-tone cue inventories, version 2

Patrick Audley approved these exact troubled and weary inventories for review and publication and designated himself reviewer and curator. Approval was observed on 2026-11-05 at 17:59:52 UTC. Both releases cite one shared approval Activity and reuse his existing GMEOW Person identity.

| Payload | Cues | Phrases | Bytes | SHA-256 |
|---|---:|---:|---:|---|
| `troubled-cues.payload.ttl` | 3 | 7 | 3947 | `e33516e14a746f3802aff5225a4756226afc4d1b36772d5ac12c6308bd427770` |
| `weary-cues.payload.ttl` | 2 | 4 | 2654 | `d9bdfd0ead31a0c07534d86663d203d60e3031d9b14408ed559bbb956b414ccd` |

This directory contains standalone external application data distributed under `slices/core/refinement-cues/releases/word-tone-v2/`. `word-tone-cue-releases.ttl` is its complete data entrypoint, assembled from source documentation, the two exact payloads and the review envelope. `release-assembly.json` records their deterministic byte order, digests, membership, provenance and approval. Payload digests cover the exact adjacent UTF-8 payload bytes, including their final newline; they exclude the envelope.

Full original source attribution, observed CC-BY-4.0 notice and adaptation mapping are retained in `source-evidence.json`, `source-documentation.ttl` and the verbatim documentary excerpt `source-cues.ttl.txt`. The excerpt records the original source; it is not an additional release payload. The two approved payloads retain their approved bytes. Version 1 was an unreviewed candidate, so no supersession of a released version is asserted.

All eleven English phrase alternatives, guidance and synthetic examples retain their approved bytes. These cues supply word evidence. Phrase matching does not execute the guidance’s person, time, quotation, negation or idiom exclusions and does not establish anyone’s state, a diagnosis or an automatic AffectiveClaim. Missing or failed reading supplies no neutral-tone fact.

External data validation has not run. The validation boundary is `gmeow validate word-tone-cue-releases.ttl`, which parses external data against resident shapes. No RDF or native conformance is asserted. The combined entrypoint is 16543 bytes with SHA-256 `e699cbe796f157281c2936eaafb258e874dc59a954d6414fa6ff0342743d8225`; `SHA256SUMS` identifies the distributed files.

The owner’s releases directory is external data storage. These inventories are not imported into the ontology dataset and have no dedicated release-blob or loader integration. Files under `slices/core/` participate in the existing compression-dictionary training corpus and may affect dictionary bytes. No matcher adoption or recipient-isolation qualification is established.
