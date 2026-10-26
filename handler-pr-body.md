The standalone reasoning build now exercises the registered tool handlers directly: `lookup_term` returns typed core-segment deferral, and `reason_graph` reaches its own format validation. This addresses the optional handler-coverage comment on PR #1958.

The case uses an authored empty in-memory view and produces no corpus. It covers routing and argument refusal; successful reasoning and authenticated RPC remain outside this check. Principles 4 and 7 apply.

Validation: the focused synthetic case passed, reasoning-only all-target Clippy completed with warnings denied, formatting and normal commit hooks passed, and independent source review found no issues. The existing combined/core-only checks from #1958 remain applicable because the new case is compiled only for reasoning without core. Full corpus and hosted acceptance remain unverified under the recorded CI exception.
