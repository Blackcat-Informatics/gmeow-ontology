
## Hosted checks at c5053878f (the head that merges; main merged in at f71169a78)

Green: `helpers`, `lint`, `producer-build`, `rust-prebuild`.

Red, each red on `main` for the same cause (the producer exceeds the hosted runner's memory):

| Check | This head | `main` |
|---|---|---|
| `generation (a)` | The hosted runner lost communication with the server | f71169a78: The operation was canceled; 04aa28a7e: ended with no step conclusion in the same step |
| `generation (b)` | The operation was canceled | f71169a78: Process completed with exit code 143 |
| `fixture-prefix` | The hosted runner lost communication with the server | 04aa28a7e: ended in "Produce optimized corpus fixtures", runner lost |
| `quality` | a quality job did not succeed (skipped) | the same line on 04aa28a7e |

The review bot could not review this pull request (its usage limit). An independent review at this exact head
stands in for it: PASS, no required finding.

Not shown: no producer run. The annotation gate, the shapes derived from the domain and range, reasoning and
the translation coverage gates are unchecked for this change.
