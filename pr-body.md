The i18n lint observation path now writes diagnostic identity tuples with native JSON. Compact `[rule,[subjects...]]` bytes, string escaping, subject order and empty-list behavior stay stable, so the surrounding ledger identity and diagnostic context remain unchanged.

Three authored controls cover exact focus bytes, escaping/empty inputs and tuple boundaries; the existing wording/grade control is also selected. Independent source review found no actionable issue. All four selected controls, strict producer-independent Clippy, helpers, source admission and formatting passed; repository i18n lint, corpus production, installed and full consumer validation remain unrun under the recorded campaign exception. Remaining docs-model Serde consumers are retained.

Refs #1776. Principles 4 and 7.
