Review follow-up is pushed through 1d5d7f36d (Principles 1, 4, 7, 12, 17 and 18):

- d3ca06032 preserves ordinary distinct_pairs and separates restricted-blocking premises from the deliberately coarser vocabulary quotient. A reproduced cycle exhausted 40 steps while previously receiving a false certificate because two ordinary data URIs were identified. It is now refused; valid mixed operator spellings remain certifiable. Unknown-count summaries cannot supply two guaranteed witnesses; the selected count-1 expansion is covered.
- 551dee071 compares admitted anyURI enumeration values using the substrate's XML whitespace normalization and preserves unknown foreign/nonmatching values.
- 13a44f7a9 integrates the native artifact-test fixture repair from #1782. Real execution evidence is retained and missing/mismatched evidence is rejected.
- 1d5d7f36d removes the unused csv/flate2 dependencies and refreshes the existing locks without changing PurRDF selections or compiler settings.

Verified on the combined working source: make fmt, make lint, and 152 focused tests (nextest d240e531-3bac-4fe6-b5bb-3141da4549f5; 3801 skipped). These include actual SHACL consumer controls for the numeric-range repair. That canonical math source/compiler change is still uncommitted pending its required full qualification, so its review finding stays open.

Full production/corpus qualification, both authenticated artifact-verifier cases, cold-generation comparison and independent completion review remain open. #1805 is now in active implementation on an isolated branch; its plan is published. No merge or issue-completion claim is made.

@coderabbitai review
