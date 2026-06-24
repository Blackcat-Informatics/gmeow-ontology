# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Slice discovery and manifest loading (CONSTITUTION Principles 15 & 16).

A *slice* is the unit of the ontology: a self-contained directory under
``slices/<group>/<name>/`` whose anatomy (``module.ttl``, ``shapes.ttl``,
``mappings/``, ``queries/``, ``tests/``, ``docs.md``) is fixed by convention
and discovered, never configured. The ``manifest.ttl`` beside those files —
authored in ``slices/vocabulary.ttl`` terms — is the *sole* source of slice
identity and tier:

* The slice IRI lives in the manifest only. The ``<group>`` path segment is
  human organization with no semantics (``slices/anything/baz`` builds
  identically), and third-party slices declare IRIs under their own domain.
* ``gmeow:sliceTier`` is the only core/extension distinction.

This module performs discovery and *structural* loading. The semantic gates
(declared dependencies ≡ the computed cross-slice reference graph, the
extension→extension ban, the one-defining-slice rule, guide anchor
resolution) live in the slice gate, which has the whole graph in view.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import DCTERMS, RDFS

from gmeow_tools.config import NAMESPACE, SLICES_DIR


class SliceError(Exception):
    """A structurally invalid slice manifest or slice-set contradiction.

    Slice discovery is foundational (config-only imports) and sits *below* the
    compilers in the layering, so it raises its own error type.
    """


#: Manifest vocabulary terms (slices/vocabulary.ttl).
_SLICE = URIRef(NAMESPACE + "Slice")
_TIER = URIRef(NAMESPACE + "sliceTier")
_TIER_CORE = URIRef(NAMESPACE + "tierCore")
_TIER_EXTENSION = URIRef(NAMESPACE + "tierExtension")
_DEPENDS_ON = URIRef(NAMESPACE + "sliceDependsOn")
_CONSUMER = URIRef(NAMESPACE + "sliceConsumer")
_PROFILE = URIRef(NAMESPACE + "sliceProfile")
_SUBCOMMAND = URIRef(NAMESPACE + "providesSubcommand")
_BUILT_AGAINST = URIRef(NAMESPACE + "builtAgainstCore")

#: The conventional anatomy, discovered beside the manifest (never declared).
ANATOMY = (
    "module.ttl",
    "shapes.ttl",
    "mappings",
    "queries",
    "examples",
    "tests",
    "docs.md",
)


@dataclass(frozen=True)
class Slice:
    """A loaded slice manifest plus its checkout location.

    Identity is :attr:`iri`; :attr:`group` and :attr:`name` are checkout
    organization only and never feed the build's keying or gating.
    """

    iri: str
    name: str
    group: str
    path: Path
    tier: str  # "core" | "extension"
    depends_on: tuple[str, ...]
    consumers: tuple[str, ...]
    title: str
    creators: tuple[str, ...]
    profiles: tuple[str, ...] = ()
    subcommands: tuple[str, ...] = ()
    built_against_core: str | None = None

    @property
    def module_path(self) -> Path:
        """The slice's canonical terms file (convention: ``module.ttl``)."""
        return self.path / "module.ttl"

    @property
    def is_core(self) -> bool:
        """True when the manifest declares ``gmeow:tierCore``."""
        return self.tier == "core"


def _strings(graph: Graph, subject: URIRef, predicate: URIRef) -> tuple[str, ...]:
    """All literal values of ``predicate`` on ``subject``, as plain strings.

    This is a deliberate *lexical projection*: it accepts any RDF literal —
    PROSE properties carry ``@x-gmeow-english`` and TOKEN properties are plain
    ``xsd:string`` (the #820 S0 manifest datatype contract, enforced by
    ``shapes/slice-manifest-shapes.ttl``) — and returns its lexical form, so the
    structural loader stays agnostic to the literal's language/datatype identity.
    Preserving that identity end-to-end is the job of the native catalog (S1),
    not of this Python structural view, which intentionally keeps the same
    string-returning contract its callers already rely on.
    """
    values = (o for o in graph.objects(subject, predicate) if isinstance(o, Literal))
    return tuple(sorted(str(o) for o in values))


def _load_manifest(manifest: Path) -> Slice:
    """Parse one ``manifest.ttl`` into a :class:`Slice`.

    Raises:
        SliceError: On a structurally invalid manifest (no/multiple
            ``gmeow:Slice`` subjects, missing tier, unknown tier value).
    """
    graph = Graph()
    graph.parse(manifest, format="turtle")

    subjects = sorted(set(graph.subjects(RDF.type, _SLICE)), key=str)
    if len(subjects) != 1:
        raise SliceError(
            f"{manifest}: expected exactly one gmeow:Slice subject, "
            f"found {len(subjects)}"
        )
    node = subjects[0]
    if not isinstance(node, URIRef):
        raise SliceError(f"{manifest}: the gmeow:Slice subject must be an IRI")

    tiers = sorted(set(graph.objects(node, _TIER)), key=str)
    if len(tiers) != 1 or tiers[0] not in (_TIER_CORE, _TIER_EXTENSION):
        raise SliceError(
            f"{manifest}: gmeow:sliceTier must be exactly one of "
            "gmeow:tierCore / gmeow:tierExtension"
        )

    titles = _strings(graph, node, DCTERMS.title) or _strings(graph, node, RDFS.label)
    profiles = _strings(graph, node, _PROFILE)
    built = _strings(graph, node, _BUILT_AGAINST)

    return Slice(
        iri=str(node),
        name=manifest.parent.name,
        group=manifest.parent.parent.name,
        path=manifest.parent,
        tier="core" if tiers[0] == _TIER_CORE else "extension",
        depends_on=tuple(
            sorted(
                str(o)
                for o in graph.objects(node, _DEPENDS_ON)
                if isinstance(o, URIRef)
            )
        ),
        consumers=_strings(graph, node, _CONSUMER),
        title=titles[0] if titles else manifest.parent.name,
        creators=_strings(graph, node, DCTERMS.creator),
        profiles=profiles,
        subcommands=_strings(graph, node, _SUBCOMMAND),
        built_against_core=built[0] if built else None,
    )


def iter_slice_module_files(root: Path = SLICES_DIR) -> list[Path]:
    """Every slice's canonical terms file (``slices/*/*/module.ttl``), sorted."""
    return sorted(root.glob("*/*/module.ttl"))


def module_path(name: str, root: Path = SLICES_DIR) -> Path:
    """Resolve a slice's module file by slice (directory) name.

    The ``<group>`` segment carries no semantics, so resolution globs across
    groups; exactly one match is required.

    Raises:
        SliceError: When no slice — or more than one — has that name.
    """
    matches = sorted(root.glob(f"*/{name}/module.ttl"))
    if len(matches) != 1:
        raise SliceError(
            f"slice {name!r}: expected exactly one slices/*/{name}/module.ttl, "
            f"found {len(matches)}"
        )
    return matches[0]


def iter_slice_shape_files(root: Path = SLICES_DIR) -> list[Path]:
    """Every slice's SHACL shapes file (``slices/*/*/shapes.ttl``), sorted."""
    return sorted(root.glob("*/*/shapes.ttl"))


def iter_slice_mapping_files(root: Path = SLICES_DIR) -> list[Path]:
    """Every slice's mapping-DSL cell file (``slices/*/*/mappings/*.ttl``), sorted."""
    return sorted(root.glob("*/*/mappings/*.ttl"))


def iter_slice_example_files(root: Path = SLICES_DIR) -> list[Path]:
    """Every slice's worked-example Turtle files (#332).

    Examples are canonical worked instance data — consumed by slice tests,
    coverage, the #325 guides, the eval corpus, and the slice's GTS package
    sections — and they validate in ``make validate`` so they never rot.
    """
    return sorted(root.glob("*/*/examples/*.ttl"))


def iter_slice_query_files(kind: str, root: Path = SLICES_DIR) -> list[Path]:
    """Every slice's SPARQL files of one kind (``competency`` / ``verify``)."""
    return sorted(root.glob(f"*/*/queries/{kind}/*.rq"))


def iter_slice_test_files(root: Path = SLICES_DIR) -> list[Path]:
    """Every slice's declarative test-DSL fixture (``slices/*/*/tests/*.ttl``), sorted.

    Non-recursive past ``tests/``: only the fixture Turtle files directly in a
    slice's ``tests/`` directory are returned, so any ``tests/*.py`` harness code
    and any ``tests/counter-examples/*.ttl`` data are excluded.
    """
    return sorted(root.glob("*/*/tests/*.ttl"))


def discover_slices(root: Path = SLICES_DIR) -> dict[str, Slice]:
    """Discover every slice under ``root`` and load its manifest.

    Globs ``slices/*/*/manifest.ttl`` — the middle segment is organizational
    and unconstrained. The returned mapping is keyed by slice IRI (the only
    identity); insertion order is sorted by IRI for determinism.

    Raises:
        SliceError: When two manifests declare the same slice IRI, or any
            manifest is structurally invalid.
    """
    found: dict[str, Slice] = {}
    if not root.is_dir():
        return found
    for manifest in sorted(root.glob("*/*/manifest.ttl")):
        loaded = _load_manifest(manifest)
        if loaded.iri in found:
            raise SliceError(
                f"duplicate slice IRI {loaded.iri}: declared by both "
                f"{found[loaded.iri].path} and {loaded.path}"
            )
        found[loaded.iri] = loaded
    return dict(sorted(found.items()))


def extension_dependency_violations(slices: dict[str, Slice]) -> list[str]:
    """Return DAG-rule violations: extension→extension or dangling deps.

    An extension slice may depend only on core slices (Principle 16). A
    dependency on an IRI not present in ``slices`` is reported too — except
    for third-party scenarios the caller handles by loading the full set.
    """
    problems: list[str] = []
    for s in slices.values():
        for dep in s.depends_on:
            target = slices.get(dep)
            if target is None:
                problems.append(f"{s.iri}: depends on unknown slice {dep}")
            elif s.tier == "extension" and target.tier == "extension":
                problems.append(
                    f"{s.iri}: extension→extension dependency on {dep} "
                    "(extensions may depend only on core slices)"
                )
    return problems
