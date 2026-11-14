
## Hosted checks at 74e334917 (the head that merges; main merged in at f71169a78)

Green: `helpers`, `lint`, `producer-build`, `rust-prebuild`.

Red, each red on `main` for the same cause (the producer does not finish on the hosted runner):

| Check | This head | `main` |
|---|---|---|
| `generation (a)` | The hosted runner lost communication with the server | f71169a78: The operation was canceled |
| `generation (b)` | The job has exceeded the maximum execution time of 2h0m0s | f71169a78: Process completed with exit code 143 |
| `fixture-prefix` | The operation was canceled | 04aa28a7e: runner lost in "Produce optimized corpus fixtures" |
| `quality` | a quality job did not succeed (skipped) | the same line on 04aa28a7e |

Independent review at this exact head: PASS (its one required finding, a reply that misdescribed the example
file, is corrected in the review thread).

Not shown: no producer run. The annotation gate, the derived minimum-one shapes from the three closure
entries, the qualified cardinalities under reasoning, the example-loading gate and the translation coverage
gates are unchecked for this change.
