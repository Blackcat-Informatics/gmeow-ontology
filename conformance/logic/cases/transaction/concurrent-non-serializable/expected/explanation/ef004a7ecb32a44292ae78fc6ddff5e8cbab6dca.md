<!-- cited-iri-skeleton
  http://www.w3.org/1999/02/22-rdf-syntax-ns#type
  https://blackcatinformatics.ca/gmeow/derivation/153bf5b03a87fb49c8e7f16b2a7eeab1b6ff3814
  https://blackcatinformatics.ca/gmeow/derivation/e92679e456a3f46619845721fdfaee50a6719d0e
  https://blackcatinformatics.ca/gmeow/reifier/7b6d9be4e7568af504f5fe77bb778fe68e376a16
  https://blackcatinformatics.ca/gmeow/reifier/ef004a7ecb32a44292ae78fc6ddff5e8cbab6dca
  https://blackcatinformatics.ca/logic/ConcurrentComposition
  https://blackcatinformatics.ca/logic/assert
  https://blackcatinformatics.ca/logic/outcome/fa50c597ea21ad7e001cf7c955f071fcb1bae479
  https://blackcatinformatics.ca/logic/rule/transaction
  https://blackcatinformatics.ca/logic/transactionSucceeds
  https://example.org/transaction/concurrent-non-serializable/conc
  https://example.org/transaction/concurrent-non-serializable/world
-->

<!-- step-skeleton
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/153bf5b03a87fb49c8e7f16b2a7eeab1b6ff3814
    rule=https://blackcatinformatics.ca/logic/rule/transaction
    term=https://blackcatinformatics.ca/logic/outcome/fa50c597ea21ad7e001cf7c955f071fcb1bae479
    term=https://blackcatinformatics.ca/logic/transactionSucceeds
  step derivation=https://blackcatinformatics.ca/gmeow/derivation/e92679e456a3f46619845721fdfaee50a6719d0e
    rule=https://blackcatinformatics.ca/logic/assert
    term=http://www.w3.org/1999/02/22-rdf-syntax-ns#type
    term=https://blackcatinformatics.ca/logic/ConcurrentComposition
    term=https://example.org/transaction/concurrent-non-serializable/conc
-->

# Explanation for `<https://blackcatinformatics.ca/gmeow/reifier/ef004a7ecb32a44292ae78fc6ddff5e8cbab6dca>`

**World:** `<https://example.org/transaction/concurrent-non-serializable/world>`
**Target derivation:** `<https://blackcatinformatics.ca/gmeow/derivation/153bf5b03a87fb49c8e7f16b2a7eeab1b6ff3814>`

**Derived** by rule `<https://blackcatinformatics.ca/logic/rule/transaction>`:
  `<https://blackcatinformatics.ca/logic/outcome/fa50c597ea21ad7e001cf7c955f071fcb1bae479>` `<https://blackcatinformatics.ca/logic/transactionSucceeds>` `"true"^^<http://www.w3.org/2001/XMLSchema#boolean>` *(in `<https://example.org/transaction/concurrent-non-serializable/world>`)*
  **Asserted fact** (input — `<https://blackcatinformatics.ca/gmeow/reifier/7b6d9be4e7568af504f5fe77bb778fe68e376a16>`):
    `<https://example.org/transaction/concurrent-non-serializable/conc>` `<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>` `<https://blackcatinformatics.ca/logic/ConcurrentComposition>` *(in `<https://example.org/transaction/concurrent-non-serializable/world>`)*
