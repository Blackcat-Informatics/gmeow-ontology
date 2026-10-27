Source-selection digests and source-document receipt hashing now use native PurRDF JSON. Four complete selection writers preserve every field, Rust tag and ordered set; receipt encoding reuses the existing complete document/base/origin writers. Both production encoders use native compact bytes. Principles 4 and 7.

Every identity byte is preserved: declaration order, Unicode/NUL escaping, the selection hash domain, sorted encoded receipts, big-endian receipt lengths and all existing graph/quad/reifier/annotation framing. Shared Serde, the direct streaming admission evidence commitment, public digest APIs, admission decisions and persisted publication authority remain unchanged.

Original references were captured before production edits. The two existing authored identity controls compare twelve complete selection preimages and their actual original digests, two complete sorted receipt preimages and the actual original whole-source digest. The unchanged persisted-admission roundtrip also passed. Strict all-target compiler/logic/pipeline/developer CLI compilation passed with warnings denied; independent combined review found no issues. Final runtime: three selected cases passed after 43.02 seconds compilation and 0.017 second summary, with 1,067 excluded. Complete helper hygiene and static producer-recipe admission passed. All normal signed commit hooks passed.

No local optimized producer, canonical corpus construction, sync or full gate ran. No browser, authenticated corpus, shipped assets, installed consumer or release pass is claimed. CI remains explicitly waived rather than passing; complete native admission publication/hydration and all parent acceptance remain open.

Closes #2026
