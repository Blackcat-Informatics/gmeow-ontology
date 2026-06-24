"""Generate a CrossRef deposit document for the GMEOW DOI(s).

Blackcat Informatics® mints GMEOW's DOI as a CrossRef member (its own prefix),
not via Zenodo. CrossRef registration is by depositing XML conforming to the
CrossRef deposit schema (5.4.0); an ontology is deposited as a ``<database>`` /
``<dataset>`` record whose DOI resolves to a landing page.

GMEOW follows a **single-anchor** DOI strategy (see ``docs/dois.md``):

* a **concept DOI** denotes the Work (always-latest citation anchor); it
  resolves to the concept IRI (:data:`ONTOLOGY_IRI`);
* an **optional version DOI** denotes the immutable release (the Manifestation);
  it resolves to the release ``owl:versionIRI``. Concept-only is a valid state.

The deposit uses the *whole* schema, not just ``<dataset type="record">``:
contributors (organization + ORCID-identified persons), description, publication
and update dates, publisher and institution identifiers, ``publisher_item``
identifiers, ``version_info``, ``format``, the **AccessIndicators** license
program (``ai:program`` — the CC license made machine-readable for version-of-
record and TDM uses), text-mining full-text URLs, references, and a rich
relations program (``rel:program``):

* ``hasFormat`` intra-work relations to every published serialization (the
  Crossref-native analog of the FAIR Signposting ``item`` links);
* ``isSupplementedBy`` → the source repository;
* the concept↔version edge → ``isVersionOf`` / ``hasVersion`` (read by FRBR role);
* the curated alignment targets (:data:`ALIGNMENT_TARGETS`) → ``isDerivedFrom``
  (upper ontologies) / ``references`` (peer schemas) so the deposit is a
  first-class PID-graph node;
* a documented (unpopulated) ``<component_list>`` seam where future per-profile
  sub-DOIs would attach with ``parent_relation="isPartOf"``.

Granularity (modules, mapping-sets, profiles) and provenance are NOT minted as
DOIs — they ride the content-addressed identifier triangle (``owl:versionIRI`` ↔
SWHID / ``gmeow:gtsHeadId`` / ``gmeow:contentDigest``).

The output is well-formed and follows the schema's element model; validate it
against the official XSD (deposit 5.4.0 + ``AccessIndicators.xsd`` +
``relations.xsd``) and test on the CrossRef *test* system before a production
deposit.
"""

from __future__ import annotations

import datetime
import json
from pathlib import Path

from gmeow_validate import build_deposit_xml_native, lint_deposit_native

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    CROSSMARK_ENABLED,
    CROSSMARK_POLICY_DOI,
    DATASET_SLUG,
    DEPOSIT_FORMAT,
    DIST_DIR,
    ONTOLOGY_FILE,
    ONTOLOGY_IRI,
    PROJECT_ROOT,
    REGISTRANT_ACRONYM,
    REGISTRANT_PLACE,
)
from gmeow_tools.self_desc import SelfDescription, load_self_description

#: CrossRef deposit schema namespace (version 5.4.0).
CR_NS = "http://www.crossref.org/schema/5.4.0"
#: CrossRef access-indicators (license) program namespace.
AI_NS = "http://www.crossref.org/AccessIndicators.xsd"
#: CrossRef relationships program namespace (relations.xsd).
REL_NS = "http://www.crossref.org/relations.xsd"

#: CITATION.cff — checked for DOI consistency by doi-lint.
_CITATION_CFF = PROJECT_ROOT / "CITATION.cff"


def _live_stamp(description: SelfDescription) -> tuple[str, str]:
    """Return a (timestamp, batch_id) pair for a fresh submission.

    The deposit is a transient submission document, not a committed artifact, so
    the timestamp is the current UTC time: CrossRef uses it to order (re)submissions
    of the same DOI, and a correction must out-rank the previous deposit. The batch
    id embeds it so each generated deposit is uniquely identifiable.
    """
    timestamp = datetime.datetime.now(datetime.UTC).strftime("%Y%m%d%H%M%S")
    batch_id = f"gmeow-{description.version}-{timestamp}"
    return timestamp, batch_id


def _to_deposit_input_json(description: SelfDescription) -> str:
    """Serialise a ``SelfDescription`` and runtime config to JSON for the Rust backend.

    The returned JSON encodes a ``DepositInput`` struct understood by
    ``gmeow_validate.build_deposit_xml_native``.
    """
    contributors = [
        {
            "kind": c.kind,
            "name": c.name,
            "orcid": c.orcid,
            "sequence": c.sequence,
            "role": c.role,
        }
        for c in description.contributors
    ]
    alignment_targets = [
        {
            "key": key,
            "name": target.name,
            "namespace": target.namespace,
            "kind": target.kind,
            "doi": target.doi,
            "related_identifier": target.related_identifier,
        }
        for key, target in sorted(ALIGNMENT_TARGETS.items())
    ]
    payload = {
        "self_description": {
            "title": description.title,
            "version": description.version,
            "release_date": description.release_date,
            "concept_doi": description.concept_doi,
            "version_doi": description.version_doi,
            "version_iri": description.version_iri,
            "depositor_name": description.depositor_name,
            "depositor_email": description.depositor_email,
            "registrant": description.registrant,
            "registrant_wikidata": description.registrant_wikidata,
            "license_uri": description.license_uri,
            "homepage": description.homepage,
            "description": description.description,
            "repo_url": description.repo_url,
            "contributors": contributors,
        },
        "config": {
            "ontology_iri": ONTOLOGY_IRI,
            "dataset_slug": DATASET_SLUG,
            "deposit_format": DEPOSIT_FORMAT,
            "registrant_place": REGISTRANT_PLACE,
            "registrant_acronym": REGISTRANT_ACRONYM,
            "crossmark_enabled": CROSSMARK_ENABLED,
            "crossmark_policy_doi": CROSSMARK_POLICY_DOI,
            "alignment_targets": alignment_targets,
        },
    }
    return json.dumps(payload)


def _to_lint_input_json(description: SelfDescription) -> str:
    """Serialise the full lint context to JSON for the Rust backend.

    The returned JSON encodes a ``LintInput`` struct understood by
    ``gmeow_validate.lint_deposit_native``. The Rust linter renders its own XML
    from the supplied self-description and config, so no pre-rendered XML is
    passed.
    """
    base = json.loads(_to_deposit_input_json(description))
    # Read optional file contents — lint checks their text, never resolves over network.
    citation_cff: str | None = None
    if _CITATION_CFF.exists():
        citation_cff = _CITATION_CFF.read_text(encoding="utf-8")
    ontology_ttl: str | None = None
    if ONTOLOGY_FILE.exists():
        ontology_ttl = ONTOLOGY_FILE.read_text(encoding="utf-8")
    base["citation_cff"] = citation_cff
    base["ontology_ttl"] = ontology_ttl
    return json.dumps(base)


def build_deposit_xml(
    *,
    meta: SelfDescription | None = None,
    timestamp: str | None = None,
    batch_id: str | None = None,
) -> str:
    """Build the CrossRef deposit XML for GMEOW's DOI(s).

    Emits the concept record always, and the version record only when a version
    DOI has been minted (``meta.version_doi`` is set). When both are present they
    are cross-linked with ``hasVersion`` / ``isVersionOf``.

    Args:
        meta: Preloaded self-description metadata. Defaults to the checkout file.
        timestamp: CrossRef batch timestamp (``YYYYMMDDHHMMSS``); defaults to the
            current UTC time (CrossRef uses it to order competing submissions).
        batch_id: Unique submission id; defaults to ``gmeow-{version}-{timestamp}``.

    Returns:
        The deposit document as an XML string (with declaration).
    """
    description = meta or load_self_description()
    default_ts, default_batch = _live_stamp(description)
    timestamp = timestamp or default_ts
    batch_id = batch_id or default_batch
    return build_deposit_xml_native(
        _to_deposit_input_json(description), timestamp, batch_id
    )


def lint_deposit(meta: SelfDescription | None = None) -> list[str]:
    """Return DOI consistency problems, or ``[]`` if the deposit is sound.

    Run by ``gmeow-dev crossref`` before the deposit is written, so an
    inconsistent deposit can never be produced for submission. Format/consistency
    only — never resolves a DOI over the network (our own DOI is undeposited until
    submitted, so it 404s; a network probe would be a false failure). Checks:

    (a) no ``10.XXXXX`` placeholder survives in self-description, CITATION.cff,
        or the rendered deposit;
    (b) the deposit's DOIs equal the self-description's DOIs;
    (c) the concept resource is the concept IRI and the version resource is the
        ``owl:versionIRI`` (and term IRIs carry no version — structurally true,
        since the concept resource must not contain a version segment);
    (d) concept-only (no version DOI) is valid — not flagged;
    (e) the deposit carries a license and at least one contributor (maximal-schema
        invariants that a thin deposit would silently drop).
    """
    description = meta or load_self_description()
    return lint_deposit_native(_to_lint_input_json(description))


def write_deposit(
    path: Path | None = None,
    *,
    meta: SelfDescription | None = None,
    **kwargs: str,
) -> Path:
    """Write the CrossRef deposit XML to ``dist/`` for manual submission.

    The deposit is a transient submission document — generated on demand, hand-
    verified, and submitted to CrossRef by the registrant — NOT a committed,
    drift-gated artifact. It therefore lives under ``dist/`` (ephemeral build
    output), never under ``generated/``.

    Args:
        path: Output path (defaults to ``dist/crossref-deposit.xml``).
        meta: Preloaded self-description metadata.
        **kwargs: Forwarded to :func:`build_deposit_xml`.

    Returns:
        The path written.
    """
    out = path or (DIST_DIR / "crossref-deposit.xml")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(build_deposit_xml(meta=meta, **kwargs) + "\n", encoding="utf-8")
    return out
