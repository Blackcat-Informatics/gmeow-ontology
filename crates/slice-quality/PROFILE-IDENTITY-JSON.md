<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Quality profile identity JSON

`ProfileAssessment::new` hashes native compact JSON bytes for the selected
`QualityProfile`. Its record fields stay in declaration order: `iri`, `key`,
`target_tier`, `axes`. Axis keys follow the existing sorted-map order. Applicability
records carry `status` first and a `reason` only for the two reason-bearing cases.
Empty strings/maps remain explicit, string escaping stays unchanged, and there is
no trailing newline. This contract contains no numbers.

The existing plain BLAKE3 hash and `blake3:` spelling remain unchanged. Literal
byte controls and an independently computed digest exercise the actual standalone
assessment path using tiny authored inputs. This does not qualify a real corpus
or installed bundle.

Serde derives remain for the current diagnostic-report metadata owner, including
assessment outcomes. No JSON bridge or new external package was added. That separate
metadata exit and the full dependency migration remain outstanding. The native
codec is selected through the existing pinned workspace dependency.
