<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Draft 2020-12 meta-schemas

The nine JSON files are unmodified documents retrieved from the published
`https://json-schema.org/draft/2020-12/` endpoints on 2026-10-19. Each file's
adjacent `.license` records its exact retrieval URL and upstream copyright.
Their `$id` values are the registration URIs used by the instance validator.

The specification repository licenses its contents under either BSD-3-Clause
or AFL-3.0. This distribution chooses BSD-3-Clause;
[`LICENSES/BSD-3-Clause.txt`](../../../../LICENSES/BSD-3-Clause.txt) retains that
complete grant and disclaimer, retrieved from
<https://github.com/json-schema-org/json-schema-spec/blob/main/LICENSE>.
The upstream licensing statement is in
<https://github.com/json-schema-org/json-schema-spec#license>.

These are runtime schema inputs embedded in the consumer binary, including the
format-assertion vocabulary meta-schema. They are not an engine conformance
suite. No schema or reference is fetched at runtime. Updating these inputs
requires reviewing the published documents and rerunning instance validation.
