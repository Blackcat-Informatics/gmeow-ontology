<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/3efe40ca104763dd6896546ecc619474dd755e4a
  https://blackcatinformatics.ca/gmeow/derivation/fda65d8a09a9ef543a5f084d3a159fc331be3b5a
  https://blackcatinformatics.ca/gmeow/reifier/132f446ed3c76d9e8769c7c7ff51a2a4f04ea7ff
  https://blackcatinformatics.ca/gmeow/reifier/fea0c073ae67ee5afbfa34f91e2cea14ff4eab71
  https://blackcatinformatics.ca/logic/ConcurrentComposition
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/precedes
  https://blackcatinformatics.ca/logic/rule/transaction
  https://example.org/teleology/concurrent-non-serializable/conc
  https://example.org/teleology/concurrent-non-serializable/legL
  https://example.org/teleology/concurrent-non-serializable/legR
  https://example.org/teleology/concurrent-non-serializable/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/3efe40ca104763dd6896546ecc619474dd755e4a
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=https://blackcatinformatics.ca/logic/precedes
    term=https://example.org/teleology/concurrent-non-serializable/legL
    term=https://example.org/teleology/concurrent-non-serializable/legR
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/fda65d8a09a9ef543a5f084d3a159fc331be3b5a
    rule=https://blackcatinformatics.ca/logic/assert
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/ConcurrentComposition
    term=https://example.org/teleology/concurrent-non-serializable/conc
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/fea0c073ae67ee5afbfa34f91e2cea14ff4eab71>`

**World:** `<https://example.org/teleology/concurrent-non-serializable/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/3efe40ca104763dd6896546ecc619474dd755e4a>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://example.org/teleology/concurrent-non-serializable/legL>` `<https://blackcatinformatics.ca/logic/precedes>` `<https://example.org/teleology/concurrent-non-serializable/legR>` *(in `<https://example.org/teleology/concurrent-non-serializable/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/132f446ed3c76d9e8769c7c7ff51a2a4f04ea7ff>`):
    `<https://example.org/teleology/concurrent-non-serializable/conc>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/ConcurrentComposition>` *(in `<https://example.org/teleology/concurrent-non-serializable/world>`)*
