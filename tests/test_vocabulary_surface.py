"""Vocabulary-surface integrity gates (issue #199).

Principle 4 (One canonical source) + Principle 7 (Verified by construction):
- Every authored ontology module must be in the root owl:imports closure.
- Coverage fixtures must use only declared GMEOW terms.
- Docs examples must not mint non-vocabulary gmeow: IRIs in user-copyable blocks.
"""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest
from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Literal, Namespace, URIRef

from gmeow_tools.config import (
    CATALOG_FILE,
    FIXTURES_DIR,
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_FILE,
    PROJECT_ROOT,
    SHAPES_DIR,
    SHAPES_FILE,
    SLICE_VOCABULARY_FILE,
    STATEMENT_DSL_DIR,
    TEST_DSL_VOCABULARY_FILE,
)
from gmeow_tools.graph import iter_module_files
from gmeow_tools.i18n_catalog import LOCALIZABLE_PREDICATES
from gmeow_tools.slices import (
    iter_slice_example_files,
    iter_slice_mapping_files,
    iter_slice_module_files,
    iter_slice_shape_files,
)

GMEOW = Namespace(NAMESPACE)
GMEOW_MODULES = NAMESPACE + "modules/"
GMEOW_EXAMPLES = NAMESPACE + "examples/"

# Directories that contain authored docs (not _generated).
DOCS_DIR = PROJECT_ROOT / "docs"

# DSL source directories whose terms are legitimate in docs prose.


# Terms that are intentionally retired but still mentioned in docs as historical.
_RETIRED_DOCS_TERMS = {
    NAMESPACE + name
    for name in (
        "alternateName",
        "gender",
        "sex",
    )
}

# Regex to find gmeow:LocalName in Turtle fenced blocks and inline code.
_TURTLE_BLOCK_RE = re.compile(r"```turtle\n(.*?)\n```", re.DOTALL)
_INLINE_TERM_RE = re.compile(r"`gmeow:([A-Za-z][A-Za-z0-9_]*)`")


def _parse_ttl(path: Path) -> Graph:
    return Graph().parse(path, format="turtle")


def _subjects(graph: Graph) -> set[str]:
    return {str(s) for s in graph.subjects() if isinstance(s, URIRef)}


def _gmeow_vocabulary_terms(graph: Graph) -> set[str]:
    """Return URIRefs under the GMEOW namespace that look like vocabulary terms.

    Filters out example-instance IRIs (``/examples/…``) and module ontology IRIs
    (``/modules/…``).
    """
    terms: set[str] = set()
    for triple in graph:
        for node in triple:
            if isinstance(node, URIRef) and str(node).startswith(NAMESPACE):
                s = str(node)
                if s.startswith(GMEOW_EXAMPLES) or s.startswith(GMEOW_MODULES):
                    continue
                terms.add(s)
    return terms


# --------------------------------------------------------------------------- #
# Check A — module import/catalog closure
# --------------------------------------------------------------------------- #


def test_root_imports_are_exactly_the_core_profile() -> None:
    """The root IRI IS the core profile (#330): its owl:imports must equal
    the tierCore slice set exactly — an extension in the root, or a core
    slice missing from it, is a gated failure, never silent drift."""
    from gmeow_tools.slices import discover_slices

    root = _parse_ttl(ONTOLOGY_FILE)
    imports = {
        str(o) for o in root.objects(predicate=OWL.imports) if isinstance(o, URIRef)
    }
    core = {s.iri for s in discover_slices().values() if s.is_core}
    assert imports == core, (
        f"root/core drift — extra: {sorted(imports - core)}; "
        f"missing: {sorted(core - imports)}"
    )


def test_full_profile_imports_every_slice() -> None:
    """<…/gmeow/full> aggregates the root (core) plus every extension —
    no slice can exist outside its profiles (#330)."""
    from gmeow_tools.config import FULL_PROFILE_FILE, ONTOLOGY_IRI
    from gmeow_tools.slices import discover_slices

    full = _parse_ttl(FULL_PROFILE_FILE)
    imports = {
        str(o) for o in full.objects(predicate=OWL.imports) if isinstance(o, URIRef)
    }
    slices = discover_slices()
    extensions = {s.iri for s in slices.values() if not s.is_core}
    assert imports == {ONTOLOGY_IRI} | extensions

    # closure sanity: root(core) + extensions covers every discovered slice
    core = {s.iri for s in slices.values() if s.is_core}
    assert core | extensions == {s.iri for s in slices.values()}


def test_claims_profile_is_genuinely_sub_core() -> None:
    """The slim ruling made measurable: claims ⊂ core, strictly, and
    carries no extension."""
    from gmeow_tools.config import PROFILES_DIR
    from gmeow_tools.slices import discover_slices

    slices = discover_slices()
    core = {s.iri for s in slices.values() if s.is_core}
    doc = _parse_ttl(PROFILES_DIR / "claims.ttl")
    imports = {
        str(o) for o in doc.objects(predicate=OWL.imports) if isinstance(o, URIRef)
    }
    assert imports < core, "claims must be a strict subset of core"


def test_all_modules_are_in_catalog() -> None:
    """Every ontology/modules/*.ttl owl:Ontology must be mapped in catalog."""
    catalog = ET.parse(CATALOG_FILE)
    ns = {"catalog": "urn:oasis:names:tc:entity:xmlns:xml:catalog"}
    catalog_iris = {uri.get("name") for uri in catalog.findall(".//catalog:uri", ns)}

    missing: list[str] = []
    for module_path in iter_module_files():
        module_graph = _parse_ttl(module_path)
        ontology_subjects = list(module_graph.subjects(RDF.type, OWL.Ontology))
        module_iri = str(ontology_subjects[0])
        if module_iri not in catalog_iris:
            missing.append(module_iri)

    assert not missing, f"Modules missing from catalog-v001.xml: {missing}"


def test_module_iri_matches_filename() -> None:
    """Each module's owl:Ontology IRI follows its location.

    Flat modules: ``…/gmeow/modules/<stem>``. Slice modules
    (``slices/<group>/<name>/module.ttl``, #287): ``…/gmeow/slices/<name>`` —
    the unified slice IRI, named by the slice directory, never the group.
    """
    mismatches: list[tuple[str, str]] = []
    for module_path in iter_module_files():
        module_graph = _parse_ttl(module_path)
        ontology_subjects = list(module_graph.subjects(RDF.type, OWL.Ontology))
        module_iri = str(ontology_subjects[0])
        if module_path.name == "module.ttl":
            expected = f"{NAMESPACE}slices/{module_path.parent.name}"
        else:
            expected = GMEOW_MODULES + module_path.stem
        if module_iri != expected:
            mismatches.append((module_path.name, module_iri))

    assert not mismatches, f"Module IRI / filename mismatches: {mismatches}"


# --------------------------------------------------------------------------- #
# Check B — fixture GMEOW terms must be declared
# --------------------------------------------------------------------------- #


def _declared_ontology_terms() -> set[str]:
    """Build the set of GMEOW terms declared as subjects in the ontology."""
    declared: set[str] = set()
    for path in [ONTOLOGY_FILE, *iter_module_files()]:
        declared.update(_subjects(_parse_ttl(path)))
    # Exclude module ontology IRIs themselves — they are not vocabulary terms.
    declared = {s for s in declared if not s.startswith(GMEOW_MODULES)}
    return declared


@pytest.fixture(scope="session")
def declared_ontology_terms() -> set[str]:
    return _declared_ontology_terms()


def test_coverage_fixtures_use_only_declared_terms(
    declared_ontology_terms: set[str],
) -> None:
    """Coverage fixtures must not use undeclared GMEOW vocabulary terms."""
    undeclared: dict[str, list[str]] = {}
    for fixture_path in sorted(FIXTURES_DIR.glob("*.ttl")):
        fixture_graph = _parse_ttl(fixture_path)
        terms = _gmeow_vocabulary_terms(fixture_graph)
        bad = sorted(terms - declared_ontology_terms)
        if bad:
            undeclared[fixture_path.name] = bad

    messages = []
    for name, bad in undeclared.items():
        messages.append(f"  {name}: {bad}")
    assert not undeclared, "Undeclared GMEOW terms in coverage fixtures:\n" + "\n".join(
        messages
    )


def test_slice_examples_use_only_declared_terms(
    declared_ontology_terms: set[str],
) -> None:
    """Slice worked examples must use only DECLARED GMEOW vocabulary terms.

    SHACL does not catch an undeclared predicate — an unrecognized GMEOW IRI in
    an example A-box trips no shape, so it sits inert and silently fails to link
    anything (e.g. a typo'd ``gmeow:hasBeginInstant`` for ``gmeow:hasStartInstant``
    leaves the interval's start instant unattached while still "validating").
    This surface check, the example-side sibling of
    :func:`test_coverage_fixtures_use_only_declared_terms`, fails closed on any
    GMEOW term an example uses that the ontology never declares.
    """
    example_files = iter_slice_example_files()
    assert example_files, "No slice example files found to validate!"
    undeclared: dict[str, list[str]] = {}
    for example_path in example_files:
        example_graph = _parse_ttl(example_path)
        terms = _gmeow_vocabulary_terms(example_graph)
        bad = sorted(terms - declared_ontology_terms)
        if bad:
            undeclared[example_path.relative_to(PROJECT_ROOT).as_posix()] = bad

    messages = [f"  {name}: {bad}" for name, bad in undeclared.items()]
    assert not undeclared, "Undeclared GMEOW terms in slice examples:\n" + "\n".join(
        messages
    )


def _iter_slice_source_files() -> list[Path]:
    """Every hand-authored TTL under ``slices/`` — modules, shapes, mappings, examples.

    Generated artifacts live under ``generated/`` and are governed instead by the
    internal-tag leak gate (they carry public ``@en``), so they are out of scope here.
    """
    return [
        *iter_slice_module_files(),
        *iter_slice_shape_files(),
        *iter_slice_mapping_files(),
        *iter_slice_example_files(),
    ]


# Hand-authored TTL OUTSIDE ``slices/`` — the global SHACL shapes, the
# constitution, and the DSL grammars. These were retrofitted alongside the
# slice corpus (#474 follow-through): a translation has to attach a sibling
# literal beside every source rendering wherever it lives, not just in slices.
# Generated artifacts (``generated/``) carry public ``@en`` and are governed by
# the internal-tag leak gate instead, so they stay out of scope here.
GOVERNANCE_DIR = PROJECT_ROOT / "governance"


def _iter_nonslice_authored_files() -> list[Path]:
    """Authored TTL outside ``slices/``: shapes, governance, and DSL grammars."""
    files: list[Path] = []
    files.extend(sorted(SHAPES_DIR.glob("*.ttl")))
    files.extend(sorted(GOVERNANCE_DIR.glob("*.ttl")))
    files.extend(sorted(MAPPING_DSL_DIR.rglob("*.ttl")))
    files.extend(sorted(STATEMENT_DSL_DIR.rglob("*.ttl")))
    return files


def _untagged_localizable_literals(paths: list[Path]) -> dict[str, list[str]]:
    """Map each file with plain (untagged) localizable literals to those literals.

    A localizable literal carrying any language tag (``@x-gmeow-english`` for
    authoring, ``@en`` for already-public governance vocab) satisfies the
    discipline; only a *plain* literal — distinct RDF term from any tagged
    sibling, hence silently untranslatable — fails closed.
    """
    untagged: dict[str, list[str]] = {}
    for path in paths:
        graph = _parse_ttl(path)
        bad = sorted(
            f"{graph.namespace_manager.normalizeUri(str(p))} {obj!r}"
            for _, p, obj in graph
            if p in LOCALIZABLE_PREDICATES
            and isinstance(obj, Literal)
            and not obj.language
        )
        if bad:
            untagged[path.relative_to(PROJECT_ROOT).as_posix()] = bad
    return untagged


def test_slice_source_localizable_literals_are_language_tagged() -> None:
    """Localizable literals in slice source must carry a language tag.

    The ontology is about to be translated wholesale (Mandarin, French). A
    translation works by attaching a language-tagged sibling literal beside each
    source rendering — which is impossible if the source literal is *plain* (no
    tag), because a plain literal and a tagged one are distinct RDF terms with no
    declared relationship. A plain ``rdfs:label`` or ``gmeow:name`` therefore
    silently becomes untranslatable. SHACL does not catch this (a plain literal
    satisfies any ``sh:datatype rdfs:Literal`` / language-free shape), so this
    surface gate fails closed on every untagged localizable literal — the
    translation-readiness sibling of the declared-term-surface checks above.
    """
    source_files = _iter_slice_source_files()
    assert source_files, "No slice source files found to validate!"
    untagged = _untagged_localizable_literals(source_files)

    messages = [
        f"  {name}:\n    " + "\n    ".join(bad) for name, bad in untagged.items()
    ]
    assert not untagged, (
        "Plain (untagged) localizable literals in slice source — every label, "
        "name, title, definition and comment must carry a language tag "
        "(@x-gmeow-english for authoring) so translations can attach:\n"
        + "\n".join(messages)
    )


def test_nonslice_authored_localizable_literals_are_language_tagged() -> None:
    """Authored TTL outside ``slices/`` must also carry language tags.

    The translation-readiness discipline is corpus-wide, not slice-local: the
    global SHACL shapes (``shapes/``), the constitution (``governance/``) and the
    DSL grammars (``dsl/``) carry human-readable ``rdfs:label`` / ``skos:definition``
    / ``rdfs:comment`` prose too, and a plain literal there is just as
    untranslatable as one in a slice. This is the corpus-wide sibling of
    :func:`test_slice_source_localizable_literals_are_language_tagged`; together
    they cover every hand-authored localizable literal in the repository.
    """
    authored_files = _iter_nonslice_authored_files()
    assert authored_files, "No non-slice authored files found to validate!"
    untagged = _untagged_localizable_literals(authored_files)

    messages = [
        f"  {name}:\n    " + "\n    ".join(bad) for name, bad in untagged.items()
    ]
    assert not untagged, (
        "Plain (untagged) localizable literals in authored shapes / governance / "
        "DSL — every label, name, title, definition and comment must carry a "
        "language tag (@x-gmeow-english for authoring, @en for public governance "
        "vocab) so translations can attach:\n" + "\n".join(messages)
    )


# --------------------------------------------------------------------------- #
# Check C — docs/examples lint for authored GMEOW terms
# --------------------------------------------------------------------------- #


def _dsl_subjects(dsl_dir: Path) -> set[str]:
    """Collect all subjects from every TTL file under a DSL directory."""
    subjects: set[str] = set()
    for path in sorted(dsl_dir.rglob("*.ttl")):
        subjects.update(_subjects(_parse_ttl(path)))
    return subjects


def _docs_allowlist() -> set[str]:
    """Terms that are legitimate in docs prose."""
    allowed = _declared_ontology_terms()
    # Shapes are referenced in reasoning/rights/standpoints docs.
    if SHAPES_FILE.exists():
        allowed.update(_subjects(_parse_ttl(SHAPES_FILE)))
    # Mapping DSL terms are referenced in projections docs.
    allowed.update(_dsl_subjects(MAPPING_DSL_DIR))
    # Statement DSL terms are referenced in statement docs.
    allowed.update(_dsl_subjects(STATEMENT_DSL_DIR))
    # Slice DSL terms (manifest fields) are referenced in the CLI roll-up doc.
    if SLICE_VOCABULARY_FILE.exists():
        allowed.update(_subjects(_parse_ttl(SLICE_VOCABULARY_FILE)))
    # Test-DSL terms (competency/structural/conformance cells) are referenced in
    # the docs/TESTING.md examples that explain the slice-test harness.
    if TEST_DSL_VOCABULARY_FILE.exists():
        allowed.update(_subjects(_parse_ttl(TEST_DSL_VOCABULARY_FILE)))
    # Retired terms intentionally mentioned in migration/retirement prose.
    allowed.update(_RETIRED_DOCS_TERMS)
    return allowed


def _find_gmeow_terms_in_markdown(path: Path) -> set[str]:
    """Scan a Markdown file for gmeow: terms in fenced turtle blocks and inline code."""
    text = path.read_text(encoding="utf-8")
    found: set[str] = set()

    # Fenced turtle blocks
    for block in _TURTLE_BLOCK_RE.findall(text):
        for match in _INLINE_TERM_RE.finditer(block):
            found.add(NAMESPACE + match.group(1))
        # Also catch bare gmeow:LocalName in Turtle (not backticked)
        for match in re.finditer(r"\bgmeow:([A-Za-z][A-Za-z0-9_]*)\b", block):
            found.add(NAMESPACE + match.group(1))

    # Inline backticked terms outside blocks (e.g. `gmeow:hasName`)
    for match in _INLINE_TERM_RE.finditer(text):
        found.add(NAMESPACE + match.group(1))

    return found


@pytest.fixture(scope="session")
def docs_allowlist() -> set[str]:
    return _docs_allowlist()


def test_docs_examples_use_only_allowed_terms(docs_allowlist: set[str]) -> None:
    """User-copyable docs examples must not use unallowlisted gmeow: IRIs."""
    unallowed: dict[str, list[str]] = {}
    for doc_path in sorted(DOCS_DIR.glob("*.md")):
        terms = _find_gmeow_terms_in_markdown(doc_path)
        bad = sorted(terms - docs_allowlist)
        if bad:
            unallowed[doc_path.name] = bad

    messages = []
    for name, bad in unallowed.items():
        messages.append(f"  {name}: {bad}")
    assert not unallowed, "Unallowlisted gmeow: terms in docs examples:\n" + "\n".join(
        messages
    )
