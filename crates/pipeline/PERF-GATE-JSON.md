<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Perf-gate merge JSON

`make perf-gate` writes `validate.json`, `sync.json` and `reason-verify.json`
before invoking `perf-gate-merge`. The merger requires an object document from
each file, its exact corresponding `command` string (`validate`, `sync`,
`reason-verify`), and object-valued `deterministic_work` and `observations`.
Missing or mistyped inputs fail with the binary's graded `.input` diagnostic;
they never become successful null projections. Parse/read/write failures retain
their separate diagnostic sites and filesystem behavior.

Inner payloads, unknown fields and input schema versions are opaque. Empty
objects, null values, heterogeneous arrays and profile-specific omissions are
retained. In particular, sync cache hits need no fabricated stage timings and
reason-verify's absent budget limit remains null. No receipt or identity is minted.

Native `read_document` resolves duplicate object names recursively last-wins.
Numbers retain their RFC token spelling, including large integers, `-0`,
fractions and exponents such as `1e400`, even in unused fields. This deliberately
replaces Serde's numeric normalization/range admission; it does not reinterpret
numbers as identities or normalize opaque payloads through floats.

The full resolved documents remain in `commands`. Both projection arrays retain
the same fixed file order. The aggregate stays version 1 with command `perf-gate`:
`deterministic_work` rows contain `command` and `work`; `observations` rows contain
`command` and `observations`. All object keys are recursively sorted; arrays retain
order. Native output is two-space pretty UTF-8 with exactly one terminal LF.

The performance sampler records the SHA-256 of the actual merged output bytes
and reads the retained `commands`; no previous byte identity is claimed when
numeric spelling changes. Sampling, measurement, cache accounting and comparison
tolerances remain with their existing owners. Pure decoder/merge controls use
authored JSON only and never collect timings or invoke engines or production.
