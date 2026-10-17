# Review remediation

The active PR is 1830, with source repair scope owned by 1828 and 1829. CodeRabbit reviewed ff3cfe1c6 and found one focused test gap plus insufficient documentation coverage. Hosted acceptance remains pending; the bot's timeout is not a terminal producer failure.

1. Add a recoverable formula with the exact same get and put programs used by the lossy control. Require discharge, then mutate each leg independently and require violation. Retain the existing loss refutation, missing-leg failure and overclaim failure. Run the native correspondence execution selection.
2. Document the changed stage interface and synthetic controls with their actual contracts. Normal formatting, strict Rust checks and commit hooks remain mandatory. Publish one review repair commit and reply to the inline finding with actual evidence.
3. Refresh hosted CI and bot coverage for the new source. Reuse the prior five fixture and 47 combined tests only within their unchanged behavioral scope. Do not claim full qualification or independent completion from these controls. No independent-agent API is available; shared cache/pipeline acceptance remains open until real production execution establishes it.
