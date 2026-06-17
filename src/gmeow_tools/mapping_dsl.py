"""Parse the GMEOW mapping DSL and render its closed SPARQL algebra.

The mapping DSL (``mapping-dsl/*.ttl``, vocabulary in ``mapping-dsl/vocabulary.ttl``)
is the single authoring source for GMEOW's alignment layer. This module reads it
into typed dataclasses and renders the structural pieces — property paths,
expressions, graph-pattern atoms — into SPARQL fragments. The artifact emitters
(SSSOM / EDOAL / FnO / SPARQL) live in :mod:`gmeow_tools.mapping_compile`; the
split keeps the *model + grammar* here and the *renderers* there.

Nothing in the DSL is raw SPARQL: composed values and minted nodes are expressed
as a closed operator algebra (CONCAT/COALESCE/IF/BOUND/STR/IRI/STRDT/regex) and a
small property-path algebra (alt/seq/zero-or-more), which this module walks into
SPARQL text deterministically.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

from rdflib import RDF, RDFS, SKOS, Graph, Literal, URIRef
from rdflib.collection import Collection
from rdflib.namespace import Namespace
from rdflib.term import Node

from gmeow_tools.config import MAPPING_DSL_DIR, PREFIXES
from gmeow_tools.dsl_validate import validate_mapping_dsl
from gmeow_tools.slices import iter_slice_mapping_files

GM = Namespace(PREFIXES["gmeow"])

#: Reverse prefix map (longest namespace first) for CURIE shortening.
_NS_TO_PREFIX: tuple[tuple[str, str], ...] = tuple(
    sorted(((ns, p) for p, ns in PREFIXES.items()), key=lambda kv: -len(kv[0]))
)


class CompileError(ValueError):
    """Raised on a malformed DSL cell (dangling var, bad shape, unknown profile)."""


def curie(node: URIRef) -> str:
    """Shorten an IRI to ``prefix:local`` using the canonical registry."""
    iri = str(node)
    for ns, prefix in _NS_TO_PREFIX:
        if iri.startswith(ns):
            return f"{prefix}:{iri[len(ns) :]}"
    return f"<{iri}>"


def sparql_string(text: str) -> str:
    """Render a Python string as a single-line SPARQL string literal.

    Escapes the backslash, double-quote and whitespace control characters so an
    embedded newline/tab never breaks out of the ``"..."`` literal.
    """
    escaped = (
        text.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )
    return f'"{escaped}"'


# --------------------------------------------------------------------------- #
# Model
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class Expr:
    """An expression-algebra node: a variable, a constant, or an operator app."""

    var: str | None = None
    const: Literal | URIRef | None = None
    op: URIRef | None = None
    args: tuple[Expr, ...] = ()


@dataclass(frozen=True, slots=True)
class Atom:
    """One graph-pattern (or template) atom: subject, predicate/path, object."""

    subject_var: str
    predicate: URIRef | None = None
    predicate_var: str | None = None
    path: str | None = None  # pre-rendered SPARQL property path
    path_alts: tuple[URIRef, ...] = ()  # alternatives, when path is a top-level AltPath
    object_var: str | None = None
    object_value: URIRef | None = None
    object_literal: Literal | None = None
    optional: bool = False


@dataclass(frozen=True, slots=True)
class OptionalGroup:
    """A nested ``OPTIONAL { … }`` group of pattern items (atoms or groups)."""

    items: tuple[Atom | OptionalGroup, ...]


@dataclass(frozen=True, slots=True)
class Bind:
    """A derived binding (BIND expr AS ?var) or a minted IRI."""

    var: str
    expr: Expr


@dataclass(frozen=True, slots=True)
class ValueClass:
    """One entry of a value→class table (whenValue → toClass)."""

    when_value: URIRef
    to_class: URIRef


@dataclass(frozen=True, slots=True)
class MappingPattern:
    """The GMEOW-side pattern of a projection mapping."""

    anchor: str
    value: str | None
    atoms: tuple[Atom | OptionalGroup, ...]
    suppress_when: tuple[Atom, ...] = ()
    project_when: tuple[Atom, ...] = ()
    exclude_when: tuple[Atom, ...] = ()
    filters: tuple[Expr, ...] = ()
    binds: tuple[Bind, ...] = ()
    mints: tuple[Bind, ...] = ()
    edoal_source: URIRef | None = None
    edoal_source_kind: str = "relation"
    edoal_path: bool = False


@dataclass(frozen=True, slots=True)
class ProfileBinding:
    """A per-profile output face of a projection mapping."""

    profile: str
    to_predicate: URIRef | None = None
    to_class: URIRef | None = None
    template_atoms: tuple[Atom, ...] = ()
    value_class_map: tuple[ValueClass, ...] = ()
    relation: str = "="
    transform: URIRef | None = None
    confidence: float | None = None
    lossy_drops: tuple[str, ...] = ()
    emit_sssom: bool = False
    sssom_predicate: URIRef | None = None
    sssom_file: str | None = None
    edoal_target: URIRef | None = None
    edoal_target_kind: str | None = None


@dataclass(frozen=True, slots=True)
class ProjectionCell:
    """A projection mapping: a pattern + its per-profile bindings."""

    iri: URIRef
    label: str
    pattern: MappingPattern
    bindings: tuple[ProfileBinding, ...]


@dataclass(frozen=True, slots=True)
class EquivalenceCell:
    """A pure cross-ontology term link (compiles to one SSSOM row)."""

    iri: URIRef
    subject: URIRef
    predicate: URIRef
    obj: URIRef
    confidence: float | None
    justification: URIRef | None
    comment: str
    sssom_file: str
    subject_label: str = ""
    object_label: str = ""


@dataclass(frozen=True, slots=True)
class ProjectionFunction:
    """An FnO projection-transform source declaration (from vocabulary.ttl)."""

    iri: URIRef
    label: str
    description: str
    inputs: tuple[URIRef, ...]
    optional_inputs: tuple[URIRef, ...]
    output: URIRef
    output_type: URIRef


@dataclass(frozen=True, slots=True)
class MappingSet:
    """Per-file SSSOM header metadata for one generated .sssom.tsv."""

    file: str
    set_id: str
    license: str
    comment: str
    trailer: str = ""


@dataclass(frozen=True, slots=True)
class Dsl:
    """The fully parsed DSL: cells + transform declarations."""

    equivalences: tuple[EquivalenceCell, ...]
    projections: tuple[ProjectionCell, ...]
    functions: dict[URIRef, ProjectionFunction] = field(default_factory=dict)
    mapping_sets: dict[str, MappingSet] = field(default_factory=dict)


# --------------------------------------------------------------------------- #
# Rendering — the closed SPARQL algebra (no graph needed; values pre-extracted)
# --------------------------------------------------------------------------- #

#: Function-call expression operators, rendered ``NAME(arg, …)``.
_FUNC_OPS: dict[str, str] = {
    "opConcat": "CONCAT",
    "opCoalesce": "COALESCE",
    "opIf": "IF",
    "opBound": "BOUND",
    "opStr": "STR",
    "opIri": "IRI",
    "opStrDatatype": "STRDT",
    # string / language / datatype functions
    "opLang": "LANG",
    "opLangMatches": "LANGMATCHES",
    "opStrLang": "STRLANG",
    "opDatatype": "DATATYPE",
    "opSubstr": "SUBSTR",
    "opReplace": "REPLACE",
    "opUcase": "UCASE",
    "opLcase": "LCASE",
    "opStrBefore": "STRBEFORE",
    "opStrAfter": "STRAFTER",
    "opStrLen": "STRLEN",
    "opContains": "CONTAINS",
    "opStrStarts": "STRSTARTS",
    "opStrEnds": "STRENDS",
    "opEncodeForUri": "ENCODE_FOR_URI",
    # numeric cast: normalize a value to canonical xsd:decimal (e.g. a source's
    # scientific-notation xsd:double coordinate 5.35e+01 → 53.5, an integer 47 →
    # 47) — a real cast, unlike STRDT which would retype the illegal lexical form.
    "opDecimal": "xsd:decimal",
}

#: Infix expression operators, rendered ``(a OP b OP …)``.
_INFIX_OPS: dict[str, str] = {
    "opAdd": "+",
    "opSub": "-",
    "opMul": "*",
    "opDiv": "/",
    "opEq": "=",
    "opNe": "!=",
    "opLt": "<",
    "opGt": ">",
    "opLe": "<=",
    "opGe": ">=",
    "opAnd": "&&",
    "opOr": "||",
}


def render_expr(expr: Expr) -> str:
    """Render an expression node to a SPARQL expression string."""
    if expr.var is not None:
        return f"?{expr.var}"
    if expr.const is not None:
        if isinstance(expr.const, URIRef):
            return curie(expr.const)
        return sparql_string(str(expr.const))
    if expr.op is None:
        raise CompileError("empty expression node")
    name = str(expr.op).rsplit("/", 1)[-1].rsplit("#", 1)[-1]
    rendered = [render_expr(a) for a in expr.args]
    if name == "opRegex":
        return f"regex({', '.join(rendered)})"
    if name == "opNot":
        return f"(!{rendered[0]})"
    if name == "opIn":
        return f"({rendered[0]} IN ({', '.join(rendered[1:])}))"
    if name in _INFIX_OPS:
        return "(" + f" {_INFIX_OPS[name]} ".join(rendered) + ")"
    fn = _FUNC_OPS.get(name)
    if fn is None:
        raise CompileError(f"unknown expression operator {name}")
    return f"{fn}({', '.join(rendered)})"


# --------------------------------------------------------------------------- #
# Parsing
# --------------------------------------------------------------------------- #


def _rdf_list(graph: Graph, head: Node | None) -> list[Node]:
    """Return the members of an rdf:List head node (empty if head is None)."""
    if head is None:
        return []
    return list(Collection(graph, head))


def _uri(node: object) -> URIRef | None:
    return node if isinstance(node, URIRef) else None


def _as_bool(node: object) -> bool:
    """Parse an RDF boolean literal to a Python bool (RDF ``false`` stays False).

    ``bool(Literal("false"))`` on an *untyped* literal is truthy (non-empty str);
    parse the term's value explicitly so an authored ``false`` is honoured.
    """
    if isinstance(node, Literal):
        value = node.toPython()
        if isinstance(value, bool):
            return value
        return str(value).strip().lower() in ("true", "1")
    return bool(node)


def _str(node: object) -> str | None:
    return str(node) if node is not None else None


def _path_primary(graph: Graph, node: Node | None) -> str:
    """Render a sub-path, parenthesising composite forms (for use under ^/*/+/?)."""
    if node is None:
        raise CompileError("property path missing a step")
    rendered = _render_path(graph, node)
    # A sequence/alternation is not a path-primary; parenthesise it.
    if any(sep in rendered for sep in ("/", "|")):
        return f"({rendered})"
    return rendered


def _render_path(graph: Graph, node: Node) -> str:
    """Render a structured property-path node to SPARQL path syntax."""
    if isinstance(node, URIRef):
        return "rdf:type" if node == RDF.type else curie(node)
    types = set(graph.objects(node, RDF.type))
    if GM.AltPath in types:
        alts = _rdf_list(graph, graph.value(node, GM.pathAlts))
        return "|".join(_render_path(graph, a) for a in alts)
    if GM.SeqPath in types:
        steps = _rdf_list(graph, graph.value(node, GM.pathSteps))
        return "/".join(_render_path(graph, s) for s in steps)
    if GM.InversePath in types:
        return f"^{_path_primary(graph, graph.value(node, GM.pathStep))}"
    if GM.ZeroOrMorePath in types:
        return f"{_path_primary(graph, graph.value(node, GM.pathStep))}*"
    if GM.OneOrMorePath in types:
        return f"{_path_primary(graph, graph.value(node, GM.pathStep))}+"
    if GM.ZeroOrOnePath in types:
        return f"{_path_primary(graph, graph.value(node, GM.pathStep))}?"
    if GM.NegatedPropertySet in types:
        members = _rdf_list(graph, graph.value(node, GM.pathSet))
        inner = "|".join(_render_path(graph, m) for m in members)
        return f"!({inner})" if len(members) > 1 else f"!{inner}"
    raise CompileError(f"unknown property-path node {node!r}")


def _expr(graph: Graph, node: Node) -> Expr:
    if isinstance(node, Literal | URIRef):
        return Expr(const=node)
    var = graph.value(node, GM.exprVar)
    if var is not None:
        return Expr(var=str(var))
    op = graph.value(node, GM.exprOp)
    if not isinstance(op, URIRef):
        raise CompileError(f"expression node {node!r} has neither exprVar nor exprOp")
    args = tuple(
        _expr(graph, a) for a in _rdf_list(graph, graph.value(node, GM.exprArgs))
    )
    return Expr(op=op, args=args)


def _alt_members(graph: Graph, node: Node | None) -> tuple[URIRef, ...]:
    """If ``node`` is a top-level AltPath of plain predicates, return them; else ()."""
    if node is None or isinstance(node, URIRef):
        return ()
    if GM.AltPath not in set(graph.objects(node, RDF.type)):
        return ()
    members = _rdf_list(graph, graph.value(node, GM.pathAlts))
    if all(isinstance(m, URIRef) for m in members):
        return tuple(m for m in members if isinstance(m, URIRef))
    return ()


def _atom(graph: Graph, node: Node) -> Atom:
    subj = graph.value(node, GM.subjectVar) or graph.value(node, GM.tSubj)
    if subj is None:
        raise CompileError(f"atom {node!r} missing subjectVar/tSubj")
    predicate = graph.value(node, GM.predicate) or graph.value(node, GM.tPred)
    predicate_var = graph.value(node, GM.predicateVar)
    path_node = graph.value(node, GM.path)
    obj_var = graph.value(node, GM.objectVar) or graph.value(node, GM.tObj)
    obj_value = graph.value(node, GM.objectValue) or graph.value(node, GM.tObjValue)
    obj_literal = graph.value(node, GM.objectLiteral)
    return Atom(
        subject_var=str(subj),
        predicate=_uri(predicate),
        predicate_var=str(predicate_var) if predicate_var is not None else None,
        path=_render_path(graph, path_node) if path_node is not None else None,
        path_alts=_alt_members(graph, path_node),
        object_var=str(obj_var) if obj_var is not None else None,
        object_value=_uri(obj_value),
        object_literal=obj_literal if isinstance(obj_literal, Literal) else None,
        optional=_as_bool(graph.value(node, GM.optional)),
    )


def _item(graph: Graph, node: Node) -> Atom | OptionalGroup:
    """Parse a pattern item: an OptionalGroup (gmeow:optionalGroup) or an Atom."""
    group = graph.value(node, GM.optionalGroup)
    if group is not None:
        return OptionalGroup(
            items=tuple(_item(graph, i) for i in _rdf_list(graph, group))
        )
    return _atom(graph, node)


def _bind(graph: Graph, node: Node) -> Bind:
    var = graph.value(node, GM.bindVar)
    expr_node = graph.value(node, GM.bindExpr)
    if var is None or expr_node is None:
        raise CompileError(f"bind/mint {node!r} missing bindVar/bindExpr")
    return Bind(var=str(var), expr=_expr(graph, expr_node))


def _pattern(graph: Graph, node: Node) -> MappingPattern:
    anchor = graph.value(node, GM.anchor)
    if anchor is None:
        raise CompileError(f"mapping pattern {node!r} missing anchor")
    value = graph.value(node, GM.value)
    return MappingPattern(
        anchor=str(anchor),
        value=str(value) if value is not None else None,
        atoms=tuple(
            _item(graph, a) for a in _rdf_list(graph, graph.value(node, GM.atom))
        ),
        suppress_when=tuple(
            _atom(graph, a)
            for a in sorted(graph.objects(node, GM.suppressWhen), key=str)
        ),
        project_when=tuple(
            _atom(graph, a)
            for a in sorted(graph.objects(node, GM.projectWhen), key=str)
        ),
        exclude_when=tuple(
            _atom(graph, a)
            for a in sorted(graph.objects(node, GM.excludeWhen), key=str)
        ),
        filters=tuple(_expr(graph, f) for f in graph.objects(node, GM.filter)),
        binds=tuple(_bind(graph, b) for b in graph.objects(node, GM.bind)),
        mints=tuple(_bind(graph, m) for m in graph.objects(node, GM.mint)),
        edoal_source=_uri(graph.value(node, GM.edoalSource)),
        edoal_source_kind=str(graph.value(node, GM.edoalSourceKind) or "relation"),
        edoal_path=_as_bool(graph.value(node, GM.edoalPath)),
    )


def _value_class_map(graph: Graph, head: Node | None) -> tuple[ValueClass, ...]:
    out: list[ValueClass] = []
    for entry in _rdf_list(graph, head):
        when = graph.value(entry, GM.whenValue)
        to_class = graph.value(entry, GM.toClass)
        if not isinstance(when, URIRef) or not isinstance(to_class, URIRef):
            raise CompileError(f"value-class entry {entry!r} malformed")
        out.append(ValueClass(when_value=when, to_class=to_class))
    return tuple(out)


def _binding(graph: Graph, node: Node) -> ProfileBinding:
    profile = graph.value(node, GM.profile)
    if profile is None:
        raise CompileError(f"profile binding {node!r} missing profile")
    confidence = graph.value(node, GM.confidence)
    relation = graph.value(node, GM.relation)
    return ProfileBinding(
        profile=str(profile),
        to_predicate=_uri(graph.value(node, GM.toPredicate)),
        to_class=_uri(graph.value(node, GM.toClass)),
        template_atoms=tuple(
            _atom(graph, a)
            for a in _rdf_list(graph, graph.value(node, GM.templateAtoms))
        ),
        value_class_map=_value_class_map(graph, graph.value(node, GM.valueClassMap)),
        relation=str(relation) if relation is not None else "=",
        transform=_uri(graph.value(node, GM.transform)),
        confidence=float(str(confidence)) if confidence is not None else None,
        lossy_drops=tuple(str(d) for d in graph.objects(node, GM.lossyDrop)),
        emit_sssom=_as_bool(graph.value(node, GM.emitSssom)),
        sssom_predicate=_uri(graph.value(node, GM.sssomPredicate)),
        sssom_file=_str(graph.value(node, GM.sssomFile)),
        edoal_target=_uri(graph.value(node, GM.edoalTarget)),
        edoal_target_kind=_str(graph.value(node, GM.edoalTargetKind)),
    )


def _functions(graph: Graph) -> dict[URIRef, ProjectionFunction]:
    out: dict[URIRef, ProjectionFunction] = {}
    for fn in graph.subjects(RDF.type, GM.ProjectionFunction):
        if not isinstance(fn, URIRef):
            continue
        output = graph.value(fn, GM.fnOutput)
        output_type = graph.value(fn, GM.fnOutputType)
        if not isinstance(output, URIRef) or not isinstance(output_type, URIRef):
            raise CompileError(
                f"projection function {fn} missing fnOutput/fnOutputType"
            )
        out[fn] = ProjectionFunction(
            iri=fn,
            label=str(graph.value(fn, RDFS.label) or ""),
            description=str(graph.value(fn, SKOS.definition) or ""),
            inputs=tuple(
                o for o in graph.objects(fn, GM.fnInput) if isinstance(o, URIRef)
            ),
            optional_inputs=tuple(
                o
                for o in graph.objects(fn, GM.fnInputOptional)
                if isinstance(o, URIRef)
            ),
            output=output,
            output_type=output_type,
        )
    return out


def _equivalences(graph: Graph) -> list[EquivalenceCell]:
    cells: list[EquivalenceCell] = []
    for cell in graph.subjects(RDF.type, GM.TermEquivalence):
        subject = graph.value(cell, GM.alignSubject)
        predicate = graph.value(cell, GM.alignPredicate)
        obj = graph.value(cell, GM.alignObject)
        sssom_file = graph.value(cell, GM.sssomFile)
        if not all(isinstance(x, URIRef) for x in (subject, predicate, obj)):
            raise CompileError(
                f"term equivalence {cell} missing subject/predicate/object"
            )
        if sssom_file is None:
            raise CompileError(f"term equivalence {cell} missing sssomFile")
        conf = graph.value(cell, GM.confidence)
        cells.append(
            EquivalenceCell(
                iri=cell if isinstance(cell, URIRef) else URIRef(str(cell)),
                subject=subject,  # type: ignore[arg-type]
                predicate=predicate,  # type: ignore[arg-type]
                obj=obj,  # type: ignore[arg-type]
                confidence=float(str(conf)) if conf is not None else None,
                justification=_uri(graph.value(cell, GM.justification)),
                comment=str(graph.value(cell, GM.comment) or ""),
                sssom_file=str(sssom_file),
                subject_label=str(graph.value(cell, GM.subjectLabel) or ""),
                object_label=str(graph.value(cell, GM.objectLabel) or ""),
            )
        )
    return cells


def _projections(graph: Graph) -> list[ProjectionCell]:
    cells: list[ProjectionCell] = []
    for cell in graph.subjects(RDF.type, GM.ProjectionMapping):
        if not isinstance(cell, URIRef):
            raise CompileError("projection mapping must be a named IRI")
        pattern_node = graph.value(cell, GM.hasMappingPattern)
        if pattern_node is None:
            raise CompileError(f"projection mapping {cell} missing hasMappingPattern")
        bindings = tuple(_binding(graph, b) for b in graph.objects(cell, GM.hasBinding))
        if not bindings:
            raise CompileError(f"projection mapping {cell} has no bindings")
        cells.append(
            ProjectionCell(
                iri=cell,
                label=str(graph.value(cell, RDFS.label) or ""),
                pattern=_pattern(graph, pattern_node),
                bindings=bindings,
            )
        )
    return cells


def _mapping_sets(graph: Graph) -> dict[str, MappingSet]:
    out: dict[str, MappingSet] = {}
    for node in graph.subjects(RDF.type, GM.MappingSet):
        file = graph.value(node, GM.sssomFile)
        if file is None:
            raise CompileError(f"mapping set {node} missing sssomFile")
        out[str(file)] = MappingSet(
            file=str(file),
            set_id=str(graph.value(node, GM.setId) or ""),
            license=str(graph.value(node, GM.license) or ""),
            comment=str(graph.value(node, GM.setComment) or ""),
            trailer=str(graph.value(node, GM.setTrailer) or ""),
        )
    return out


@lru_cache(maxsize=2)
def load_dsl(src: Path = MAPPING_DSL_DIR) -> Dsl:
    """Parse the whole DSL (vocabulary + equivalence + projection cells).

    Parsing the full mapping DSL (~60 Turtle files, twice each for provenance)
    costs ~15 s, and the compiler tests call this ~30 times; the result is a
    frozen, never-mutated :class:`Dsl`, so it is cached by source directory. The
    cache is keyed on ``src`` and assumes the DSL files do not change within a
    process (true for the CLI, the test suite, and CI).
    """
    graph = Graph()
    sources = sorted(src.rglob("*.ttl"))
    if src == MAPPING_DSL_DIR:
        # Slices carry their own mapping cells (slices/*/*/mappings/*.ttl,
        # #287); the compiler merges them with the shared DSL tree.
        sources += iter_slice_mapping_files()
    for path in sources:
        graph.parse(path, format="turtle")
    # SHACL validation + focus→file provenance run in Rust over the same source
    # paths (#579): the compiler keeps the rdflib graph only for its dataclass
    # extraction below.
    violations = validate_mapping_dsl([str(path) for path in sources])
    if violations:
        raise CompileError(
            "mapping DSL SHACL violations:\n  " + "\n  ".join(violations)
        )
    return Dsl(
        equivalences=tuple(_equivalences(graph)),
        projections=tuple(_projections(graph)),
        functions=_functions(graph),
        mapping_sets=_mapping_sets(graph),
    )
