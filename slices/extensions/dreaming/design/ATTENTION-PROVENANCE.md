<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Dream-derived attention

`requestedByDream` is owned by this extension. It links an AttentionRequest
to the actual DreamReport recollection episode from which the request was
derived and specializes the domain-free `wasDerivedFrom` (Principles 4 and 17).
The episode remains an occurrent Experience; it is not retyped as an enduring
report document. A written account can separately derive from that episode.

`attentionMessage` is owned by core agentic and carries verbatim request
wording, including supplied language and RDF 1.2 base direction. A message does
not acknowledge, resolve or execute the request. Derivation from dream content
does not assert that the dreamed event occurred externally (Principle 9).

The dependency direction stays extension to core: dreaming consumes agentic,
provenance and agency-records for this composition. Core agentic and
agency-records do not depend on dreaming. The ordinary scene
[`goal-attempt-satisfaction.ttl`](../examples/goal-attempt-satisfaction.ttl)
keeps dream recollection, request, intention, attempt, configured verifiers and
cited evaluation evidence distinct. It is an illustration, not a runtime
verification result.
