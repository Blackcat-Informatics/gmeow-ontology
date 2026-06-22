"""Musical-performance layer guards (performance constraints).

Principles 4, 9, 10, 11, 12, 16.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Literal, Namespace, URIRef
from tests._graph_nt import run_shacl

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-performance/")

_MERGED_GRAPH: Graph | None = None


def _graph() -> Graph:
    global _MERGED_GRAPH
    if _MERGED_GRAPH is None:
        _MERGED_GRAPH = load_merged_graph(include_imports=False)
    return _MERGED_GRAPH


def _error_text(result: ValidationResult) -> str:
    return "\n".join(result.errors)


def test_degree_of_freedom_classes_exist() -> None:
    graph = _graph()
    for term in (
        "DegreeOfFreedom",
        "MusicalParameter",
        "DeterminationStatus",
        "TraversalConstraint",
        "PerformanceDecision",
        "GenerativeProcess",
        "GenerativeProcessKind",
        "OrnamentProfile",
        "OrnamentProfileKind",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            OWL.Class,
        ) in graph, f"{term} should be an owl:Class"


def test_value_vocabularies_exist() -> None:
    graph = _graph()
    params = (
        "musicalParameterPitch",
        "musicalParameterDuration",
        "musicalParameterOrder",
        "musicalParameterTempo",
        "musicalParameterDynamics",
        "musicalParameterTimbre",
        "musicalParameterInstrumentation",
        "musicalParameterPerformerCount",
        "musicalParameterSoundContent",
        "musicalParameterLocation",
        "musicalParameterTacet",
    )
    for term in params:
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "MusicalParameter"),
        ) in graph, f"{term} should be a MusicalParameter"

    statuses = (
        "determinationFixed",
        "determinationConstrained",
        "determinationFree",
        "determinationDelegatedPerformer",
        "determinationDelegatedEnvironment",
        "determinationDelegatedProcess",
    )
    for term in statuses:
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "DeterminationStatus"),
        ) in graph, f"{term} should be a DeterminationStatus"

    process_kinds = (
        "generativeProcessKindPhasing",
        "generativeProcessKindStochastic",
        "generativeProcessKindVerbalScore",
        "generativeProcessKindRuleBased",
        "generativeProcessKindAlgorithmic",
    )
    for term in process_kinds:
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "GenerativeProcessKind"),
        ) in graph, f"{term} should be a GenerativeProcessKind"

    ornament_kinds = (
        "ornamentProfileKindGamaka",
        "ornamentProfileKindBaroqueAgrement",
        "ornamentProfileKindJazzTurn",
        "ornamentProfileKindMordent",
        "ornamentProfileKindGraceNote",
    )
    for term in ornament_kinds:
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "OrnamentProfileKind"),
        ) in graph, f"{term} should be an OrnamentProfileKind"


def test_performance_functional_properties() -> None:
    graph = _graph()
    functional = [
        "dofWork",
        "dofExpression",
        "dofParameter",
        "dofStatus",
        "constraintAppliesTo",
        "decisionPerformance",
        "decisionConstraint",
        "decisionSequence",
        "processKind",
        "ornamentProfileKind",
        "ornamentReferenceFrame",
    ]
    for prop in functional:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} should be functional"

    non_functional = [
        "dofConstraintText",
        "dofConstraintFunction",
        "mayFollow",
        "constraintText",
        "constraintFunction",
        "processFunction",
        "processParameter",
        "processRuleText",
        "appliesToSegment",
        "appliesToVoice",
        "ornamentDescription",
    ]
    for prop in non_functional:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) not in graph, f"{prop} should NOT be functional"


def test_may_follow_is_not_dl_axiomatized() -> None:
    graph = _graph()
    may_follow = URIRef(GMEOW + "mayFollow")
    # No transitive declaration.
    assert (may_follow, RDF.type, OWL.TransitiveProperty) not in graph
    # No property chain axiom on mayFollow itself, and mayFollow does not
    # appear as a member of any property chain.
    query = """
        PREFIX owl: <http://www.w3.org/2002/07/owl#>
        PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        ASK WHERE {
            {
                gmeow:mayFollow owl:propertyChainAxiom ?chain .
            } UNION {
                ?property owl:propertyChainAxiom ?chain .
                ?chain rdf:rest*/rdf:first gmeow:mayFollow .
            }
        }
    """
    assert graph.query(query).askAnswer is False


def test_four_thirty_three_fixture_exists() -> None:
    graph = _graph()
    work = URIRef(GMEOW + "fixtureFourThirtyThreeWork")
    assert (work, RDF.type, URIRef(GMEOW + "MusicalWork")) in graph
    for term in (
        "dofFourThirtyThreeDuration",
        "dofFourThirtyThreeTacet",
        "dofFourThirtyThreeSoundContent",
        "dofFourThirtyThreeInstrumentation",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "DegreeOfFreedom"),
        ) in graph


def test_klavierstuck_xi_fixture_exists() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "fixtureKlavierstuckXIWork"),
        RDF.type,
        URIRef(GMEOW + "MusicalWork"),
    ) in graph
    assert (
        URIRef(GMEOW + "fixtureKlavierstuckConstraint"),
        RDF.type,
        URIRef(GMEOW + "TraversalConstraint"),
    ) in graph
    for term in (
        "fixtureKlavierstuckFragmentA",
        "fixtureKlavierstuckFragmentB",
        "fixtureKlavierstuckFragmentC",
        "fixtureKlavierstuckFragmentD",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "MusicalSegment"),
        ) in graph
    for term in ("fixtureKlavierstuckDecisionOne", "fixtureKlavierstuckDecisionTwo"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "PerformanceDecision"),
        ) in graph


def test_generative_process_fixture_exists() -> None:
    graph = _graph()
    proc = URIRef(GMEOW + "fixtureReichPhasingProcess")
    assert (proc, RDF.type, URIRef(GMEOW + "GenerativeProcess")) in graph
    assert (
        proc,
        URIRef(GMEOW + "processFunction"),
        URIRef(GMEOW + "fnRealizePhasing"),
    ) in graph
    assert (
        URIRef(GMEOW + "fixtureXenakisStochasticProcess"),
        RDF.type,
        URIRef(GMEOW + "GenerativeProcess"),
    ) in graph


def test_ornament_profile_fixture_exists() -> None:
    graph = _graph()
    prof = URIRef(GMEOW + "fixtureYamanOrnamentProfile")
    voice = URIRef(GMEOW + "fixtureYamanVoice")
    assert (voice, RDF.type, URIRef(GMEOW + "Voice")) in graph
    assert (prof, RDF.type, URIRef(GMEOW + "OrnamentProfile")) in graph
    assert (
        prof,
        URIRef(GMEOW + "ornamentProfileKind"),
        URIRef(GMEOW + "ornamentProfileKindGamaka"),
    ) in graph
    assert (
        prof,
        URIRef(GMEOW + "appliesToVoice"),
        voice,
    ) in graph


def test_graphic_score_fixture_exists() -> None:
    graph = _graph()
    work = URIRef(GMEOW + "fixtureGraphicScoreWork")
    visual = URIRef(GMEOW + "fixtureGraphicScoreVisual")
    transcription = URIRef(GMEOW + "fixtureGraphicScoreTranscription")
    assert (work, RDF.type, URIRef(GMEOW + "MusicalWork")) in graph
    assert (visual, RDF.type, URIRef(GMEOW + "ScoreEdition")) in graph
    assert (transcription, RDF.type, URIRef(GMEOW + "Expression")) in graph
    assert (
        transcription,
        URIRef(GMEOW + "wasDerivedFrom"),
        visual,
    ) in graph


def test_degree_of_freedom_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    dof = EX.dofValid
    g.add((dof, RDF.type, GMEOW.DegreeOfFreedom))
    g.add((dof, GMEOW.dofWork, EX.work))
    g.add((dof, GMEOW.dofParameter, GMEOW.musicalParameterDuration))
    g.add((dof, GMEOW.dofStatus, GMEOW.determinationConstrained))
    g.add(
        (
            dof,
            GMEOW.dofConstraintText,
            Literal("Total duration 4'33\".", lang="x-gmeow-english"),
        )
    )
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_degree_of_freedom_missing_parameter_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    dof = EX.dofBad
    g.add((dof, RDF.type, GMEOW.DegreeOfFreedom))
    g.add((dof, GMEOW.dofWork, EX.work))
    g.add((dof, GMEOW.dofStatus, GMEOW.determinationFree))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one MusicalParameter" in _error_text(result)


def test_degree_of_freedom_both_targets_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    dof = EX.dofBad
    g.add((dof, RDF.type, GMEOW.DegreeOfFreedom))
    g.add((dof, GMEOW.dofWork, EX.work))
    g.add((dof, GMEOW.dofExpression, EX.expression))
    g.add((dof, GMEOW.dofParameter, GMEOW.musicalParameterPitch))
    g.add((dof, GMEOW.dofStatus, GMEOW.determinationFree))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one Work or exactly one Expression" in _error_text(result)


def test_traversal_constraint_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tc = EX.constraintValid
    g.add((tc, RDF.type, GMEOW.TraversalConstraint))
    g.add((tc, GMEOW.constraintAppliesTo, EX.work))
    g.add(
        (
            tc,
            GMEOW.constraintText,
            Literal(
                "Choose any fragment; stop after three repeats.", lang="x-gmeow-english"
            ),
        )
    )
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_traversal_constraint_missing_text_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tc = EX.constraintBad
    g.add((tc, RDF.type, GMEOW.TraversalConstraint))
    g.add((tc, GMEOW.constraintAppliesTo, EX.work))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "at least one rule text" in _error_text(result)


def test_performance_decision_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    pd = EX.decisionValid
    g.add((pd, RDF.type, GMEOW.PerformanceDecision))
    g.add((pd, GMEOW.decisionPerformance, EX.performance))
    g.add((pd, GMEOW.decisionConstraint, EX.constraint))
    g.add((pd, GMEOW.decisionSequence, Literal("A → B → C")))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_performance_decision_missing_sequence_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    pd = EX.decisionBad
    g.add((pd, RDF.type, GMEOW.PerformanceDecision))
    g.add((pd, GMEOW.decisionPerformance, EX.performance))
    g.add((pd, GMEOW.decisionConstraint, EX.constraint))
    # decisionSequence intentionally omitted
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one traversal sequence" in _error_text(result)


def test_generative_process_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    proc = EX.processValid
    g.add((proc, RDF.type, GMEOW.GenerativeProcess))
    g.add((proc, GMEOW.processKind, GMEOW.generativeProcessKindPhasing))
    g.add(
        (
            proc,
            GMEOW.processRuleText,
            Literal(
                "Voice A and B begin in unison; B accelerates until one beat ahead.",
                lang="x-gmeow-english",
            ),
        )
    )
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_generative_process_missing_rule_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    proc = EX.processBad
    g.add((proc, RDF.type, GMEOW.GenerativeProcess))
    g.add((proc, GMEOW.processKind, GMEOW.generativeProcessKindStochastic))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "at least one rule text" in _error_text(result)


def test_ornament_profile_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    prof = EX.ornamentValid
    g.add((prof, RDF.type, GMEOW.OrnamentProfile))
    g.add((prof, GMEOW.ornamentProfileKind, GMEOW.ornamentProfileKindGamaka))
    g.add((prof, GMEOW.appliesToVoice, EX.voice))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_ornament_profile_missing_target_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    prof = EX.ornamentBad
    g.add((prof, RDF.type, GMEOW.OrnamentProfile))
    g.add((prof, GMEOW.ornamentProfileKind, GMEOW.ornamentProfileKindGamaka))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "at least one MusicalSegment or Voice" in _error_text(result)
