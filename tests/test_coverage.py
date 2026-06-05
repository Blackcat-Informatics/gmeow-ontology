"""Tests for the coverage harness over the vendored entity slice."""

from __future__ import annotations

from gmeow_tools.coverage import run_coverage

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def test_key_entity_kinds_are_covered() -> None:
    report = run_coverage()
    # The major entity kinds in the slice must be covered by a GMEOW alignment.
    expected_covered = {
        "http://xmlns.com/foaf/0.1/Person",
        "https://schema.org/Person",
        "https://schema.org/Organization",
        "http://www.w3.org/2000/10/swap/pim/gedcom#Individual",
        "http://usefulinc.com/ns/doap#Project",
        "https://schema.org/Place",
        "https://schema.org/CreativeWork",
    }
    missing = expected_covered - report.covered_classes
    assert not missing, f"expected covered classes missing: {missing}"


def test_email_slice_terms_are_covered() -> None:
    report = run_coverage()
    # The email fixture exercises native GMEOW email + trust terms (no external
    # surface vocab) alongside schema.org participants — all must be covered.
    expected_covered = {
        "https://schema.org/EmailMessage",
        GMEOW + "EmailMessage",
        GMEOW + "Thread",
        GMEOW + "Attachment",
        GMEOW + "Mailbox",
        GMEOW + "AuthenticationResult",
        GMEOW + "DKIMSignature",
        GMEOW + "RelayHop",
        GMEOW + "TextExtraction",
        GMEOW + "Summary",
    }
    missing = expected_covered - report.covered_classes
    assert not missing, f"email classes missing: {missing}"


def test_contacts_trust_slice_covered() -> None:
    report = run_coverage()
    # The contacts revisit exercises the reified relationship + trust/WoT terms
    # plus the WOT-schema surface vocabulary.
    expected_covered = {
        GMEOW + "ProfessionalRelationship",
        GMEOW + "AcquaintanceRelationship",
        GMEOW + "CryptographicKey",
        GMEOW + "Certification",
        GMEOW + "TrustAssertion",
        "http://xmlns.com/wot/0.1/PubKey",
    }
    missing = expected_covered - report.covered_classes
    assert not missing, f"contacts-trust classes missing: {missing}"


def test_import_provenance_slice_covered() -> None:
    report = run_coverage()
    expected_covered = {
        GMEOW + "Source",
        GMEOW + "ImportActivity",
        GMEOW + "SoftwareAgent",
    }
    missing = expected_covered - report.covered_classes
    assert not missing, f"import-provenance classes missing: {missing}"


def test_slice_is_partial() -> None:
    # The slice is intentionally incomplete: there must be tracked gaps, and
    # coverage must be a real (non-zero, non-total) fraction.
    report = run_coverage()
    assert report.gap_classes, "expected some uncovered classes (slice is partial)"
    assert 0.0 < report.class_coverage <= 1.0
    assert 0.0 < report.predicate_coverage <= 1.0


def test_covered_and_gap_are_disjoint() -> None:
    report = run_coverage()
    assert not (report.covered_classes & report.gap_classes)
    assert not (report.covered_predicates & report.gap_predicates)
