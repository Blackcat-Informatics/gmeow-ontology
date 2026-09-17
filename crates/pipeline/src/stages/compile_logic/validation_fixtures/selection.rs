// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact standalone module and counterexample selections owned by the producer.

pub(super) const MODULES: &[&str] = &[
    "slices/core/attestation/module.ttl",
    "slices/core/gender/module.ttl",
    "slices/core/gts/module.ttl",
    "slices/core/norms/module.ttl",
    "slices/core/observations/module.ttl",
    "slices/core/pipeline/module.ttl",
    "slices/core/rights/module.ttl",
    "slices/core/standpoint/module.ttl",
    "slices/grounding/lang/module.ttl",
    "slices/grounding/math/module.ttl",
];

pub(super) const CASES: &[(&str, &str)] = &[
    (
        "slices/grounding/math/module.ttl",
        "slices/grounding/math/tests/counter-examples/metric-signature-dimension-mismatch.ttl",
    ),
    (
        "slices/grounding/lang/module.ttl",
        "slices/grounding/lang/tests/counter-examples/gmn-compaction-overclaim.ttl",
    ),
    (
        "slices/grounding/lang/module.ttl",
        "slices/grounding/lang/tests/counter-examples/gmn-version-overclaim.ttl",
    ),
    (
        "slices/core/observations/module.ttl",
        "slices/grounding/lang/tests/counter-examples/meaning-act-observation-conflation.ttl",
    ),
    (
        "slices/core/observations/module.ttl",
        "slices/grounding/lang/tests/counter-examples/meaning-ungrounded-claim.ttl",
    ),
    (
        "slices/core/gender/module.ttl",
        "tests/fixtures/shapes/suppression-warning-only.ttl",
    ),
    (
        "slices/core/standpoint/module.ttl",
        "tests/fixtures/shapes/standpoint-credence-band-violation.ttl",
    ),
    (
        "slices/core/rights/module.ttl",
        "tests/fixtures/shapes/privacy-malformed.ttl",
    ),
    (
        "slices/core/standpoint/module.ttl",
        "tests/fixtures/shapes/standpoint-preferred-violation.ttl",
    ),
    (
        "slices/core/norms/module.ttl",
        "tests/fixtures/shapes/rubrics-malformed.ttl",
    ),
    (
        "slices/core/attestation/module.ttl",
        "slices/core/attestation/tests/counter-examples/pin-disagreement.ttl",
    ),
    (
        "slices/core/attestation/module.ttl",
        "slices/core/attestation/tests/counter-examples/pin-missing-site.ttl",
    ),
    (
        "slices/core/attestation/module.ttl",
        "slices/core/attestation/tests/conformance-fixtures/pin-reconciliation-holds.ttl",
    ),
];
