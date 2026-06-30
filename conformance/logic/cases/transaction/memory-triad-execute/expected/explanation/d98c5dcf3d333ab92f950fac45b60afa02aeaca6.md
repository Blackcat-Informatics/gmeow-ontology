<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/0e1ff5327c63c474f3bb7aecf86347b9cd7ab192
  https://blackcatinformatics.ca/gmeow/derivation/bfc538373000c76e18f685d55db6ce4e9438b062
  https://blackcatinformatics.ca/gmeow/reifier/5a1675d42cced9218ee9edb439a53d7aab53baa8
  https://blackcatinformatics.ca/gmeow/reifier/d98c5dcf3d333ab92f950fac45b60afa02aeaca6
  https://blackcatinformatics.ca/logic/Path
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/path/f34f3255b6464f9166eab1a741c659fb7216a077
  https://blackcatinformatics.ca/logic/rule/transaction
  https://blackcatinformatics.ca/logic/transitionFromState
  https://example.org/transaction/memory-triad-execute/start
  https://example.org/transaction/memory-triad-execute/turn
  https://example.org/transaction/memory-triad-execute/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/0e1ff5327c63c474f3bb7aecf86347b9cd7ab192
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/Path
    term=https://blackcatinformatics.ca/logic/path/f34f3255b6464f9166eab1a741c659fb7216a077
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/bfc538373000c76e18f685d55db6ce4e9438b062
    rule=https://blackcatinformatics.ca/logic/assert
    term=https://blackcatinformatics.ca/logic/transitionFromState
    term=https://example.org/transaction/memory-triad-execute/start
    term=https://example.org/transaction/memory-triad-execute/turn
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/d98c5dcf3d333ab92f950fac45b60afa02aeaca6>`

**World:** `<https://example.org/transaction/memory-triad-execute/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/0e1ff5327c63c474f3bb7aecf86347b9cd7ab192>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/path/f34f3255b6464f9166eab1a741c659fb7216a077>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/Path>` *(in `<https://example.org/transaction/memory-triad-execute/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/5a1675d42cced9218ee9edb439a53d7aab53baa8>`):
    `<https://example.org/transaction/memory-triad-execute/turn>` `<https://blackcatinformatics.ca/logic/transitionFromState>` `<https://example.org/transaction/memory-triad-execute/start>` *(in `<https://example.org/transaction/memory-triad-execute/world>`)*
