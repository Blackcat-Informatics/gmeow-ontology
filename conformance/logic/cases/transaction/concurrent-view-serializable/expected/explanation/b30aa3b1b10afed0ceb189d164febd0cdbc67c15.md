<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/88395feea05061abea545edf568d0b8b87d466d6
  https://blackcatinformatics.ca/gmeow/derivation/f3c9cac0e8b313de4d069526c26b93da9249bf87
  https://blackcatinformatics.ca/gmeow/reifier/b30aa3b1b10afed0ceb189d164febd0cdbc67c15
  https://blackcatinformatics.ca/gmeow/reifier/ca82fe3f33942586046694f5583ba7e7f5707e34
  https://blackcatinformatics.ca/logic/ConcurrentComposition
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/rule/transaction
  https://blackcatinformatics.ca/logic/temporallySucceeds
  https://blackcatinformatics.ca/logic/txstate/fdb523b877382a22b705b8a099122b1c74dc4c40
  https://example.org/transaction/concurrent-view-serializable/conc
  https://example.org/transaction/concurrent-view-serializable/s0
  https://example.org/transaction/concurrent-view-serializable/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/f3c9cac0e8b313de4d069526c26b93da9249bf87
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=https://blackcatinformatics.ca/logic/temporallySucceeds
    term=https://blackcatinformatics.ca/logic/txstate/fdb523b877382a22b705b8a099122b1c74dc4c40
    term=https://example.org/transaction/concurrent-view-serializable/s0
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/88395feea05061abea545edf568d0b8b87d466d6
    rule=https://blackcatinformatics.ca/logic/assert
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/ConcurrentComposition
    term=https://example.org/transaction/concurrent-view-serializable/conc
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/b30aa3b1b10afed0ceb189d164febd0cdbc67c15>`

**World:** `<https://example.org/transaction/concurrent-view-serializable/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/f3c9cac0e8b313de4d069526c26b93da9249bf87>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/txstate/fdb523b877382a22b705b8a099122b1c74dc4c40>` `<https://blackcatinformatics.ca/logic/temporallySucceeds>` `<https://example.org/transaction/concurrent-view-serializable/s0>` *(in `<https://example.org/transaction/concurrent-view-serializable/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/ca82fe3f33942586046694f5583ba7e7f5707e34>`):
    `<https://example.org/transaction/concurrent-view-serializable/conc>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/ConcurrentComposition>` *(in `<https://example.org/transaction/concurrent-view-serializable/world>`)*
