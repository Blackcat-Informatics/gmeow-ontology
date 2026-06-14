# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Canonical citation ledger and generated bibliography exports.

The authored source is ``metadata/references.ttl``.  Everything under
``generated/references/`` is a lossy projection for tools that expect
flat bibliography formats (CSL JSON, BibTeX, Markdown).
"""

from __future__ import annotations

import json
import re
import subprocess
from collections import Counter
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path
from typing import cast
from urllib.parse import quote, urlparse

from rdflib import RDF, RDFS, Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS
from rdflib.term import Node

from gmeow_tools.config import (
    DIST_DIR,
    NAMESPACE,
    PROJECT_ROOT,
    REFERENCES_BIB_FILE,
    REFERENCES_CSL_FILE,
    REFERENCES_FILE,
    REFERENCES_MD_FILE,
)
from gmeow_tools.generator import Generator, register, write_text

GMEOW = Namespace(NAMESPACE)
REF = Namespace(NAMESPACE + "references/")
REPO = Namespace(NAMESPACE + "repo/")

DEFAULT_REPO = "Blackcat-Informatics/gmeow-ontology"
DEFAULT_CANDIDATES_FILE = DIST_DIR / "reference-candidates.jsonl"

URL_RE = re.compile(r"https?://[^\s<>)\"\\\]]+")
DOI_RE = re.compile(r"(?<![A-Za-z0-9])10\.\d{4,9}/[^\s<>)\"\\\]]+")
BIBLIOGRAPHIC_RE = re.compile(
    r"(?:gmeow:|dcterms:)?bibliographicCitation\s+\"([^\"]+)\"",
    re.IGNORECASE,
)
TRAILING_PUNCTUATION = ".,;:!?)]}'\"`"
INVALID_URL_CHARS = frozenset('`{}|\\^[]<>"…*$')

TEXT_SUFFIXES = {
    ".cff",
    ".csv",
    ".go",
    ".json",
    ".md",
    ".py",
    ".rq",
    ".rs",
    ".toml",
    ".ttl",
    ".ts",
    ".txt",
    ".yaml",
    ".yml",
}
TEXT_FILENAMES = {
    "AGENTS.md",
    "CLAUDE.md",
    "CONSTITUTION.md",
    "CONTRIBUTING.md",
    "CITATION.cff",
    "Makefile",
    "NOTICE",
    "README.md",
}
SKIP_PARTS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".stamps",
    ".uv-cache",
    ".venv",
    ".worktrees",
    "dist",
    "docs/_generated",
    "generated",
    "imports",
    "ontology-docs",
    "target",
}
SKIP_FILES = {"Cargo.lock", "package-lock.json", "uv.lock"}
SKIP_URL_HOSTS = {
    "app.coderabbit.ai",
    "discord.gg",
    "ex.org",
    "example",
    "example.org",
    "github.com",
    "redirect.github.com",
    "storage.googleapis.com",
    "www.gstatic.com",
}
SKIP_URL_SUFFIXES = (".internal",)
SKIP_URL_SUBSTRINGS = ("googleusercontent.com",)

KNOWN_LABELS = {
    "http://purl.org/dc/terms/": "DCMI Metadata Terms",
    "http://purl.org/ontology/bibo/": "Bibliographic Ontology",
    "http://purl.org/nemo/gufo#": "gUFO",
    "http://usefulinc.com/ns/doap#": "DOAP",
    "http://www.opengis.net/ont/geosparql#": "GeoSPARQL",
    "http://www.w3.org/2002/12/cal/icaltzd#": "iCalendar RDF vocabulary",
    "http://www.w3.org/2004/02/skos/core#": "SKOS",
    "http://www.w3.org/2006/time#": "OWL-Time",
    "http://www.w3.org/2006/vcard/ns#": "vCard RDF vocabulary",
    "http://www.w3.org/ns/prov#": "PROV-O",
    "http://xmlns.com/foaf/0.1/": "FOAF",
    "https://schema.org/": "Schema.org",
}


@dataclass(frozen=True, slots=True)
class CitationCandidate:
    """One extracted citation-like edge before or after RDF normalization."""

    citing_iri: str
    citing_label: str
    citing_location: str
    cited_iri: str
    cited_label: str
    cited_kind: str
    selector_locator: str
    selector_quote: str = ""
    citation_intent: str = "intentBridgedByReference"

    @property
    def key(self) -> tuple[str]:
        """Stable deduplication key for the candidate."""
        return (self.cited_iri,)

    @property
    def sort_key(self) -> tuple[str, str, str]:
        """Stable ordering key before cited-work deduplication."""
        return (self.cited_iri, self.citing_iri, self.selector_locator)


@dataclass(frozen=True, slots=True)
class BackfillReport:
    """Summary of a citation backfill run."""

    local_candidates: int
    github_candidates: int
    unique_candidates: int
    references_file: Path
    candidates_file: Path


def _hash(value: str, length: int = 16) -> str:
    """Return a short deterministic hash for minted local identifiers."""
    return sha256(value.encode("utf-8")).hexdigest()[:length]


def _slug(value: str, *, max_length: int = 72) -> str:
    """Return a compact ASCII slug."""
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return (slug or "reference")[:max_length].strip("-") or "reference"


def _trim_reference(value: str) -> str:
    """Strip punctuation that commonly clings to Markdown/prose URLs."""
    return value.strip().rstrip(TRAILING_PUNCTUATION)


def _doi_from_text(value: str) -> str:
    """Return a normalized DOI string from a DOI token or DOI URL."""
    text = _trim_reference(value)
    parsed = urlparse(text)
    if parsed.netloc.lower() in {"doi.org", "dx.doi.org"}:
        text = parsed.path.lstrip("/")
    return text.rstrip(TRAILING_PUNCTUATION)


def _doi_iri(doi: str) -> str:
    """Return the canonical DOI resolver IRI for a DOI string."""
    return "https://doi.org/" + doi


def _label_for_url(url: str) -> str:
    """Return a human-readable fallback label for a URL reference."""
    if url in KNOWN_LABELS:
        return KNOWN_LABELS[url]
    parsed = urlparse(url)
    if parsed.netloc.lower() in {"doi.org", "dx.doi.org"}:
        return f"DOI {parsed.path.lstrip('/')}"
    path = parsed.path.rstrip("/")
    if not path:
        return parsed.netloc
    return f"{parsed.netloc}{path}"


def _should_skip_url(url: str) -> bool:
    """Return true for URLs that are repository plumbing, not citations."""
    if "..." in url or any(char in url for char in INVALID_URL_CHARS):
        return True
    parsed = urlparse(url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return True
    host = parsed.netloc.lower()
    if (
        host in SKIP_URL_HOSTS
        or any(host.endswith(suffix) for suffix in SKIP_URL_SUFFIXES)
        or any(part in host for part in SKIP_URL_SUBSTRINGS)
    ):
        return True
    if host == "blackcatinformatics.ca" and parsed.path.startswith("/gmeow"):
        return True
    return host == "github.com" and parsed.path.startswith(
        "/Blackcat-Informatics/gmeow-ontology/"
    )


def _local_file_iri(path: Path) -> str:
    """Return the stable repo-local IRI for an authored file."""
    rel = path.relative_to(PROJECT_ROOT).as_posix()
    return str(REPO) + quote(rel, safe="/")


def _github_issue_number(url: str) -> str:
    """Extract an issue or PR number from a GitHub API/html URL if present."""
    match = re.search(r"/(?:issues|pulls)/(\d+)(?:$|[/#?])", url)
    if match:
        return match.group(1)
    match = re.search(r"/issues/(\d+)$", url)
    return match.group(1) if match else "unknown"


def _candidate_from_url(
    *,
    url: str,
    citing_iri: str,
    citing_label: str,
    citing_location: str,
    selector_locator: str,
) -> CitationCandidate | None:
    """Create a citation candidate from a URL token."""
    clean = _trim_reference(url)
    if _should_skip_url(clean):
        return None
    parsed = urlparse(clean)
    if parsed.netloc.lower() in {"doi.org", "dx.doi.org"}:
        doi = _doi_from_text(clean)
        cited_iri = _doi_iri(doi)
        label = f"DOI {doi}"
        kind = "doi"
    else:
        cited_iri = clean
        label = _label_for_url(clean)
        kind = "url"
    return CitationCandidate(
        citing_iri=citing_iri,
        citing_label=citing_label,
        citing_location=citing_location,
        cited_iri=cited_iri,
        cited_label=label,
        cited_kind=kind,
        selector_locator=selector_locator,
    )


def _candidate_from_doi(
    *,
    doi: str,
    citing_iri: str,
    citing_label: str,
    citing_location: str,
    selector_locator: str,
) -> CitationCandidate:
    """Create a citation candidate from a DOI token."""
    clean = _doi_from_text(doi)
    return CitationCandidate(
        citing_iri=citing_iri,
        citing_label=citing_label,
        citing_location=citing_location,
        cited_iri=_doi_iri(clean),
        cited_label=f"DOI {clean}",
        cited_kind="doi",
        selector_locator=selector_locator,
    )


def _candidate_from_bibliographic_text(
    *,
    text: str,
    citing_iri: str,
    citing_label: str,
    citing_location: str,
    selector_locator: str,
) -> CitationCandidate:
    """Create a citation candidate from a bibliographic citation string."""
    clean = text.strip()
    local = str(REF) + "work/" + _slug(clean) + "-" + _hash(clean, 8)
    return CitationCandidate(
        citing_iri=citing_iri,
        citing_label=citing_label,
        citing_location=citing_location,
        cited_iri=local,
        cited_label=clean,
        cited_kind="bibliographic",
        selector_locator=selector_locator,
        selector_quote=clean,
        citation_intent="intentCitesAsDataSource",
    )


def extract_candidates_from_text(
    text: str,
    *,
    citing_iri: str,
    citing_label: str,
    citing_location: str,
) -> list[CitationCandidate]:
    """Extract citation candidates from one textual carrier."""
    candidates: list[CitationCandidate] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        locator = f"{citing_location}:{line_no}"
        for match in URL_RE.finditer(line):
            candidate = _candidate_from_url(
                url=match.group(0),
                citing_iri=citing_iri,
                citing_label=citing_label,
                citing_location=citing_location,
                selector_locator=locator,
            )
            if candidate is not None:
                candidates.append(candidate)
        for match in DOI_RE.finditer(line):
            candidates.append(
                _candidate_from_doi(
                    doi=match.group(0),
                    citing_iri=citing_iri,
                    citing_label=citing_label,
                    citing_location=citing_location,
                    selector_locator=locator,
                )
            )
        for match in BIBLIOGRAPHIC_RE.finditer(line):
            candidates.append(
                _candidate_from_bibliographic_text(
                    text=match.group(1),
                    citing_iri=citing_iri,
                    citing_label=citing_label,
                    citing_location=citing_location,
                    selector_locator=locator,
                )
            )
    return candidates


def dedupe_candidates(
    candidates: Iterable[CitationCandidate],
) -> list[CitationCandidate]:
    """Return candidates in stable order with duplicate citation acts removed."""
    seen: set[tuple[str]] = set()
    unique: list[CitationCandidate] = []
    for candidate in sorted(candidates, key=lambda c: c.sort_key):
        if candidate.key in seen:
            continue
        seen.add(candidate.key)
        unique.append(candidate)
    return unique


def _is_text_path(path: Path) -> bool:
    """Return true when a repo path should be scanned for citation text."""
    rel = path.relative_to(PROJECT_ROOT)
    if path.name in SKIP_FILES:
        return False
    if rel.parts == ("metadata", "references.ttl"):
        return False
    if any(part in SKIP_PARTS for part in rel.parts):
        return False
    if rel.parts[:4] == ("tests", "fixtures", "coverage", "external"):
        return False
    return path.name in TEXT_FILENAMES or path.suffix in TEXT_SUFFIXES


def local_candidates(root: Path = PROJECT_ROOT) -> list[CitationCandidate]:
    """Harvest citation candidates from authored local repository files."""
    candidates: list[CitationCandidate] = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or not _is_text_path(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        rel = path.relative_to(root).as_posix()
        candidates.extend(
            extract_candidates_from_text(
                text,
                citing_iri=_local_file_iri(path),
                citing_label=rel,
                citing_location=rel,
            )
        )
    return dedupe_candidates(candidates)


def _run_gh(args: Sequence[str]) -> str:
    """Run ``gh`` and return stdout, raising RuntimeError on failure."""
    result = subprocess.run(
        ["gh", *args],
        check=False,
        capture_output=True,
        text=True,
        cwd=PROJECT_ROOT,
        timeout=120,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"gh {' '.join(args)} failed: {detail}")
    return result.stdout


def _gh_paginated(repo: str, endpoint: str) -> list[Mapping[str, object]]:
    """Fetch and flatten a paginated GitHub REST endpoint via ``gh api``."""
    payload = _run_gh(["api", "--paginate", "--slurp", f"repos/{repo}/{endpoint}"])
    data = json.loads(payload)
    pages = cast(list[object], data)
    rows: list[Mapping[str, object]] = []
    for page in pages:
        if isinstance(page, list):
            rows.extend(cast(list[Mapping[str, object]], page))
        elif isinstance(page, dict):
            rows.append(cast(Mapping[str, object], page))
    return rows


_PR_REVIEWS_QUERY = """
query($owner: String!, $name: String!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(
      first: 100
      after: $cursor
      states: [OPEN, CLOSED, MERGED]
      orderBy: {field: CREATED_AT, direction: ASC}
    ) {
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        number
        title
        url
        reviews(first: 100) {
          nodes {
            body
            url
            author {
              login
            }
          }
        }
      }
    }
  }
}
"""


def _gh_pr_review_summaries(repo: str) -> list[Mapping[str, object]]:
    """Fetch PR review summaries in repository-wide GraphQL pages."""
    owner, name = repo.split("/", 1)
    rows: list[Mapping[str, object]] = []
    cursor: str | None = None
    while True:
        args = [
            "api",
            "graphql",
            "-f",
            f"query={_PR_REVIEWS_QUERY}",
            "-F",
            f"owner={owner}",
            "-F",
            f"name={name}",
        ]
        if cursor is not None:
            args.extend(["-F", f"cursor={cursor}"])
        payload = json.loads(_run_gh(args))
        root = cast(Mapping[str, object], payload)
        data = cast(Mapping[str, object], root.get("data", {}))
        repository = cast(Mapping[str, object], data.get("repository", {}))
        pulls = cast(Mapping[str, object], repository.get("pullRequests", {}))
        nodes = pulls.get("nodes", [])
        if isinstance(nodes, list):
            for pr in cast(list[Mapping[str, object]], nodes):
                number = pr.get("number")
                title = pr.get("title")
                reviews = cast(Mapping[str, object], pr.get("reviews", {}))
                review_nodes = reviews.get("nodes", [])
                if not isinstance(review_nodes, list):
                    continue
                for review in cast(list[Mapping[str, object]], review_nodes):
                    body = review.get("body")
                    html_url = review.get("url")
                    author = review.get("author")
                    if not isinstance(body, str) or not isinstance(html_url, str):
                        continue
                    login = "unknown"
                    if isinstance(author, dict) and isinstance(
                        author.get("login"), str
                    ):
                        login = cast(str, author["login"])
                    rows.append(
                        {
                            "body": body,
                            "html_url": html_url,
                            "title": f"{title or 'review'}",
                            "user": {"login": login},
                            "number": number,
                        }
                    )
        page_info = cast(Mapping[str, object], pulls.get("pageInfo", {}))
        if page_info.get("hasNextPage") is not True:
            break
        next_cursor = page_info.get("endCursor")
        if not isinstance(next_cursor, str) or not next_cursor:
            break
        cursor = next_cursor
    return rows


def _text_field(row: Mapping[str, object], field: str) -> str:
    """Return a string field from a GitHub JSON row."""
    value = row.get(field)
    return value if isinstance(value, str) else ""


def _user_login(row: Mapping[str, object]) -> str:
    """Return the GitHub login nested under ``user`` if present."""
    user = row.get("user")
    if not isinstance(user, dict):
        return "unknown"
    login = user.get("login")
    return login if isinstance(login, str) else "unknown"


def _github_candidates_from_rows(
    rows: Iterable[Mapping[str, object]],
    *,
    label_prefix: str,
    body_field: str = "body",
) -> list[CitationCandidate]:
    """Extract candidates from generic GitHub rows with body/html_url fields."""
    candidates: list[CitationCandidate] = []
    for row in rows:
        body = _text_field(row, body_field)
        html_url = _text_field(row, "html_url")
        if not body or not html_url:
            continue
        number_value = row.get("number")
        number = str(number_value) if isinstance(number_value, int) else ""
        if not number:
            number = _github_issue_number(html_url)
        title = _text_field(row, "title")
        if title:
            label = f"{label_prefix} #{number}: {title}"
        else:
            label = f"{label_prefix} #{number} by {_user_login(row)}"
        candidates.extend(
            extract_candidates_from_text(
                body,
                citing_iri=html_url,
                citing_label=label,
                citing_location=html_url,
            )
        )
    return candidates


def github_candidates(repo: str = DEFAULT_REPO) -> list[CitationCandidate]:
    """Harvest citation candidates from accessible GitHub issue and PR history."""
    candidates: list[CitationCandidate] = []
    issues = _gh_paginated(repo, "issues?state=all&per_page=100")
    candidates.extend(
        _github_candidates_from_rows(issues, label_prefix="GitHub issue or PR")
    )
    issue_comments = _gh_paginated(repo, "issues/comments?per_page=100")
    candidates.extend(
        _github_candidates_from_rows(issue_comments, label_prefix="GitHub comment")
    )
    review_comments = _gh_paginated(repo, "pulls/comments?per_page=100")
    candidates.extend(
        _github_candidates_from_rows(
            review_comments, label_prefix="GitHub PR review comment"
        )
    )

    review_summaries = _gh_pr_review_summaries(repo)
    candidates.extend(
        _github_candidates_from_rows(
            review_summaries, label_prefix="GitHub PR review summary"
        )
    )
    return dedupe_candidates(candidates)


def _bind(graph: Graph) -> None:
    """Bind prefixes used by the citation ledger."""
    graph.bind("gmeow", GMEOW)
    graph.bind("ref", REF)
    graph.bind("repo", REPO)
    graph.bind("dcterms", DCTERMS)
    graph.bind("rdfs", RDFS)


def candidates_to_graph(candidates: Iterable[CitationCandidate]) -> Graph:
    """Convert citation candidates into the canonical GMEOW RDF shape."""
    graph = Graph()
    _bind(graph)
    for candidate in dedupe_candidates(candidates):
        citing = URIRef(candidate.citing_iri)
        cited = URIRef(candidate.cited_iri)
        graph.add((citing, RDF.type, GMEOW.CreativeWork))
        graph.add((citing, RDFS.label, Literal(candidate.citing_label, lang="en")))
        graph.add((citing, GMEOW.sourceLocation, Literal(candidate.citing_location)))

        graph.add((cited, RDF.type, GMEOW.CreativeWork))
        graph.add((cited, RDFS.label, Literal(candidate.cited_label, lang="en")))
        if candidate.cited_kind == "doi":
            graph.add((cited, GMEOW.identifier, Literal(candidate.cited_iri)))
        elif candidate.cited_kind == "bibliographic":
            graph.add(
                (cited, GMEOW.bibliographicCitation, Literal(candidate.cited_label))
            )
        else:
            graph.add((cited, GMEOW.sourceLocation, Literal(candidate.cited_iri)))

        citation = URIRef(str(REF) + "citation/" + _hash("|".join(candidate.key)))
        selector = URIRef(str(REF) + "selector/" + _hash("|".join(candidate.key)))
        intent = URIRef(str(GMEOW) + candidate.citation_intent)
        graph.add((citation, RDF.type, GMEOW.CitationAct))
        graph.add((citation, RDFS.label, Literal(candidate.citing_label, lang="en")))
        graph.add((citation, GMEOW.citingEntity, citing))
        graph.add((citation, GMEOW.citedEntity, cited))
        graph.add((citation, GMEOW.citationIntent, intent))
        graph.add((citation, GMEOW.viaSelector, selector))

        graph.add((selector, RDF.type, GMEOW.Selector))
        graph.add(
            (selector, GMEOW.selectorLocator, Literal(candidate.selector_locator))
        )
        if candidate.selector_quote:
            graph.add(
                (selector, GMEOW.selectorTextQuote, Literal(candidate.selector_quote))
            )
    return graph


def write_references_ttl(
    candidates: Iterable[CitationCandidate],
    path: Path = REFERENCES_FILE,
) -> Path:
    """Write the canonical citation ledger Turtle file."""
    graph = candidates_to_graph(candidates)
    path.parent.mkdir(parents=True, exist_ok=True)
    turtle = graph.serialize(format="turtle").rstrip() + "\n"
    path.write_text(
        "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. "
        "<paudley@blackcatinformatics.ca>\n"
        "# SPDX-License-Identifier: CC-BY-4.0\n"
        "#\n"
        "# Canonical GMEOW citation ledger. Edit this file, then run "
        "`make regenerate`.\n"
        "# Generated bibliography projections live under generated/references/.\n\n"
        + turtle,
        encoding="utf-8",
    )
    return path


def load_reference_graph(path: Path = REFERENCES_FILE) -> Graph:
    """Load the canonical citation ledger graph."""
    graph = Graph()
    graph.parse(path, format="turtle")
    _bind(graph)
    return graph


def _literal(graph: Graph, subject: Node, predicate: URIRef) -> str:
    """Return the first literal value for a predicate."""
    for obj in graph.objects(subject, predicate):
        if isinstance(obj, Literal):
            return str(obj)
    return ""


def _reference_id(iri: str) -> str:
    """Return a stable identifier for bibliography exports."""
    parsed = urlparse(iri)
    if parsed.netloc.lower() == "doi.org":
        return "doi-" + _slug(parsed.path.lstrip("/"))
    tail = iri.rstrip("/").rsplit("/", 1)[-1].rsplit("#", 1)[-1]
    return _slug(tail or parsed.netloc) + "-" + _hash(iri, 8)


def _date_parts(value: str) -> list[list[int]] | None:
    """Convert an ISO-ish date string to CSL ``date-parts``."""
    match = re.match(r"^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?", value)
    if not match:
        return None
    parts = [int(part) for part in match.groups() if part is not None]
    return [parts]


def _cited_subjects(graph: Graph) -> list[URIRef]:
    """Return cited CreativeWork subjects in deterministic order."""
    subjects = {
        obj for obj in graph.objects(None, GMEOW.citedEntity) if isinstance(obj, URIRef)
    }
    return sorted(subjects, key=str)


def build_csl_json(graph: Graph) -> str:
    """Build CSL JSON from the canonical ledger."""
    items: list[dict[str, object]] = []
    for subject in _cited_subjects(graph):
        iri = str(subject)
        item: dict[str, object] = {
            "id": _reference_id(iri),
            "type": "webpage",
            "title": _literal(graph, subject, RDFS.label) or iri,
        }
        source_location = _literal(graph, subject, GMEOW.sourceLocation)
        if source_location:
            item["URL"] = source_location
        elif iri.startswith(("http://", "https://")):
            item["URL"] = iri
        if iri.startswith("https://doi.org/"):
            item["DOI"] = iri.removeprefix("https://doi.org/")
        date = _literal(graph, subject, GMEOW.datePublished) or _literal(
            graph, subject, DCTERMS.issued
        )
        date_value = _date_parts(date)
        if date_value is not None:
            item["issued"] = {"date-parts": date_value}
        note = _literal(graph, subject, GMEOW.bibliographicCitation)
        if note:
            item["note"] = note
        items.append(item)
    return json.dumps(items, indent=2, sort_keys=True) + "\n"


def _bibtex_escape(value: str) -> str:
    """Escape a string for conservative BibTeX output."""
    return (
        value.replace("\\", "\\textbackslash{}").replace("{", "\\{").replace("}", "\\}")
    )


def build_bibtex(graph: Graph) -> str:
    """Build BibTeX from the canonical ledger."""
    entries: list[str] = []
    for subject in _cited_subjects(graph):
        iri = str(subject)
        key = _reference_id(iri)
        title = _literal(graph, subject, RDFS.label) or iri
        fields = [f"  title = {{{_bibtex_escape(title)}}}"]
        source_location = _literal(graph, subject, GMEOW.sourceLocation)
        url = source_location or (
            iri if iri.startswith(("http://", "https://")) else ""
        )
        if url:
            fields.append(f"  url = {{{_bibtex_escape(url)}}}")
        if iri.startswith("https://doi.org/"):
            doi = iri.removeprefix("https://doi.org/")
            fields.append(f"  doi = {{{_bibtex_escape(doi)}}}")
        fields.append("  note = {Generated from the GMEOW citation ledger}")
        entries.append(f"@misc{{{key},\n" + ",\n".join(fields) + "\n}")
    return "\n\n".join(entries) + ("\n" if entries else "")


def build_markdown(graph: Graph) -> str:
    """Build a human-readable Markdown bibliography from the canonical ledger."""
    counts = Counter(str(obj) for obj in graph.objects(None, GMEOW.citedEntity))
    lines = [
        "# GMEOW Citation Ledger",
        "",
        "This bibliography is generated from `metadata/references.ttl`.",
        "",
        "| Reference | Locator | Citation acts |",
        "|---|---:|---:|",
    ]
    for subject in _cited_subjects(graph):
        iri = str(subject)
        title = _literal(graph, subject, RDFS.label) or iri
        source_location = _literal(graph, subject, GMEOW.sourceLocation) or iri
        if source_location.startswith(("http://", "https://")):
            locator = f"[link]({source_location})"
        else:
            locator = f"`{source_location}`"
        lines.append(f"| {title} | {locator} | {counts[iri]} |")
    return "\n".join(lines) + "\n"


def write_reference_exports(
    graph: Graph,
    *,
    csl_path: Path = REFERENCES_CSL_FILE,
    bib_path: Path = REFERENCES_BIB_FILE,
    md_path: Path = REFERENCES_MD_FILE,
    name: str = "",
    source_hash: str = "",
) -> None:
    """Write all generated reference exports."""
    csl_path.parent.mkdir(parents=True, exist_ok=True)
    csl_path.write_text(build_csl_json(graph), encoding="utf-8")
    write_text(
        bib_path,
        build_bibtex(graph),
        name=name,
        source_hash=source_hash,
    )
    md_path.write_text(
        "<!-- GENERATED by gmeow references - DO NOT EDIT. -->\n"
        f"<!-- Source hash: {source_hash} -->\n"
        "<!-- https://github.com/Blackcat-Informatics/gmeow-ontology -->\n\n"
        + build_markdown(graph),
        encoding="utf-8",
    )


def _candidate_json(candidate: CitationCandidate) -> str:
    """Serialize one candidate as JSONL."""
    return json.dumps(
        {
            "citing_iri": candidate.citing_iri,
            "citing_label": candidate.citing_label,
            "citing_location": candidate.citing_location,
            "cited_iri": candidate.cited_iri,
            "cited_label": candidate.cited_label,
            "cited_kind": candidate.cited_kind,
            "selector_locator": candidate.selector_locator,
            "selector_quote": candidate.selector_quote,
            "citation_intent": candidate.citation_intent,
        },
        sort_keys=True,
    )


def backfill_references(
    *,
    include_github: bool = True,
    repo: str = DEFAULT_REPO,
    candidates_file: Path = DEFAULT_CANDIDATES_FILE,
    references_file: Path = REFERENCES_FILE,
) -> BackfillReport:
    """Harvest citation candidates and update the canonical ledger."""
    local = local_candidates()
    github = github_candidates(repo) if include_github else []
    unique = dedupe_candidates([*local, *github])
    candidates_file.parent.mkdir(parents=True, exist_ok=True)
    candidates_file.write_text(
        "".join(_candidate_json(candidate) + "\n" for candidate in unique),
        encoding="utf-8",
    )
    write_references_ttl(unique, references_file)
    return BackfillReport(
        local_candidates=len(local),
        github_candidates=len(github),
        unique_candidates=len(unique),
        references_file=references_file,
        candidates_file=candidates_file,
    )


@register
class ReferencesGenerator(Generator):
    """Generate bibliography exports from the canonical citation ledger."""

    name = "references"

    @property
    def inputs(self) -> Sequence[Path]:
        """Canonical citation ledger source."""
        return [REFERENCES_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """Generated citation exports."""
        return [REFERENCES_CSL_FILE, REFERENCES_BIB_FILE, REFERENCES_MD_FILE]

    def render(self, staging: Path) -> None:
        """Render generated citation exports."""
        graph = load_reference_graph()
        source_hash = getattr(self, "_source_hash", "")
        out_dir = staging / REFERENCES_CSL_FILE.parent.relative_to(PROJECT_ROOT)
        write_reference_exports(
            graph,
            csl_path=out_dir / REFERENCES_CSL_FILE.name,
            bib_path=out_dir / REFERENCES_BIB_FILE.name,
            md_path=out_dir / REFERENCES_MD_FILE.name,
            name=self.name,
            source_hash=source_hash,
        )
