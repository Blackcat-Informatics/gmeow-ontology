The source-law merge #1831 reproduced this exact failure in CI run [38956864396, generation (a)](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38956864396/job/117169103488):

`sync pipeline failed: recovery evidence has no executable resolved leg pair`

This is the existing defect owned here, not a distinct new failure. Repair PR #1830 supplies executable legs and retains the honest-loss refutation, with a recoverable control and independent body mutations added after CodeRabbit review. Its current run 38948027790 has passed producer-build, rust-prebuild and lint; generation and fixture production remain live. Full qualification is not yet claimed. Keeping this issue open until that repair lands and its acceptance is proven.
