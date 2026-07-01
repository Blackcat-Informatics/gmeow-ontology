<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/017d40c0f612852c0596ac5f176c0876796c70bb
  https://blackcatinformatics.ca/gmeow/derivation/6ebe2c7110a7fdd4e1b8057c9a3e7375adb90367
  https://blackcatinformatics.ca/gmeow/reifier/0cae65dc3ca97e00246ae6c3f9345be2377b8841
  https://blackcatinformatics.ca/gmeow/reifier/feda8a81ac849ef6edb725a25fce4d3c83af1de5
  https://blackcatinformatics.ca/logic/ConcurrentComposition
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/rule/transaction
  https://blackcatinformatics.ca/logic/temporallySucceeds
  https://blackcatinformatics.ca/logic/txstate/9a431450edf62a0a15891214894a0b5c205eb549
  https://example.org/transaction/isolation-write-skew-serializable/conc
  https://example.org/transaction/isolation-write-skew-serializable/s0
  https://example.org/transaction/isolation-write-skew-serializable/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/017d40c0f612852c0596ac5f176c0876796c70bb
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=https://blackcatinformatics.ca/logic/temporallySucceeds
    term=https://blackcatinformatics.ca/logic/txstate/9a431450edf62a0a15891214894a0b5c205eb549
    term=https://example.org/transaction/isolation-write-skew-serializable/s0
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/6ebe2c7110a7fdd4e1b8057c9a3e7375adb90367
    rule=https://blackcatinformatics.ca/logic/assert
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/ConcurrentComposition
    term=https://example.org/transaction/isolation-write-skew-serializable/conc
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/feda8a81ac849ef6edb725a25fce4d3c83af1de5>`

**World:** `<https://example.org/transaction/isolation-write-skew-serializable/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/017d40c0f612852c0596ac5f176c0876796c70bb>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/txstate/9a431450edf62a0a15891214894a0b5c205eb549>` `<https://blackcatinformatics.ca/logic/temporallySucceeds>` `<https://example.org/transaction/isolation-write-skew-serializable/s0>` *(in `<https://example.org/transaction/isolation-write-skew-serializable/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/0cae65dc3ca97e00246ae6c3f9345be2377b8841>`):
    `<https://example.org/transaction/isolation-write-skew-serializable/conc>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/ConcurrentComposition>` *(in `<https://example.org/transaction/isolation-write-skew-serializable/world>`)*
