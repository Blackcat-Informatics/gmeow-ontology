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


def test_rights_slice_covered() -> None:
    report = run_coverage()
    # The rights fixture exercises the IP relators, the deontic trio, the mark,
    # and the schema.org/ODRL rights cluster — all must be covered (#21).
    expected_covered = {
        GMEOW + "RightsStatement",
        GMEOW + "Copyright",
        GMEOW + "License",
        GMEOW + "Trademark",
        GMEOW + "Mark",
        GMEOW + "Permission",
        GMEOW + "Prohibition",
        GMEOW + "Duty",
    }
    missing = expected_covered - report.covered_classes
    assert not missing, f"rights classes missing: {missing}"


def test_import_provenance_slice_covered() -> None:
    report = run_coverage()
    expected_covered = {
        GMEOW + "Source",
        GMEOW + "ImportActivity",
        GMEOW + "SoftwareAgent",
    }
    missing = expected_covered - report.covered_classes
    assert not missing, f"import-provenance classes missing: {missing}"


def test_locations_slice_covered() -> None:
    report = run_coverage()
    # GMEOW location terms exercised by the places fixture.
    for cls in ("Place", "VirtualLocation", "StorageLocation", "Geometry"):
        assert GMEOW + cls in report.covered_classes
    # Aligning the address/geometry surface vocab now covers bii/paudley usage.
    assert "https://schema.org/addressLocality" in report.covered_predicates
    assert "http://www.opengis.net/ont/geosparql#asWKT" in report.covered_predicates


def test_names_slice_covered() -> None:
    report = run_coverage()
    # The names fixture exercises the universal Appellation framework, the
    # context relator, structured parts and the pronoun set — all native GMEOW.
    for cls in (
        "PersonName",
        "Filename",
        "PlaceName",
        "OrganizationName",
        "NamePart",
        "NameUsage",
        "PronounSet",
    ):
        assert GMEOW + cls in report.covered_classes
    # The co-equality machinery routes through these predicates; place naming
    # (issue #105) adds hasPlaceName + the usageAuthority facet, and nameLanguage
    # now links a first-class gmeow:Language.
    for prop in (
        "hasName",
        "fullName",
        "namePurpose",
        "usageAppellation",
        "hasPlaceName",
        "usageAuthority",
        "nameLanguage",
    ):
        assert GMEOW + prop in report.covered_predicates


def test_languages_slice_covered() -> None:
    report = run_coverage()
    # The languages fixture exercises the first-class language + writing-system
    # model, the two reified relators, and version lineage — all native GMEOW.
    for cls in (
        "Language",
        "WritingSystem",
        "WritingSystemUsage",
        "LanguageProficiency",
        "LanguageVersion",
        "ProgrammingLanguage",
        "LanguageCreation",
    ):
        assert GMEOW + cls in report.covered_classes
    for prop in ("usesWritingSystem", "knowsLanguage", "languageOrigin", "scriptRole"):
        assert GMEOW + prop in report.covered_predicates


def test_contact_field_alignments_covered() -> None:
    report = run_coverage()
    # The new SSSOM alignments move these previously-gap external IRIs (used heavily
    # in the bii/paudley fixtures) into coverage: description, url, homepage.
    for iri in (
        "https://schema.org/description",
        "https://schema.org/url",
        "http://xmlns.com/foaf/0.1/homepage",
    ):
        assert iri in report.covered_predicates


def test_standpoint_slice_covered() -> None:
    report = run_coverage()
    # The standpoint slice (#43) exercises the facility's native terms — GMEOW's
    # own, so they register as covered, not as gaps.
    expected_covered = {
        GMEOW + "Standpoint",
        GMEOW + "StandpointTenure",
        GMEOW + "accordingTo",
        GMEOW + "sharpens",
        GMEOW + "standpointModality",
    }
    missing = expected_covered - (report.covered_classes | report.covered_predicates)
    assert not missing, f"standpoint terms missing from coverage: {missing}"


def test_coreference_slice_covered() -> None:
    report = run_coverage()
    expected_covered = {
        GMEOW + "authorityLink",
        GMEOW + "counterpartOf",
        GMEOW + "versionOf",
        GMEOW + "editionOf",
        GMEOW + "supersedes",
        "https://schema.org/sameAs",
        "http://purl.org/dc/terms/isVersionOf",
    }
    missing = expected_covered - (report.covered_classes | report.covered_predicates)
    assert not missing, f"coreference terms missing from coverage: {missing}"


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
