Language-tagged and directional text currently conflicts with string-only ranges on natural-language fields. This changes 30 word properties across 11 slice owners to canonical logic restrictions admitting unspecified-language strings, language literals and directional language literals. Existing word cardinalities use the same datatype union. Subject-domain inference remains open.

Documentation and glossary emitters preserve their known source language; glossary translations carry their catalog language. Owner structural checks, a multilingual condition example and focused production-code tests cover the contracts. Principles 4, 7, 17 and 19 govern source ownership and derived validation.

Part of #1807. The original issue remains open for its complete literal, machine-field, digest, media, time and tool-payload contract. The shared competency rationale must migrate together with its central DSL validation and is unchanged here.

Validation: 14 focused tests passed; the final glossary test also passed after changing its French sample text for source hygiene. Strict producer-independent workspace/all-target Clippy, source lint, static producer-recipe admission and the normal commit hooks passed. CI is ignored for sequential integration by explicit maintainer instruction. No local optimized producer, corpus construction or full gate was run.
