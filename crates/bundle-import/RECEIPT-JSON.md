<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Import receipt JSON

`ImportReceipt` and its immutable `ReceiptEnvelope` use native JSON records at
publication, replay and garbage-collection boundaries. Every declared field is
required. Known duplicate fields and null/wrong-kind values are refused; unknown
fields are ignored. Records must be objects, not positional arrays. Unsigned
fields admit integer tokens within their declared width, excluding negative
zero, negative values, fractions and exponents.

Compact receipt bytes retain declaration order and UTF-8 string escaping. The
receipt digest hashes the existing domain followed by those complete bytes using
the existing field separators. Envelope publication uses two-space pretty JSON
in declaration order with no terminal newline. Runtime observations are excluded
from the receipt. Replay and GC retain their original context, digest, bounded
read, protected-root and corruption checks; absent receipts remain cache misses.

The fixture selector and bundle-derived artifact publication/payload records are
separate Serde owners. `ImportReceipt` retains its Serde traits for that enclosing
selector. The native receipt operation neither replaces nor reparses through
that selector boundary.

The `json_receipt_` controls use independently authored JSON, inert receipt
metadata and temporary receipt/GC directories. They do not import a dataset,
construct a GTS container or execute a producer.
