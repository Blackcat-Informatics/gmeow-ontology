<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-gts-profile

The mandatory GMEOW GTS authorship profile: the **one** production entry that
emits a GMEOW bundle, and the wire-level audit that every payload-bearing frame
carries `zstd-rsyncable` at compression level 12.

## Why this is a leaf crate

The profile is a distribution contract, so it only holds if *every* bundle author
goes through it. It previously lived inside `gmeow-pipeline`, which put it out of
reach of two production authors:

- `gmeow-math` — `gmeow-pipeline` depends on `gmeow-math`, so the edge cannot be
  reversed;
- `gmeow-music` — no dependency on the pipeline at all.

Both therefore called `purrdf::gts_compose::emit_gts` directly, and the
single-entry claim in the profile's own documentation was false. Depending only
on `purrdf`, `gmeow-errors` and `ciborium`, this crate sits below every author, so
the narrow waist is real rather than aspirational.

## Surface

| Item | Role |
|---|---|
| `emit_gmeow_gts` | The only production call to `purrdf::gts_compose::emit_gts` in the workspace |
| `validate_mandated_frames` | Cheap CBOR wire audit of a materialized bundle; does not fold or parse the RDF payload |
| `GMEOW_GTS_FRAME_TRANSFORM` / `GMEOW_GTS_ZSTD_LEVEL` | The pinned contract values |
| `error::Profile` | The `gts-profile.frame` diagnostic kind every violation is reported as |

A compile-time assertion pins `purrdf::gts_compose::DIST_ZSTD_LEVEL` to 12, so an
upstream default drift is a build failure rather than a silently different
committed bundle.
