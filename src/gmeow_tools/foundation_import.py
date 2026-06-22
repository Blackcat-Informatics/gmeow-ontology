# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Foundation-corpus importer (#364, EPIC #358) — the consumer child.

Imports a Lillith_Foundation_Docs-shaped JSONL corpus into GMEOW instance
data, exercising every interior facility the two EPICs landed: WEMI spine,
claim-spine author facts, #353 Assessments for goal-score vectors, #359
narrative positions, #360 seam links (FLAT BY DEFAULT — the efficiency
doctrine, budget-reported), #361 arc samples, #362 scoped roles, #363
motifs, and provenance via ImportActivity.

DOCTRINE.
* THE EFFICIENCY DOCTRINE IS LOAD-BEARING: seam links (active characters,
  key events) emit as flat quads; only constructs whose vantage/score/mode
  is data (assessments, arc samples, roles, exemplars) reify. The
  BudgetReport records the split per link type; silent full reification is
  a defect (#360).
* TAGS ARE NOT PROMOTED: thematic_tags stay unimported (counted in the
  budget report as unpromoted) — motif promotion needs recurrence + curator
  confirmation (#363); explicit corpus concepts DO become Motifs.
* CLAIMS-SCOPING IS LAYERED: v1 emits the base graph; per-claim
  accordingTo/confidence cells are the statement layer's emission mode and
  arrive with the compiler-arc window (the alignment/projection sets parked
  in the child issues). The vantage-indexed constructs (samples,
  assessments) carry their vantage NATIVELY already.
* PRIVACY: the corpus is private source material. This module never embeds
  corpus content in the repo; CI runs against a SYNTHETIC fixture, and
  full-corpus runs are local, reporting aggregate numbers only.
* NOT a registry generator: registry artifacts derive from repo sources and
  are drift-gated; corpus-derived artifacts are external products written
  to a caller-chosen directory.
"""

from __future__ import annotations

import csv
import io
import json
import re
import unicodedata
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from xml.sax.saxutils import escape

from gmeow_rdf.compat.rdflib import RDF, RDFS, XSD, Graph, Literal, Namespace, URIRef

from gmeow_tools.config import NAMESPACE

GM = Namespace(NAMESPACE)
CORP = Namespace(NAMESPACE + "corpus/foundation/")
LANG = "x-gmeow-english"

# Corpus role strings → narrative-role seeds; unknown kinds mint open
# corpus-local NarrativeRole individuals (the vocabulary is open, P9).
ROLE_SEEDS = {
    "protagonist": GM.roleProtagonist,
    "antagonist": GM.roleAntagonist,
    "mentor": GM.roleMentor,
    "foil": GM.roleFoil,
    "narrator": GM.roleNarratingVoice,
    "confidant": GM.roleConfidant,
    "love interest": GM.roleLoveInterest,
    "trickster": GM.roleTrickster,
}


def _slug(text: str) -> str:
    norm = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", norm.lower()).strip("-") or "x"


@dataclass
class BudgetReport:
    """Flat-vs-reified statement budget (#360 made checkable)."""

    flat: Counter[str] = field(default_factory=Counter)
    reified: Counter[str] = field(default_factory=Counter)
    skipped: Counter[str] = field(default_factory=Counter)

    def as_text(self) -> str:
        """Render the human-readable budget table."""
        lines = ["FOUNDATION IMPORT BUDGET", "== flat links (1 quad each) =="]
        lines += [f"  {k}: {v}" for k, v in sorted(self.flat.items())]
        lines.append("== reified constructs (vantage/score/mode is data) ==")
        lines += [f"  {k}: {v}" for k, v in sorted(self.reified.items())]
        lines.append("== deliberately not imported (no silent caps) ==")
        lines += [f"  {k}: {v}" for k, v in sorted(self.skipped.items())]
        return "\n".join(lines)


def load_records(path: Path) -> list[dict[str, Any]]:
    """Read a JSONL corpus into a list of record dicts."""
    records = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records


class FoundationImporter:
    """One corpus → one graph + one budget report."""

    def __init__(self) -> None:
        """Create an empty graph, budget, and corpus-pipeline scaffolding ids."""
        self.graph = Graph()
        self.graph.bind("gmeow", GM)
        self.graph.bind("corp", CORP)
        self.budget = BudgetReport()
        self._pipeline = CORP["agent/corpus-pipeline"]
        self._rubric = CORP["rubric/principia-goals"]
        self._criteria: dict[str, URIRef] = {}

    # -- scaffolding ------------------------------------------------------- #

    def _scaffold(self, source_path: str) -> None:
        g = self.graph
        g.add((self._pipeline, RDF.type, GM.SoftwareAgent))
        g.add(
            (
                self._pipeline,
                RDFS.label,
                Literal("foundation corpus pipeline", lang=LANG),
            )
        )
        activity = CORP["activity/import"]
        g.add((activity, RDF.type, GM.ImportActivity))
        g.add((activity, GM.wasAssociatedWith, self._pipeline))
        # Basename only: raw local paths leak usernames/layout into shared
        # artifacts (PR #389 review).
        g.add(
            (
                activity,
                GM.sourceLocation,
                Literal(Path(source_path).name if source_path else ""),
            )
        )
        g.add((self._rubric, RDF.type, GM.Rubric))
        g.add((self._rubric, RDFS.label, Literal("principia goal rubric", lang=LANG)))
        g.add((self._rubric, GM.normIssuer, self._pipeline))
        scale = CORP["scale/unit"]
        g.add((scale, RDF.type, GM.ScoreScale))
        g.add((scale, GM.scaleMin, Literal("0.0", datatype=XSD.decimal)))
        g.add((scale, GM.scaleMax, Literal("1.0", datatype=XSD.decimal)))
        g.add((self._rubric, GM.usesScale, scale))

    def _narrated_event_type(self) -> URIRef:
        iri = CORP["event-type/narrated-event"]
        if (iri, RDF.type, GM.EventType) not in self.graph:
            self.graph.add((iri, RDF.type, GM.EventType))
            self.graph.add((iri, RDFS.label, Literal("narrated event", lang=LANG)))
        return iri

    def _criterion(self, goal_id: str) -> URIRef:
        if goal_id not in self._criteria:
            iri = CORP[f"criterion/{_slug(goal_id)}"]
            self.graph.add((iri, RDF.type, GM.Criterion))
            self.graph.add((iri, RDFS.label, Literal(goal_id, lang=LANG)))
            self.graph.add((self._rubric, GM.hasCriterion, iri))
            # Named poles are the rubric contract; the corpus carries only
            # ids, so poles are minted as placeholders pending the principia
            # importer's pole prose (EPIC #348 consumer).
            up = CORP[f"pole/{_slug(goal_id)}-embodiment"]
            down = CORP[f"pole/{_slug(goal_id)}-antithesis"]
            for pole, label in ((up, "embodiment"), (down, "antithesis")):
                self.graph.add((pole, RDF.type, GM.CriterionPole))
                self.graph.add(
                    (pole, RDFS.label, Literal(f"{goal_id} {label}", lang=LANG))
                )
            self.graph.add((iri, GM.rewardPole, up))
            self.graph.add((iri, GM.penaltyPole, down))
            self._criteria[goal_id] = iri
        return self._criteria[goal_id]

    # -- per-record mapping ------------------------------------------------ #

    def import_corpus(
        self, records: list[dict[str, Any]], source_path: str = ""
    ) -> Graph:
        """Map all records (sections first, then books) into the graph."""
        self._scaffold(source_path)
        for record in records:
            if record.get("type") == "section":
                self._import_section(record)
        for record in records:
            if record.get("type") == "book":
                self._import_book(record)
        return self.graph

    def _import_section(self, record: dict[str, Any]) -> None:
        g = self.graph
        iri = CORP[f"section/{_slug(record['section_id'])}"]
        g.add((iri, RDF.type, GM.SocialObject))
        g.add(
            (
                iri,
                RDFS.label,
                Literal(record.get("title", record["section_id"]), lang=LANG),
            )
        )

    def _character_iri(self, record: dict[str, Any], name: str) -> URIRef:
        """Work-scoped character IRI (the corpus .nq convention).

        Identically-named characters in different books are DIFFERENT nodes:
        cross-work identity is a coreference claim (counterpartOf /
        statement-layer mode), never string equality (PR #389 review).
        """
        book = record.get("book_number", _slug(record.get("title", "x")))
        return CORP[f"character/{book}/{_slug(name)}"]

    def _book_iri(self, record: dict[str, Any]) -> URIRef:
        return CORP[f"book/{record.get('book_number', _slug(record['title']))}"]

    def _import_book(self, record: dict[str, Any]) -> None:
        g = self.graph
        work = self._book_iri(record)
        expression = URIRef(str(work) + "/expression")
        release = URIRef(str(work) + "/release")
        title = record["title"]
        g.add((work, RDF.type, GM.Work))
        g.add((work, RDFS.label, Literal(title, lang=LANG)))
        g.add((expression, RDF.type, GM.Expression))
        g.add((expression, RDFS.label, Literal(f"{title} (text)", lang=LANG)))
        g.add((expression, GM.realizes, work))
        g.add((release, RDF.type, GM.BookRelease))
        g.add((release, RDFS.label, Literal(f"{title} (release)", lang=LANG)))
        g.add((release, GM.embodies, expression))
        if record.get("section_id"):
            g.add((work, GM.partOf, CORP[f"section/{_slug(record['section_id'])}"]))
        for author in (record.get("author_s_") or "").split(" and "):
            author = author.strip()
            if author:
                agent = CORP[f"person/{_slug(author)}"]
                g.add((agent, RDF.type, GM.Person))
                g.add((agent, RDFS.label, Literal(author, lang=LANG)))
                g.add((work, GM.hasContributor, agent))
                self.budget.flat["contributor"] += 1

        self._import_scores(record, work)
        frame, positions, segments = self._import_chapters(record, work, expression)
        characters = self._import_characters(record, work, frame, positions, segments)
        self._import_concepts(record, segments)
        self.budget.skipped["thematic_tags (unpromoted — #363 heuristic)"] += sum(
            len(ch.get("thematic_tags") or [])
            for ch in record.get("corpus_db_chapter_summaries") or []
        )
        del characters  # bound for clarity; all uses are inline above

    def _import_scores(self, record: dict[str, Any], work: URIRef) -> None:
        g = self.graph
        for goal_id, score in (record.get("corpus_db_primary_goals") or {}).items():
            if goal_id.startswith("_"):
                continue
            assessment = URIRef(str(work) + f"/score/{_slug(goal_id)}")
            g.add((assessment, RDF.type, GM.Assessment))
            g.add((assessment, GM.vantage, self._pipeline))
            g.add((assessment, GM.assessmentTarget, work))
            g.add((assessment, GM.assessmentCriterion, self._criterion(goal_id)))
            g.add((assessment, GM.assessmentRubric, self._rubric))
            g.add(
                (
                    assessment,
                    GM.assessmentScoreValue,
                    Literal(f"{float(score):.4f}", datatype=XSD.decimal),
                )
            )
            self.budget.reified["goal-score assessments (zeros are scores)"] += 1

    def _import_chapters(
        self, record: dict[str, Any], work: URIRef, expression: URIRef
    ) -> tuple[URIRef, dict[int, URIRef], dict[int, URIRef]]:
        g = self.graph
        frame = URIRef(str(work) + "/discourse-frame")
        g.add((frame, RDF.type, GM.NarrativeTimeFrame))
        g.add((frame, RDFS.label, Literal("discourse order", lang=LANG)))
        g.add((frame, GM.narrativeTimeAxis, GM.axisDiscourseTime))
        g.add((frame, GM.discourseTimeOf, work))
        g.add((frame, GM.frameRealm, GM.frameRealmNarrative))
        g.add((frame, GM.frameKind, GM.frameKindNarrative))
        g.add((frame, GM.hasAxis, URIRef(str(frame) + "/axis")))
        g.add((frame, GM.dimensionCount, Literal(1, datatype=XSD.nonNegativeInteger)))
        g.add((frame, GM.requiresHost, Literal(False)))
        g.add((frame, GM.determinacyModel, GM.determinacyCrisp))
        positions: dict[int, URIRef] = {}
        segments: dict[int, URIRef] = {}
        for chapter in record.get("corpus_db_chapter_summaries") or []:
            index = int(chapter["chapter_index"])
            pos = URIRef(str(frame) + f"/pos/{index}")
            g.add((pos, RDF.type, GM.NarrativePosition))
            g.add((pos, GM.positionFrame, frame))
            g.add((pos, GM.positionOrdinal, Literal(index)))
            if chapter.get("chapter_title"):
                g.add((pos, GM.positionLabel, Literal(chapter["chapter_title"])))
            segment = URIRef(str(work) + f"/chapter/{index}")
            g.add((segment, RDF.type, GM.ContentSegment))
            g.add(
                (
                    segment,
                    RDFS.label,
                    Literal(chapter.get("chapter_title", str(index)), lang=LANG),
                )
            )
            g.add((segment, GM.segmentOf, expression))
            g.add((segment, GM.atNarrativePosition, pos))
            positions[index] = pos
            segments[index] = segment
            for event_no, event_text in enumerate(chapter.get("key_events") or [], 1):
                event = URIRef(
                    str(segment) + f"/event/{event_no}-{_slug(event_text[:48])}"
                )
                g.add((event, RDF.type, GM.Event))
                g.add((event, RDFS.label, Literal(event_text[:120], lang=LANG)))
                g.add((event, GM.eventType, self._narrated_event_type()))
                g.add((segment, GM.narrates, event))
                self.budget.flat["narrates → key event"] += 1
            for name in chapter.get("active_characters") or []:
                g.add((segment, GM.narrates, self._character_iri(record, name)))
                self.budget.flat["narrates → active character"] += 1
        return frame, positions, segments

    def _import_characters(
        self,
        record: dict[str, Any],
        work: URIRef,
        frame: URIRef,
        positions: dict[int, URIRef],
        segments: dict[int, URIRef],
    ) -> dict[str, URIRef]:
        g = self.graph
        characters: dict[str, URIRef] = {}
        for char in record.get("corpus_db_characters") or []:
            iri = self._character_iri(record, char["name"])
            characters[char["name"]] = iri
            g.add((iri, RDF.type, GM.Person))
            g.add((iri, RDFS.label, Literal(char["name"], lang=LANG)))
            for index in char.get("chapter_appearances") or []:
                if int(index) in segments:
                    g.add((iri, GM.narratedIn, segments[int(index)]))
                    self.budget.flat["narratedIn ← appearance"] += 1
            role_text = (char.get("role") or "").strip().lower()
            if role_text:
                role_value = ROLE_SEEDS.get(role_text)
                if role_value is None:
                    role_value = CORP[f"role/{_slug(role_text)}"]
                    g.add((role_value, RDF.type, GM.NarrativeRole))
                    g.add((role_value, RDFS.label, Literal(role_text, lang=LANG)))
                claim = URIRef(str(iri) + f"/role-in/{_slug(str(work))[-24:]}")
                g.add((claim, RDF.type, GM.RoleInNarrative))
                g.add((claim, GM.narrativeRoleBearer, iri))
                g.add((claim, GM.narrativeRoleScope, work))
                g.add((claim, GM.narrativeRoleValue, role_value))
                self.budget.reified["role claims (scoped, interpretive)"] += 1
            for goal_id in char.get("exemplar_principia") or []:
                exemplar = URIRef(str(iri) + f"/exemplifies/{_slug(goal_id)}")
                g.add((exemplar, RDF.type, GM.Exemplar))
                g.add((exemplar, GM.citingEntity, self._rubric))
                g.add((exemplar, GM.citedEntity, work))
                g.add((exemplar, GM.citationIntent, GM.intentSupports))
                g.add((exemplar, GM.exemplarSubject, iri))
                g.add((exemplar, GM.exemplarPolarity, GM.polarityPositive))
                rationale = char.get("exemplar_rationale")
                if rationale:
                    g.add(
                        (exemplar, GM.exemplarRationale, Literal(rationale, lang=LANG))
                    )
                g.add(
                    (
                        self._criterion(goal_id),
                        GM.hasScoreAnchor,
                        self._anchor(goal_id, exemplar),
                    )
                )
                self.budget.reified[
                    "entity exemplars (exemplarSubject, #353/#362)"
                ] += 1
        self._import_arcs(record, characters, frame, positions)
        return characters

    def _anchor(self, goal_id: str, exemplar: URIRef) -> URIRef:
        g = self.graph
        anchor = URIRef(str(exemplar) + "/anchor")
        g.add((anchor, RDF.type, GM.ScoreAnchor))
        g.add((anchor, GM.anchorRangeMin, Literal("0.8", datatype=XSD.decimal)))
        g.add((anchor, GM.anchorRangeMax, Literal("1.0", datatype=XSD.decimal)))
        g.add(
            (
                anchor,
                GM.anchorMeaning,
                Literal(f"Conduct embodying {goal_id} across the work.", lang=LANG),
            )
        )
        g.add((anchor, GM.anchorExemplar, exemplar))
        return anchor

    def _import_arcs(
        self,
        record: dict[str, Any],
        characters: dict[str, URIRef],
        frame: URIRef,
        positions: dict[int, URIRef],
    ) -> None:
        g = self.graph
        del frame  # samples anchor to positions; the frame rides on them
        for chapter in record.get("corpus_db_chapter_summaries") or []:
            index = int(chapter["chapter_index"])
            if index not in positions:
                continue
            for entry in chapter.get("character_arcs") or []:
                name = entry.get("character_name")
                state_text = (entry.get("emotional_state") or "").strip()
                if not name or not state_text:
                    self.budget.skipped["arc entries without state"] += 1
                    continue
                subject = characters.get(name) or self._character_iri(record, name)
                # One cell, one state: a second reading at the same position
                # (duplicate corpus rows, blended states) is a SIBLING sample,
                # so the IRI carries the state slug (#361 doctrine).
                sample = URIRef(str(subject) + f"/sample/{index}/{_slug(state_text)}")
                state = CORP[f"emotion/{_slug(state_text)}"]
                g.add((state, RDF.type, GM.EmotionType))
                g.add((state, RDFS.label, Literal(state_text, lang=LANG)))
                g.add((sample, RDF.type, GM.ArcSample))
                g.add((sample, GM.vantage, self._pipeline))
                g.add((sample, GM.sampleSubject, subject))
                g.add((sample, GM.samplePosition, positions[index]))
                g.add((sample, GM.sampleState, state))
                for signal in entry.get("development_signals") or []:
                    g.add(
                        (sample, GM.developmentSignalText, Literal(signal, lang=LANG))
                    )
                self.budget.reified["arc samples (vantage is data)"] += 1

    def _import_concepts(
        self, record: dict[str, Any], segments: dict[int, URIRef]
    ) -> None:
        g = self.graph
        for concept in record.get("corpus_db_concepts") or []:
            motif = CORP[f"motif/{_slug(concept['name'])}"]
            g.add((motif, RDF.type, GM.Motif))
            g.add((motif, RDFS.label, Literal(concept["name"], lang=LANG)))
            g.add((motif, GM.motifKind, GM.motifKindTheme))
            for index in concept.get("chapter_appearances") or []:
                if int(index) in segments:
                    g.add((motif, GM.motifOccursIn, segments[int(index)]))
                    self.budget.flat["motifOccursIn ← concept appearance"] += 1


def reconcile_nq(nq_path: Path, mapped: dict[str, str]) -> str:
    """Coverage table: every predicate in the .nq form mapped to its status.

    The .nq is A form, not THE form — this report proves nothing was
    silently dropped (mapped / deliberately-dropped-with-reason / improved).
    """
    counts: Counter[str] = Counter()
    with nq_path.open(encoding="utf-8") as handle:
        for line in handle:
            parts = line.split(maxsplit=3)
            if len(parts) > 2 and parts[1].startswith("<"):
                counts[parts[1].strip("<>")] += 1
    lines = ["NQ RECONCILIATION (predicate → status)"]
    for predicate, count in counts.most_common():
        status = mapped.get(predicate, "UNREVIEWED")
        lines.append(f"  {predicate} ({count}): {status}")
    return "\n".join(lines)


# --------------------------------------------------------------------------- #
# Projections — each lossy, each with its loss named in the emitter docstring.
# --------------------------------------------------------------------------- #


def project_dracor_csv(graph: Graph) -> str:
    """DraCor-style co-occurrence edges, one row per character pair.

    DECLARED LOSS: frames, vantage, and event co-occurrents drop.
    """
    pairs: Counter[tuple[str, str]] = Counter()
    for segment in set(graph.subjects(GM.narrates, None)) | set(
        graph.objects(None, GM.narratedIn)
    ):
        members = sorted(
            str(c)
            for c in set(graph.objects(segment, GM.narrates))
            | set(graph.subjects(GM.narratedIn, segment))
            if (c, RDF.type, GM.Person) in graph
        )
        for i, a in enumerate(members):
            for b in members[i + 1 :]:
                pairs[(a, b)] += 1
    out = io.StringIO()
    writer = csv.writer(out)
    writer.writerow(["Source", "Target", "Weight"])
    for (a, b), weight in sorted(pairs.items()):
        writer.writerow([a, b, weight])
    return out.getvalue()


def project_syuzhet_csv(graph: Graph) -> str:
    """Syuzhet-style trajectory rows (subject,vantage,ordinal,state).

    DECLARED LOSS: states flatten to labels; no valence scalar is invented.
    """
    rows = []
    for sample in graph.subjects(RDF.type, GM.ArcSample):
        pos = graph.value(sample, GM.samplePosition)
        ordinal = graph.value(pos, GM.positionOrdinal) if pos else None
        state = graph.value(sample, GM.sampleState)
        label = graph.value(state, RDFS.label) if state else None
        rows.append(
            (
                str(graph.value(sample, GM.sampleSubject)),
                str(graph.value(sample, GM.vantage)),
                int(ordinal.toPython()) if isinstance(ordinal, Literal) else -1,
                str(label or state),
            )
        )
    out = io.StringIO()
    writer = csv.writer(out)
    writer.writerow(["subject", "vantage", "ordinal", "state"])
    for row in sorted(rows):
        writer.writerow(row)
    return out.getvalue()


def project_schema_jsonld(graph: Graph) -> str:
    """schema.org Book JSON-LD.

    DECLARED LOSS: WEMI tiers collapse to one Book node; scores, arcs,
    and frames drop entirely.
    """
    books = []
    for work in graph.subjects(RDF.type, GM.Work):
        label = graph.value(work, RDFS.label)
        authors = [
            str(graph.value(a, RDFS.label) or a)
            for a in graph.objects(work, GM.hasContributor)
        ]
        entry: dict[str, Any] = {
            "@type": "Book",
            "@id": str(work),
            "name": str(label or work),
        }
        if authors:
            entry["author"] = [
                {"@type": "Person", "name": name} for name in sorted(authors)
            ]
        books.append(entry)
    doc = {
        "@context": "https://schema.org",
        "@graph": sorted(books, key=lambda b: b["@id"]),
    }
    return json.dumps(doc, indent=2, ensure_ascii=False)


def project_tei_xml(graph: Graph) -> str:
    """TEI skeleton: castList of persons + chapter div per segment.

    DECLARED LOSS: positions flatten to div order; roles/arcs drop.
    """
    narrated = set(graph.objects(None, GM.narrates)) | set(
        graph.subjects(GM.narratedIn, None)
    )
    people = sorted(
        str(graph.value(p, RDFS.label) or p)
        for p in graph.subjects(RDF.type, GM.Person)
        if p in narrated  # cast = narrative characters; authors stay out
    )
    segments = []
    for s in graph.subjects(RDF.type, GM.ContentSegment):
        pos = graph.value(s, GM.atNarrativePosition)
        ordinal = graph.value(pos, GM.positionOrdinal) if pos else None
        n = int(ordinal.toPython()) if isinstance(ordinal, Literal) else -1
        segments.append((n, str(graph.value(s, RDFS.label) or s)))
    segments.sort()
    cast = "".join(f"<castItem><role>{escape(p)}</role></castItem>" for p in people)
    divs = "".join(
        f'<div type="chapter" n="{n}"><head>{escape(t)}</head></div>'
        for n, t in segments
    )
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<TEI xmlns="http://www.tei-c.org/ns/1.0"><teiHeader/>'
        f"<text><front><castList>{cast}</castList></front><body>{divs}</body></text></TEI>"
    )


def project_web_annotation_jsonld(graph: Graph) -> str:
    """Web Annotation: each flat narrates link as an oa:Annotation.

    DECLARED LOSS: promoted NarrationUsage modes flatten to bodies.
    """
    annotations = []
    for segment, target in sorted(graph.subject_objects(GM.narrates)):
        annotations.append(
            {
                "@type": "Annotation",
                "motivation": "describing",
                "target": str(segment),
                "body": str(target),
            }
        )
    doc = {"@context": "http://www.w3.org/ns/anno.jsonld", "@graph": annotations}
    return json.dumps(doc, indent=2, ensure_ascii=False)


def project_training_manifest_jsonl(graph: Graph) -> str:
    """Training-corpus manifest: one record per (work, criterion) score.

    Provenance-carrying — the lbox SFT/DPO loop's input shape. DECLARED
    LOSS: none at the score level; chunk pairing happens downstream.
    """
    lines = []
    for assessment in graph.subjects(RDF.type, GM.Assessment):
        target = graph.value(assessment, GM.assessmentTarget)
        criterion = graph.value(assessment, GM.assessmentCriterion)
        record = {
            "work": str(target),
            "work_title": str(graph.value(target, RDFS.label) or ""),
            "criterion": str(graph.value(criterion, RDFS.label) or criterion),
            "score": float(str(graph.value(assessment, GM.assessmentScoreValue))),
            "vantage": str(graph.value(assessment, GM.vantage)),
            "assessment": str(assessment),
        }
        lines.append(json.dumps(record, ensure_ascii=False, sort_keys=True))
    return "\n".join(sorted(lines)) + "\n"


PROJECTIONS = {
    "dracor.csv": project_dracor_csv,
    "syuzhet.csv": project_syuzhet_csv,
    "schema-org.jsonld": project_schema_jsonld,
    "tei.xml": project_tei_xml,
    "web-annotation.jsonld": project_web_annotation_jsonld,
    "training-manifest.jsonl": project_training_manifest_jsonl,
}


def run_import(
    jsonl_path: Path, out_dir: Path, nq_path: Path | None = None
) -> tuple[Graph, BudgetReport]:
    """Run the full pipeline into ``out_dir``.

    Import, then write graph + budget + projections (+ optional .nq
    reconciliation).
    """
    importer = FoundationImporter()
    records = load_records(jsonl_path)
    graph = importer.import_corpus(records, source_path=str(jsonl_path))
    out_dir.mkdir(parents=True, exist_ok=True)
    graph.serialize(out_dir / "foundation.ttl", format="turtle")
    (out_dir / "budget-report.txt").write_text(
        importer.budget.as_text() + "\n", encoding="utf-8"
    )
    for name, emitter in PROJECTIONS.items():
        (out_dir / name).write_text(emitter(graph), encoding="utf-8")
    if nq_path is not None and nq_path.exists():
        (out_dir / "nq-reconciliation.txt").write_text(
            reconcile_nq(nq_path, NQ_PREDICATE_STATUS) + "\n", encoding="utf-8"
        )
    return graph, importer.budget


# The .nq form's predicates, each accounted for (the no-silent-drop table).
NQ_PREDICATE_STATUS = {
    "http://lillith.internal/principia/active_character": (
        "MAPPED → flat gmeow:narrates (#360)"
    ),
    "http://lillith.internal/principia/key_event": (
        "MAPPED → gmeow:Event + flat gmeow:narrates (#360)"
    ),
    "http://lillith.internal/principia/goal_score": (
        "IMPROVED → gmeow:Assessment with vantage/rubric/criterion (#353)"
    ),
    "http://lillith.internal/principia/thematic_tag": (
        "DROPPED-WITH-REASON → unpromoted; #363 heuristic needs curator "
        "confirmation (budget-reported)"
    ),
    "http://lillith.internal/principia/emotional_state": (
        "IMPROVED → gmeow:ArcSample with vantage + frame-carried position (#361)"
    ),
    "http://lillith.internal/principia/arc_position": (
        "IMPROVED → gmeow:NarrativePosition in a discourse frame (#359)"
    ),
    "http://lillith.internal/principia/content_mode": (
        "DEFERRED → statement-layer emission mode (compiler-arc window)"
    ),
    "http://lillith.internal/principia/chapter_index": (
        "IMPROVED → gmeow:positionOrdinal on a frame-carried position (#359)"
    ),
    "http://lillith.internal/principia/predicate/exemplifies": (
        "IMPROVED → gmeow:Exemplar with exemplarSubject + polarity + anchor (#353/#362)"
    ),
    "http://lillith.internal/principia/predicate/rationale": (
        "MAPPED → gmeow:exemplarRationale"
    ),
    "http://lillith.internal/principia/predicate/relationship": (
        "DEFERRED → relator extraction from prose blobs (source-data "
        "deficiency; statement-layer mode)"
    ),
    "http://lillith.internal/principia/motivation": (
        "DEFERRED → #350 Goal extraction from prose (statement-layer mode)"
    ),
    "http://lillith.internal/principia/emotional_arc": (
        "IMPROVED → sampled trajectory (whole-arc prose retained at "
        "CharacterArc level in the statement-layer mode)"
    ),
    "http://lillith.internal/principia/predicate/character_role": (
        "IMPROVED → gmeow:RoleInNarrative (scoped, interpretive) (#362)"
    ),
    "http://lillith.internal/principia/penalty_boundary": (
        "DEFERRED → anti-score-anchor import with the principia importer "
        "(EPIC #348 consumer)"
    ),
    "http://lillith.internal/principia/predicate/paradigm_assignment": (
        "DEFERRED → #355 persona import with the principia importer"
    ),
}
