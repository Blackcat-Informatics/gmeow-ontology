<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rendered-site cache JSON

The shared site/book cache envelope uses native `purrdf_lex::json` records.
Compact UTF-8 output keeps declaration order `digest`, `files`, without a
terminal newline. `files` maps lexically sorted paths to exact UTF-8 text;
Unicode, control escaping, empty strings and file contents retain their bytes
after decoding. Non-UTF-8 file bytes remain a hard failure before caching.

Both fields are required. `digest` must be a string, `files` an object of string
values. Known duplicate envelope fields refuse, unknown fields remain ignored,
and repeated file-map names keep the last decoded string. Native records require
objects; incidental positional arrays formerly admitted by Serde are not a cache
input format. Ignored metadata may contain any native RFC JSON value, including
numeric lexemes outside binary64 range; numbers are never admitted as file text.

The canonical files payload is the native compact JSON string map, with lexical
key order. The existing shared documentation payload-digest owner consumes these
bytes directly. Its SHA-1 feed remains exactly:
`gmeow-docs-fixture-payload + US + label + RS + canonical-payload`.
The label remains `site`, including book artifacts. The digest excludes itself
and the envelope; whitespace or escaped spelling changes that decode to the same
files cannot alter it. No new cache identity, codec version or fallback is added.

The model payload remains with its existing Serde codec and supplies its existing
canonical bytes to the same digest/verification owner. Action-cache receipt
metadata and minijinja values retain their separate Serde owners. The docs crate
no longer needs production `serde_json`; independent tests retain the dev edge.

Both cache reads retain their existing context and receipt checks. The selected
read-only loader still verifies its selector before decoding; the explicit
producer's warm-cache read still validates its render receipt. `into_site`
still verifies the payload before reconstructing files.
Corrupt or missing selected entries fail closed without rendering or model
discovery. Only the explicit producer may build an absent render action.
Pure JSON controls use authored Site values and bytes; they never invoke that
producer, action-store construction, real rendering, discovery or corpus loading.
