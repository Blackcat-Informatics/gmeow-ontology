<!-- cited-iri-skeleton
  https://blackcatinformatics.ca/gmeow/derivation/9853ba306af17d1ecc4ae93f05e14e175dd25264
  https://blackcatinformatics.ca/gmeow/derivation/ffa8cce528ff30ba8e75ebe46a892c3a78542cd1
  https://blackcatinformatics.ca/gmeow/reifier/059dd4339502c9933c760f3f219db51e15db4579
  https://blackcatinformatics.ca/gmeow/reifier/fa34124ac2efeb918be7af50cd0d1a2bfff28f05
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/effect
  https://blackcatinformatics.ca/logic/rule/transaction
  https://blackcatinformatics.ca/logic/situationObtains
  https://blackcatinformatics.ca/logic/txstate/419748c47d7bdfb04ff709580c297f8ac25b5ab2
  https://example.org/transaction/concurrent-serializable/effL
  https://example.org/transaction/concurrent-serializable/schemaL
  https://example.org/transaction/concurrent-serializable/shared
  https://example.org/transaction/concurrent-serializable/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/ffa8cce528ff30ba8e75ebe46a892c3a78542cd1
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=https://blackcatinformatics.ca/logic/situationObtains
    term=https://blackcatinformatics.ca/logic/txstate/419748c47d7bdfb04ff709580c297f8ac25b5ab2
    term=https://example.org/transaction/concurrent-serializable/shared
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/9853ba306af17d1ecc4ae93f05e14e175dd25264
    rule=https://blackcatinformatics.ca/logic/assert
    term=https://blackcatinformatics.ca/logic/effect
    term=https://example.org/transaction/concurrent-serializable/effL
    term=https://example.org/transaction/concurrent-serializable/schemaL
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/fa34124ac2efeb918be7af50cd0d1a2bfff28f05>`

**World:** `<https://example.org/transaction/concurrent-serializable/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/ffa8cce528ff30ba8e75ebe46a892c3a78542cd1>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/txstate/419748c47d7bdfb04ff709580c297f8ac25b5ab2>` `<https://blackcatinformatics.ca/logic/situationObtains>` `<https://example.org/transaction/concurrent-serializable/shared>` *(in `<https://example.org/transaction/concurrent-serializable/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/059dd4339502c9933c760f3f219db51e15db4579>`):
    `<https://example.org/transaction/concurrent-serializable/schemaL>` `<https://blackcatinformatics.ca/logic/effect>` `<https://example.org/transaction/concurrent-serializable/effL>` *(in `<https://example.org/transaction/concurrent-serializable/world>`)*
