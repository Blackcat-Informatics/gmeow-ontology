Current source audit for the historical probability-model blocker:

The authored logic module already declares `FullIndependence` as a
`ProbabilityModel`. The compiler's structural admission recognizes a declared
model under both RDF typing and native `logic:instanceOf`; it only consults the
selected default graph. This does not select an independence model for an actual
probabilistic query, which still requires its explicit model declaration.

The combined candidate strengthens the existing compiler regression to cover both
typing surfaces, require the emitted contract, and retain its uncertainty facet.
A new synthetic named-graph control requires the hard contract diagnostic when
the only model is outside the selected graph. Together with the existing absent-
model and native structural-typing controls, the selection passed 4/4. Source lint
and producer-independent strict Clippy also passed.

These test-only changes do not establish a new full-canon blocker count. The
historical 327-unit inventory belongs to its original bundle/selection and must
be remeasured on the authenticated new product; it is not a reason to invent a
model or erase graph context. The complete prepared-canon, request composition,
grounding-diagnostic, native/external proof and blinded consumer requirements
remain open. Current production CLI routing still compiles caller files, so the
one-file distribution evidence alone cannot establish those requirements.

The existing optimized producer build continues for the authenticated bootstrap.
No new PR is being opened, and no full proof or issue completion is claimed.
