The medium winner-table writer and reader now use native JSON for the complete five-record family. The implementation preserves declaration-order pretty output, its final newline, vector order, fraction strings, required fields, ignored unknown members and strict integer token bounds. Schema admission happens before complete decoding from the same parsed value. Typed records require objects; incidental positional arrays are rejected explicitly.

Dictionary selection now refuses an unrepresentable target size with an actionable diagnostic instead of saturating the conversion. Existing strategy and training behavior remains unchanged. Actual sweep/seed writers and dictionary-stage readers retain their public APIs.

All ten authored-data controls passed, including eight new wire/refusal/boundary controls and two existing roundtrip/strategy controls. Strict prebuild, helpers, source admission, lint, formatting and independent source review passed. Normal signing and hooks remain mandatory. Actual authenticated winner-table and installed-consumer execution remains unverified.

Refs #1776. Principles 4 and 7. The parent migration remains open. Actual committed measurements, authenticated corpus, installed consumers and narrower-platform execution remain unverified under the campaign exception.
