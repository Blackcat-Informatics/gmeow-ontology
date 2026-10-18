The native RDF 1.2 rule migration already landed in [PR 1802](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1802). Correct the remaining aggregation-test comment: atom variables use resource carriers, and aggregation fields round-trip independently. This changes no executable Rust or generated artifact.

Fresh focused Make verification passed all 15 native writer/reader and GTS controls. These cover literal and variable identity, scope, guards, aggregation, malformed-input refusal, exact logical round trips and actual GMEOW-profile GTS write/import/readback. Corpus-purity checks and all normal commit hooks passed. CodeRabbit reviewed the final correction without actionable findings. Broader hosted CI remains pending under the existing user merge authorization; no full-corpus or full-gate pass is claimed.

Principles 2, 4, 6 and 7: one canonical native representation, accurate documentation and verified round trips.

Closes #1805.
