The native source-support lookup repair is implemented in the isolated branch. Immutable validation batches build a proof index once; growing class admission uses the existing evidence owner's checked proof positions. Missing/wrong proof IDs, source/world/contract isolation and the serialized ledger remain enforced.

Twenty focused native evidence/class proof controls passed. Initial synthetic class-operation measurements, including retained-proof validation, are:

| Descriptors | Baseline point estimate | Candidate point estimate |
| --- | --- | --- |
| 1250 | 8.6553 s | 0.50117 s |
| 2500 | 45.919 s | 1.3018 s |

That is about 17x and 35x for these cases, with the same harness, dependency lock and normal bench profile. Every descriptor must retain its original onProperty proof and the class outcome must actually engage. The old baseline binary is still collecting the 5000-descriptor case; its matching candidate run is pending. This is not a full-pipeline speedup claim.

The change is uncommitted and unqualified. The current base still has known helper-home and corpus-purity gate defects already repaired in the combined candidate; those must be integrated without raising ceilings. Lint passed before the final Make filter parameter. Independent review, full qualification and ghprsq integration remain open. Principles 4, 7, 12, 17, 18.
