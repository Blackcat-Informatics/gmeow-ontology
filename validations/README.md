<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# `validations/` — external validator-zoo lanes

This directory is **deliberately outside the repository's normal workflow**. Nothing here
is run by `make check`, by the `maint-*` maintainer lanes, or by CI. Ordinary contributors
and even maintainers are not expected to run it.

These lanes are **standalone tooling for outside parties** — people validating GMEOW's
cross-standard claims, or building bridges between GMEOW and an external standard — who want
to confirm, against the *external* standard's own reference validators, that a GMEOW
projection is accepted by that ecosystem. Each lane vendors the external artifacts it needs,
stands up (or drives) the external validator, and reports a single falsifiable outcome:
either a **PASS** or a **named boundary** (the exact field the external validator rejects).

## Layout

One self-contained subdirectory per lane, each with its own `Makefile` and `README.md`:

| Lane | What it proves |
|------|----------------|
| [`openehr-bloodpressure/`](./openehr-bloodpressure/) | The down-projection `d(g)` = `blood_pressure.augmented.json` (a valid openEHR composition + the GMEOW in-band complement) validates under the real `Blutdruck.opt` Operational Template, in an openEHR reference CDR (EHRbase) or the Archie RM validator — the empirical half of the openEHR blood-pressure section/retraction claim (`docs/APPLIED_CATEGORY_THEORY/usecase_openehr_bloodpressure.md`). |

## Running a lane

```bash
make -C validations/<lane>          # the lane's default target prints PASS or BOUNDARY
```

Each lane documents its own prerequisites (Docker, Java, network) in its `README.md`. A lane
is allowed to need heavy external tooling precisely because it lives outside the Docker-free
`make check` gate.
