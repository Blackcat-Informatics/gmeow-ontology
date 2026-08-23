<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Logic Projection Back-Ends

This directory lowers a `LogicProgram` into every target format emitted by the
logic compiler. Projection code is also responsible for explicit preservation
claims and loss ledgers, so a target must disclose what it drops.

## Module Families

| Family | Modules | Role |
| --- | --- | --- |
| Whole-program targets | `text`, `rdf` | Emit Datalog, N3, OWL DL/EL, gUFO, and canonical RDF 1.2 projections. |
| Correspondence calculus | `correspondence`, `get_leg`, `put_derivation`, `correspondence_gate`, `correspondence_gates` | Lower get/put legs and enforce law/overclaim/round-trip/mnemomorphism/composition gates. |
| Mapping artifacts | `sparql`, `edoal`, `fno`, `sssom` | Emit executable CONSTRUCT queries and alignment/function artifacts from the shared correspondence model. |
| Reports and helpers | `report`, `paths`, `tests` | Build the projection report, path-shape projections, and shared test support. |

## Boundaries

- A projection that cannot represent a construct must record the drop in the
  target's preservation ledger.
- The overclaim gate is the backstop: `ExactPreservation` with dropped content is
  a bug, not a warning.
- Keep SPARQL/EDOAL/FnO/SSSOM get-leg logic shared; do not fork target-specific
  string assembly when a structured lowering already exists.

## Checks

```bash
make rust-test
make conformance
make rust-docs
```
