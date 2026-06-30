<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/772814b44cf4df1a973e86f56dd12e50acb34a28
  https://blackcatinformatics.ca/gmeow/derivation/bb3fb2417c0e5b55189913307770693788b57b62
  https://blackcatinformatics.ca/gmeow/reifier/1bdde43ebaa0bc33d9df97afaad32492ccd7b80b
  https://blackcatinformatics.ca/gmeow/reifier/538c97085869f48fa0aa6ccb1d818fa0e4595a1a
  https://blackcatinformatics.ca/logic/ConcurrentComposition
  https://blackcatinformatics.ca/logic/Path
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/path/ba3370602a53d7c736dec74257e779594756abdf
  https://blackcatinformatics.ca/logic/rule/transaction
  https://example.org/transaction/concurrent-serializable/conc
  https://example.org/transaction/concurrent-serializable/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/bb3fb2417c0e5b55189913307770693788b57b62
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/Path
    term=https://blackcatinformatics.ca/logic/path/ba3370602a53d7c736dec74257e779594756abdf
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/772814b44cf4df1a973e86f56dd12e50acb34a28
    rule=https://blackcatinformatics.ca/logic/assert
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/ConcurrentComposition
    term=https://example.org/transaction/concurrent-serializable/conc
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/1bdde43ebaa0bc33d9df97afaad32492ccd7b80b>`

**World:** `<https://example.org/transaction/concurrent-serializable/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/bb3fb2417c0e5b55189913307770693788b57b62>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/path/ba3370602a53d7c736dec74257e779594756abdf>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/Path>` *(in `<https://example.org/transaction/concurrent-serializable/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/538c97085869f48fa0aa6ccb1d818fa0e4595a1a>`):
    `<https://example.org/transaction/concurrent-serializable/conc>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/ConcurrentComposition>` *(in `<https://example.org/transaction/concurrent-serializable/world>`)*
