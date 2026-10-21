Transcode loss reports and matrices now use the existing PurRDF JSON record/value/writer owners. Ordered record fields, loss-ledger aggregation, codec filtering and exact newline contracts remain preserved. Free-form matrix documents retain sorted last-wins objects; the infallible value writer removes silent empty-array serialization fallbacks.

Principles 4 and 7. Refs #1776; this completes the transcode package unit after perf-evidence PR1904, while other migrations and full workspace acceptance remain open.

Validation: seven focused pure tests passed, including literal byte/order/escaping/aggregation controls and malformed static-owner refusals; strict producer-independent prebuild Clippy and formatting passed. Generated-artifact parity, installed/actual MCP execution, consumer-inclusive full Clippy and local corpus/full gates remain unrun under the campaign exception.
