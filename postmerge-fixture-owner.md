The source-law merge #1831 reproduced this exact failure in CI run [38956864396, fixture-prefix](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38956864396/job/117169103468):

`stage stage-goal-directed failed: cross-process fixture requires stable persistent admission, got .../stabilityStablePrefix / .../cacheRecomputeAggregate`

This is the existing defect owned here. PR #1830 corrects the Rust and canonical stage declarations together, retains the independent output contract, changes implementation identity and adds read-only admission coverage for every selected fixture. Its current run 38948027790 has passed producer-build, rust-prebuild and lint; generation and fixture production remain live. Full qualification is not yet claimed. Keeping this issue open until that repair lands and its acceptance is proven.
