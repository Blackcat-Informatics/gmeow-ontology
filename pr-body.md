The quality assessment now hashes native JSON records for the selected profile. The writer preserves field and applicability order, sorted axes, string escaping, explicit empty fields, and the existing BLAKE3 identity. The separate diagnostic metadata output still uses Serde.

Eight focused tests passed: three new exact-byte and independent-digest controls through the actual standalone assessment path, plus five existing profile/refusal/report controls. Strict producer-independent Clippy, helper hygiene and source-input validation passed. Commit and push hooks run normally. Full corpus, installed consumers and dependency migration remain open under the recorded campaign exception.

Also correct two comments describing ignored pipeline-generated cost/soak outputs as committed artifacts. The tracked cost baseline and renderer behavior are unchanged; generated files remain producer-owned.

Refs #1776. Principles 4 and 7.
