<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/c951c3ca537db79ed354ebec1abfd27b4e2302ed
  https://blackcatinformatics.ca/gmeow/derivation/f00e24468a8cf08436d69ba2a2dbefa2b5e98d30
  https://blackcatinformatics.ca/gmeow/reifier/029c7c2188e7afd435ddb4f47f2348276dc8b8d7
  https://blackcatinformatics.ca/gmeow/reifier/3df5fab90bbf53f12cebd24eaaa7b0be7559c32a
  https://blackcatinformatics.ca/logic/ConcurrentComposition
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/rule/transaction
  https://blackcatinformatics.ca/logic/temporallySucceeds
  https://blackcatinformatics.ca/logic/txstate/aadcb2bac3493877324f4888003f4a6df04e9139
  https://example.org/transaction/isolation-write-skew-snapshot/conc
  https://example.org/transaction/isolation-write-skew-snapshot/s0
  https://example.org/transaction/isolation-write-skew-snapshot/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/c951c3ca537db79ed354ebec1abfd27b4e2302ed
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=https://blackcatinformatics.ca/logic/temporallySucceeds
    term=https://blackcatinformatics.ca/logic/txstate/aadcb2bac3493877324f4888003f4a6df04e9139
    term=https://example.org/transaction/isolation-write-skew-snapshot/s0
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/f00e24468a8cf08436d69ba2a2dbefa2b5e98d30
    rule=https://blackcatinformatics.ca/logic/assert
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/ConcurrentComposition
    term=https://example.org/transaction/isolation-write-skew-snapshot/conc
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/3df5fab90bbf53f12cebd24eaaa7b0be7559c32a>`

**World:** `<https://example.org/transaction/isolation-write-skew-snapshot/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/c951c3ca537db79ed354ebec1abfd27b4e2302ed>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/txstate/aadcb2bac3493877324f4888003f4a6df04e9139>` `<https://blackcatinformatics.ca/logic/temporallySucceeds>` `<https://example.org/transaction/isolation-write-skew-snapshot/s0>` *(in `<https://example.org/transaction/isolation-write-skew-snapshot/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/029c7c2188e7afd435ddb4f47f2348276dc8b8d7>`):
    `<https://example.org/transaction/isolation-write-skew-snapshot/conc>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/ConcurrentComposition>` *(in `<https://example.org/transaction/isolation-write-skew-snapshot/world>`)*
