# Prior art assessment

Local assessment of brief's prior-art.md and the current implementation; no
independent-agent review is claimed. The brief covers the issue, zero comments,
six linked issues within two hops, fourteen search matches and the last 200
commit defect trailers. It found no previous commit, branch or PR for this issue.
Those bounded searches do not establish a global zero recurrence count.

PurRDF already owns deterministic hash states, Domain and little-endian framing.
The pinned 7734b01 implementation was inspected; frame_le_with emits the same
length prefix plus bytes. The RDF builder records invalid IRIs and refuses them
at freeze, which determines the sink failure-path test's correct boundary.

The existing math dependency already contains math-lift, so the affect producer
can use that sink without adding a crate or runtime dependency. Its previous
claim that three independent sinks were necessary did not reflect that graph.
The duplicate sink and decimal implementation are removed instead of retained
behind a compatibility path. Math's Rational decimal formatter has a distinct
rounding contract and remains separate from the finite-f64 lexical formatter.

The diagnostic owner already supports generated cloneable payloads; factoring
its identity/grade implementation supports existing non-cloneable evidence
without flattening it or forcing an unrelated Clone/Arc payload redesign.
No fallback parser, alternate profile, process cap or host probe is introduced.
