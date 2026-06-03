"""Quality and FAIR scoring via the OOPS! and FOOPS! web services.

Both are network calls, so callers gate them (they skip cleanly offline). OOPS!
accepts inline ontology content (works pre-publication); FOOPS! assesses a
dereferenceable ontology URL (meaningful only once published).
"""

from __future__ import annotations

from dataclasses import dataclass

import httpx

_OOPS_ENDPOINT = "https://oops.linkeddata.es/rest"
_FOOPS_ENDPOINT = "https://w3id.org/foops/assessOntology"

_OOPS_REQUEST = """\
<?xml version="1.0" encoding="UTF-8"?>
<OOPSRequest>
  <OntologyURI></OntologyURI>
  <OntologyContent><![CDATA[{content}]]></OntologyContent>
  <Pitfalls></Pitfalls>
  <OutputFormat>RDF/XML</OutputFormat>
</OOPSRequest>
"""


@dataclass(slots=True)
class FoopsResult:
    """A FOOPS! FAIR assessment summary."""

    score: float
    checks_total: int
    checks_passed: int


def run_oops(ttl_content: str, *, timeout: float = 120.0) -> str:
    """Run the OOPS! pitfall scanner on inline ontology content.

    Args:
        ttl_content: The ontology serialized as RDF (Turtle/RDF-XML).
        timeout: HTTP timeout in seconds.

    Returns:
        The OOPS! evaluation as RDF/XML text.
    """
    response = httpx.post(
        _OOPS_ENDPOINT,
        content=_OOPS_REQUEST.format(content=ttl_content),
        headers={"Content-Type": "application/xml"},
        timeout=timeout,
    )
    response.raise_for_status()
    return response.text


def run_foops(ontology_url: str, *, timeout: float = 180.0) -> FoopsResult:
    """Run the FOOPS! FAIR assessment on a dereferenceable ontology URL.

    Args:
        ontology_url: The published ontology IRI/URL to assess.
        timeout: HTTP timeout in seconds.

    Returns:
        A :class:`FoopsResult` summarising the FAIR score.
    """
    response = httpx.post(
        _FOOPS_ENDPOINT,
        data={"ontologyUrl": ontology_url},
        timeout=timeout,
    )
    response.raise_for_status()
    payload = response.json()
    checks = payload.get("checks", [])
    passed = sum(1 for c in checks if c.get("status") == "ok" or c.get("score") == 1)
    return FoopsResult(
        score=float(payload.get("overall_score", 0.0)),
        checks_total=len(checks),
        checks_passed=passed,
    )
