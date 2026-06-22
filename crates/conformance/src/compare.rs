// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The three comparison modes and the per-case `diff_case`.
//!
//! Implemented in Task 2 (comparators) + Task 5 (`diff_case`):
//! * `compare_rdf` — RDFC-1.0 graph-isomorphism.
//! * `compare_canonical_json` — sorted-key JSON equality.
//! * `compare_explanation_skeleton` — cited-IRI set equality (never prose).
