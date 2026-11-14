
## Hosted checks at 4d376c7c1 (the head that merges)

Green: `helpers` (including the new locked fuzz resolution), `lint`, `producer-build`, `rust-prebuild`.

Pull request checks that are red, each red on `main` for the same cause (the producer does not finish on the hosted runner):

| Check | This head | `main` |
|---|---|---|
| `generation (a)` | The operation was canceled | f71169a78: The operation was canceled |
| `generation (b)` | exceeded the maximum execution time of 2h0m0s | f71169a78: Process completed with exit code 143 |
| `fixture-prefix` | The operation was canceled | 04aa28a7e: runner lost in "Produce optimized corpus fixtures" |
| `quality` | a quality job did not succeed (skipped) | the same line on 04aa28a7e |

The dispatched scheduled workflows on this commit are also red. They are red on `main` too, but not at the same lines: on `main` each fails at the step this change repairs; here each passes that step and fails at the next one. Those next failures are filed and are not fixed by this change:

| Job | `main` | This head |
|---|---|---|
| fuzz | stale fuzz lock, `--locked` refused | lock and substrate check pass; cancelled at its 60 minute limit while compiling |
| coverage, heavy | corpus scanner exits 127 (ripgrep absent) | scanner passes; no rule to make a generated codebook file |
| benchmarks | instruction-count runner not installed | instruction benches pass; a later benchmark panics for missing generated shapes |
| mutation | 13 failing tests in the unmutated tree | unchanged (not in scope) |

Independent review at this exact head: PASS.
