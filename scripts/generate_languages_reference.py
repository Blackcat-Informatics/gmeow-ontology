#!/usr/bin/env python3
"""Generate imports/languages-reference.ttl from ISO 639-3 + Wikidata + overrides.

This is a build-time helper, not runtime code. It fetches the SIL ISO 639-3 code
table, queries Wikidata for QIDs and Glottolog IDs, merges a small hand-curated
override table, emits a reviewable TSV seed, and appends missing languages to the
canonical Turtle file.

The existing Turtle file is treated as authoritative: existing individuals are
preserved verbatim and the script only adds languages/writing systems that are
not already present.

Principles:
- Principle 4: the .ttl file is the canonical source; this script only extends it.
- Principle 5: bridge by reference to Wikidata, Lexvo, and Glottolog.
- Principle 8: every term carries label, definition, and isDefinedBy.
- Principle 9: co-equal scripts and names, no "primary".

Usage:
    uv run python scripts/generate_languages_reference.py
"""

from __future__ import annotations

import argparse
import csv
import logging
import re
from dataclasses import dataclass
from pathlib import Path

import httpx
from rdflib import RDF, Graph, Literal, URIRef

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
_log = logging.getLogger(__name__)

PROJECT_ROOT = Path(__file__).resolve().parents[1]
IMPORTS_DIR = PROJECT_ROOT / "imports"
SCRIPTS_DIR = PROJECT_ROOT / "scripts"
SEED_PATH = SCRIPTS_DIR / "languages_reference_seed.tsv"
OVERRIDES_PATH = SCRIPTS_DIR / "languages_reference_overrides.tsv"
OUTPUT_PATH = IMPORTS_DIR / "languages-reference.ttl"

GMEOW_NS = "https://blackcatinformatics.ca/gmeow/"
WD_NS = "http://www.wikidata.org/entity/"

SIL_ISO6393_URL = (
    "https://iso639-3.sil.org/sites/iso639-3/files/downloads/iso-639-3.tab"
)
WIKIDATA_SPARQL_URL = "https://query.wikidata.org/sparql"

#: ISO 15924 script code -> (label, type_iri_local, direction_iri_local, wikidata_qid)
WRITING_SYSTEMS: dict[str, tuple[str, str, str, str]] = {
    "Latn": ("Latin", "wsTypeAlphabet", "directionLtr", "Q8229"),
    "Hani": ("Han", "wsTypeLogographic", "directionLtr", "Q8201"),
    "Hira": ("Hiragana", "wsTypeSyllabary", "directionLtr", "Q48332"),
    "Kana": ("Katakana", "wsTypeSyllabary", "directionLtr", "Q48334"),
    "Hang": ("Hangul", "wsTypeFeatural", "directionLtr", "Q8222"),
    "Arab": ("Arabic", "wsTypeAbjad", "directionRtl", "Q1828555"),
    "Cyrl": ("Cyrillic", "wsTypeAlphabet", "directionLtr", "Q8209"),
    "Deva": ("Devanagari", "wsTypeAbugida", "directionLtr", "Q15780"),
    "Grek": ("Greek", "wsTypeAlphabet", "directionLtr", "Q8216"),
    "Hebr": ("Hebrew", "wsTypeAbjad", "directionRtl", "Q14318"),
    "Beng": ("Bengali", "wsTypeAbugida", "directionLtr", "Q33007"),
    "Taml": ("Tamil", "wsTypeAbugida", "directionLtr", "Q15774"),
    "Thai": ("Thai", "wsTypeAbugida", "directionLtr", "Q160064"),
    "Geor": ("Georgian", "wsTypeAlphabet", "directionLtr", "Q8199"),
    "Armn": ("Armenian", "wsTypeAlphabet", "directionLtr", "Q8207"),
    "Ethi": ("Ethiopic", "wsTypeAbugida", "directionLtr", "Q8210"),
    "Gujr": ("Gujarati", "wsTypeAbugida", "directionLtr", "Q200109"),
    "Knda": ("Kannada", "wsTypeAbugida", "directionLtr", "Q318030"),
    "Mlym": ("Malayalam", "wsTypeAbugida", "directionLtr", "Q1316787"),
    "Mymr": ("Myanmar", "wsTypeAbugida", "directionLtr", "Q193631"),
    "Khmr": ("Khmer", "wsTypeAbugida", "directionLtr", "Q187586"),
    "Laoo": ("Lao", "wsTypeAbugida", "directionLtr", "Q13266"),
    "Tibt": ("Tibetan", "wsTypeAbugida", "directionLtr", "Q244699"),
    "Mong": ("Mongolian", "wsTypeAlphabet", "directionLtr", "Q1048139"),
    "Sinh": ("Sinhala", "wsTypeAbugida", "directionLtr", "Q34000"),
    "Telu": ("Telugu", "wsTypeAbugida", "directionLtr", "Q13359"),
    "Guru": ("Gurmukhi", "wsTypeAbugida", "directionLtr", "Q173285"),
    "Orya": ("Odia", "wsTypeAbugida", "directionLtr", "Q34234"),
    "Cher": ("Cherokee", "wsTypeSyllabary", "directionLtr", "Q26549"),
    "Cans": ("Canadian Aboriginal", "wsTypeSyllabary", "directionLtr", "Q2813094"),
    "Bopo": ("Bopomofo", "wsTypeSyllabary", "directionLtr", "Q58524"),
    "Brai": ("Braille", "wsTypeNonLinear", "directionLtr", "Q79894"),
    "Java": ("Javanese", "wsTypeAbugida", "directionLtr", "Q1818037"),
    "Sund": ("Sundanese", "wsTypeAbugida", "directionLtr", "Q2528564"),
    "Yiii": ("Yi", "wsTypeSyllabary", "directionLtr", "Q132965"),
    "Modi": ("Modi", "wsTypeAbugida", "directionLtr", "Q1073727"),
    "Brah": ("Brahmi", "wsTypeAbugida", "directionLtr", "Q173224"),
    "Khar": ("Kharoshthi", "wsTypeAbugida", "directionRtl", "Q269964"),
    "Glag": ("Glagolitic", "wsTypeAlphabet", "directionLtr", "Q155836"),
    "Thaa": ("Thaana", "wsTypeAbjad", "directionRtl", "Q2605130"),
    "Olck": ("Ol Chiki", "wsTypeAlphabet", "directionLtr", "Q2601410"),
    "Hans": ("Simplified Han", "wsTypeLogographic", "directionLtr", "Q13413878"),
    "Hant": ("Traditional Han", "wsTypeLogographic", "directionLtr", "Q13413880"),
    "Jpan": ("Japanese", "wsTypeMixed", "directionLtr", "Q5287"),
    "Kore": ("Korean", "wsTypeMixed", "directionLtr", "Q9176"),
}

#: Default script codes for ISO 639-1 languages.  Unknown codes fall back to Latn.
DEFAULT_SCRIPTS: dict[str, list[str]] = {
    "ab": ["Cyrl"],
    "aa": ["Latn"],
    "af": ["Latn"],
    "ak": ["Latn"],
    "sq": ["Latn"],
    "am": ["Ethi"],
    "ar": ["Arab"],
    "an": ["Latn"],
    "hy": ["Armn"],
    "as": ["Beng"],
    "av": ["Cyrl"],
    "ae": ["Latn"],
    "ay": ["Latn"],
    "az": ["Latn"],
    "bm": ["Latn"],
    "ba": ["Cyrl"],
    "eu": ["Latn"],
    "be": ["Cyrl"],
    "bn": ["Beng"],
    "bh": ["Deva"],
    "bi": ["Latn"],
    "bs": ["Latn"],
    "br": ["Latn"],
    "bg": ["Cyrl"],
    "my": ["Mymr"],
    "ca": ["Latn"],
    "ch": ["Latn"],
    "ce": ["Cyrl"],
    "ny": ["Latn"],
    "zh": ["Hani"],
    "cv": ["Cyrl"],
    "kw": ["Latn"],
    "co": ["Latn"],
    "cr": ["Cans"],
    "hr": ["Latn"],
    "cs": ["Latn"],
    "da": ["Latn"],
    "dv": ["Thaa"],
    "nl": ["Latn"],
    "dz": ["Tibt"],
    "en": ["Latn"],
    "eo": ["Latn"],
    "et": ["Latn"],
    "ee": ["Latn"],
    "fo": ["Latn"],
    "fj": ["Latn"],
    "fi": ["Latn"],
    "fr": ["Latn"],
    "ff": ["Latn"],
    "gl": ["Latn"],
    "ka": ["Geor"],
    "de": ["Latn"],
    "el": ["Grek"],
    "gn": ["Latn"],
    "gu": ["Gujr"],
    "ht": ["Latn"],
    "ha": ["Latn"],
    "he": ["Hebr"],
    "hz": ["Latn"],
    "hi": ["Deva"],
    "ho": ["Latn"],
    "hu": ["Latn"],
    "ia": ["Latn"],
    "id": ["Latn"],
    "ie": ["Latn"],
    "ga": ["Latn"],
    "ig": ["Latn"],
    "ik": ["Latn"],
    "io": ["Latn"],
    "is": ["Latn"],
    "it": ["Latn"],
    "iu": ["Cans"],
    "ja": ["Jpan"],
    "jv": ["Latn", "Java"],
    "kl": ["Latn"],
    "kn": ["Knda"],
    "kr": ["Latn"],
    "ks": ["Arab", "Deva"],
    "kk": ["Cyrl"],
    "km": ["Khmr"],
    "ki": ["Latn"],
    "rw": ["Latn"],
    "ky": ["Cyrl"],
    "kv": ["Cyrl"],
    "kg": ["Latn"],
    "ko": ["Kore"],
    "ku": ["Latn"],
    "kj": ["Latn"],
    "la": ["Latn"],
    "lb": ["Latn"],
    "lg": ["Latn"],
    "li": ["Latn"],
    "ln": ["Latn"],
    "lo": ["Laoo"],
    "lt": ["Latn"],
    "lu": ["Latn"],
    "lv": ["Latn"],
    "gv": ["Latn"],
    "mk": ["Cyrl"],
    "mg": ["Latn"],
    "ms": ["Latn"],
    "ml": ["Mlym"],
    "mt": ["Latn"],
    "mi": ["Latn"],
    "mr": ["Deva"],
    "mh": ["Latn"],
    "mn": ["Cyrl"],
    "na": ["Latn"],
    "nv": ["Latn"],
    "nd": ["Latn"],
    "ne": ["Deva"],
    "ng": ["Latn"],
    "nb": ["Latn"],
    "nn": ["Latn"],
    "no": ["Latn"],
    "ii": ["Yiii"],
    "nr": ["Latn"],
    "oc": ["Latn"],
    "oj": ["Cans"],
    "cu": ["Cyrl"],
    "om": ["Latn"],
    "or": ["Orya"],
    "os": ["Cyrl"],
    "pa": ["Guru", "Arab"],
    "fa": ["Arab"],
    "pi": ["Deva"],
    "pl": ["Latn"],
    "ps": ["Arab"],
    "pt": ["Latn"],
    "qu": ["Latn"],
    "rm": ["Latn"],
    "rn": ["Latn"],
    "ro": ["Latn"],
    "ru": ["Cyrl"],
    "sa": ["Deva"],
    "sc": ["Latn"],
    "sd": ["Arab", "Deva"],
    "se": ["Latn"],
    "sm": ["Latn"],
    "sg": ["Latn"],
    "sr": ["Cyrl"],
    "gd": ["Latn"],
    "sn": ["Latn"],
    "si": ["Sinh"],
    "sk": ["Latn"],
    "sl": ["Latn"],
    "so": ["Latn"],
    "st": ["Latn"],
    "es": ["Latn"],
    "su": ["Latn", "Sund"],
    "sw": ["Latn"],
    "ss": ["Latn"],
    "sv": ["Latn"],
    "ta": ["Taml"],
    "te": ["Telu"],
    "tg": ["Cyrl"],
    "th": ["Thai"],
    "ti": ["Ethi"],
    "bo": ["Tibt"],
    "tk": ["Latn"],
    "tl": ["Latn"],
    "tn": ["Latn"],
    "to": ["Latn"],
    "tr": ["Latn"],
    "ts": ["Latn"],
    "tt": ["Cyrl"],
    "tw": ["Latn"],
    "ty": ["Latn"],
    "ug": ["Arab"],
    "uk": ["Cyrl"],
    "ur": ["Arab"],
    "uz": ["Latn"],
    "ve": ["Latn"],
    "vi": ["Latn"],
    "vo": ["Latn"],
    "wa": ["Latn"],
    "cy": ["Latn"],
    "wo": ["Latn"],
    "fy": ["Latn"],
    "xh": ["Latn"],
    "yi": ["Hebr"],
    "yo": ["Latn"],
    "za": ["Latn"],
    "zu": ["Latn"],
}


def _title_case(s: str) -> str:
    """Return a title-cased English name suitable for rdfs:label.

    SIL reference names are already correctly capitalized for proper nouns; only
    ensure the first character is uppercase.
    """
    return s[0].upper() + s[1:] if s else s


def _is_enabled_flag(value: str | None) -> bool:
    """Return whether a TSV flag value means enabled.

    Only the literal value ``1`` is treated as enabled. Empty/missing values
    are treated as disabled so that an override row must explicitly opt in.
    """
    return (value or "").strip() == "1"


def _iri_suffix(label: str) -> str:
    """Create a safe IRI local name suffix from a label."""
    return re.sub(r"[^A-Za-z0-9]", "", label)


def _internal_tag(label: str) -> str:
    """Return an x-gmeow-* internal tag from a label."""
    parts = re.split(r"[^A-Za-z0-9]+", label.lower())
    return "x-gmeow-" + "".join(parts)


def _escape_turtle_string(s: str) -> str:
    """Escape a string for Turtle double-quoted literal."""
    return s.replace("\\", "\\\\").replace('"', '\\"')


def _definition_from_label(label: str) -> str:
    """Generate a concise definition from the English label."""
    return f"A language known as {label}."


def _strip_field(value: str | None) -> str:
    """Return a stripped string, or an empty string if value is None."""
    return value.strip() if value is not None else ""


def _optional_strip(value: str | None) -> str | None:
    """Return a stripped string, or None if value is None or empty."""
    if value is None:
        return None
    stripped = value.strip()
    return stripped if stripped else None


@dataclass
class ExistingLanguage:
    """Data extracted from an existing gmeow:Language individual."""

    iri: URIRef
    iso639_1: str | None
    iso639_3: str | None
    internal_tag: str | None


@dataclass
class LanguageSeed:
    """A language row produced by this generator."""

    iso639_1: str | None
    iso639_3: str
    label: str
    definition: str
    internal_tag: str
    bcp47_tag: str
    origin: str
    modality: str
    status: str
    script_codes: list[str]
    wikidata_qid: str | None
    glottolog_id: str | None
    endonym: str | None

    @property
    def iri_suffix(self) -> str:
        """Return the IRI local-name suffix for this language."""
        return _iri_suffix(self.label)


@dataclass
class WritingSystemSeed:
    """A writing-system row produced by this generator."""

    code: str
    label: str
    ws_type: str
    direction: str
    wikidata_qid: str


def fetch_iso6393_table() -> list[dict[str, str]]:
    """Download and parse the SIL ISO 639-3 code table."""
    _log.info("Fetching ISO 639-3 table from %s", SIL_ISO6393_URL)
    response = httpx.get(SIL_ISO6393_URL, timeout=60.0)
    response.raise_for_status()
    lines = response.text.splitlines()
    if not lines:
        return []
    reader = csv.DictReader(lines, delimiter="\t")
    return [row for row in reader if row.get("Id")]


def query_wikidata(
    client: httpx.Client,
    iso639_3_ids: list[str],
    *,
    fail_on_error: bool = False,
) -> dict[str, dict[str, str]]:
    """Return mapping iso639_3 -> {qid, glottolog_id}."""
    if not iso639_3_ids:
        raise ValueError("iso639_3_ids list cannot be empty")

    values = " ".join(f'"{code}"' for code in iso639_3_ids)
    query = f"""
    SELECT ?iso639_3 ?item ?glottologId WHERE {{
      VALUES ?iso639_3 {{ {values} }}
      ?item wdt:P220 ?iso639_3 .
      OPTIONAL {{ ?item wdt:P1394 ?glottologId . }}
    }}
    """

    result: dict[str, dict[str, str]] = {}
    try:
        response = client.get(
            WIKIDATA_SPARQL_URL,
            params={"query": query, "format": "json"},
            headers={
                "Accept": "application/sparql-results+json",
                "User-Agent": "gmeow-tools/0.1 (language catalog generator)",
            },
        )
        response.raise_for_status()
        data = response.json()
    except (httpx.HTTPError, ValueError) as exc:
        if fail_on_error:
            raise RuntimeError("Wikidata SPARQL query failed") from exc
        _log.warning("Wikidata SPARQL query failed: %s", exc)
        return result

    bindings = data.get("results", {}).get("bindings", [])
    for binding in bindings:
        iso = binding.get("iso639_3", {}).get("value", "")
        if not iso:
            continue
        entry = result.setdefault(iso, {})
        item = binding.get("item", {}).get("value", "")
        if item and "Q" in item:
            entry.setdefault("qid", item.split("/")[-1])
        glotto = binding.get("glottologId", {}).get("value", "")
        if glotto:
            entry.setdefault("glottolog_id", glotto)
    return result


def load_overrides() -> dict[str, dict[str, str]]:
    """Load per-language override rows keyed by ISO 639-3."""
    overrides: dict[str, dict[str, str]] = {}
    if not OVERRIDES_PATH.exists():
        return overrides
    with OVERRIDES_PATH.open("r", encoding="utf-8") as fh:
        reader = csv.DictReader(fh, delimiter="\t")
        for row in reader:
            iso3 = _strip_field(row.get("iso639_3"))
            if iso3:
                overrides[iso3] = {
                    k: v.strip()
                    for k, v in row.items()
                    if isinstance(v, str) and v.strip()
                }
    return overrides


def load_seed() -> list[LanguageSeed]:
    """Read languages from the committed seed TSV."""
    languages: list[LanguageSeed] = []
    if not SEED_PATH.exists():
        return languages
    with SEED_PATH.open("r", encoding="utf-8") as fh:
        reader = csv.DictReader(fh, delimiter="\t")
        for row in reader:
            iso3 = _strip_field(row.get("iso639_3"))
            if not iso3:
                continue
            languages.append(
                LanguageSeed(
                    iso639_1=_optional_strip(row.get("iso639_1")),
                    iso639_3=iso3,
                    label=_strip_field(row.get("label")),
                    definition=_strip_field(row.get("definition")),
                    internal_tag=_strip_field(row.get("internal_tag")),
                    bcp47_tag=_strip_field(row.get("bcp47_tag")),
                    origin=row.get("origin") or "originNatural",
                    modality=row.get("modality") or "modalitySpoken, modalityWritten",
                    status=row.get("status") or "statusLiving",
                    script_codes=[
                        c.strip()
                        for c in (row.get("script_codes") or "").split(",")
                        if c.strip()
                    ],
                    wikidata_qid=_optional_strip(row.get("wikidata_qid")),
                    glottolog_id=_optional_strip(row.get("glottolog_id")),
                    endonym=_optional_strip(row.get("endonym")),
                )
            )
    return languages


def save_seed(languages: list[LanguageSeed]) -> None:
    """Write the reviewable TSV seed."""
    fields = [
        "iso639_1",
        "iso639_3",
        "label",
        "definition",
        "internal_tag",
        "bcp47_tag",
        "origin",
        "modality",
        "status",
        "script_codes",
        "wikidata_qid",
        "glottolog_id",
        "endonym",
    ]
    with SEED_PATH.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.DictWriter(
            fh, fieldnames=fields, delimiter="\t", lineterminator="\n"
        )
        writer.writeheader()
        for lang in languages:
            writer.writerow(
                {
                    "iso639_1": lang.iso639_1 or "",
                    "iso639_3": lang.iso639_3,
                    "label": lang.label,
                    "definition": lang.definition,
                    "internal_tag": lang.internal_tag,
                    "bcp47_tag": lang.bcp47_tag,
                    "origin": lang.origin,
                    "modality": lang.modality,
                    "status": lang.status,
                    "script_codes": ",".join(lang.script_codes),
                    "wikidata_qid": lang.wikidata_qid or "",
                    "glottolog_id": lang.glottolog_id or "",
                    "endonym": lang.endonym or "",
                }
            )


def parse_existing_catalog() -> tuple[
    dict[str, ExistingLanguage],
    dict[str, ExistingLanguage],
    set[str],
    dict[str, str],
]:
    """Return existing languages keyed by ISO code and existing script codes.

    The returned ``script_iri_by_code`` maps an ISO 15924 script code to the
    local name of the existing ``gmeow:WritingSystem`` individual (e.g.
    ``"Latn" -> "wsLatin"``). New languages must point ``usesWritingSystem`` at
    these canonical IRIs rather than inventing new code-based IRIs for scripts
    that already exist.
    """
    existing_by_iso3: dict[str, ExistingLanguage] = {}
    existing_by_iso1: dict[str, ExistingLanguage] = {}
    existing_scripts: set[str] = set()
    script_iri_by_code: dict[str, str] = {}
    if not OUTPUT_PATH.exists():
        return existing_by_iso3, existing_by_iso1, existing_scripts, script_iri_by_code

    graph = Graph()
    graph.parse(OUTPUT_PATH, format="turtle")

    language_classes = {
        URIRef(GMEOW_NS + "Language"),
        URIRef(GMEOW_NS + "FormalLanguage"),
        URIRef(GMEOW_NS + "ProgrammingLanguage"),
    }
    natural_language_class = URIRef(GMEOW_NS + "Language")
    for cls in language_classes:
        for subject in graph.subjects(RDF.type, cls):
            if not isinstance(subject, URIRef):
                continue
            iso1: str | None = None
            iso3: str | None = None
            internal_tag: str | None = None
            for pred, obj in graph.predicate_objects(subject):
                p_local = str(pred).replace(GMEOW_NS, "")
                if p_local == "languageCode" and isinstance(obj, Literal):
                    val = str(obj)
                    if len(val) == 2:
                        iso1 = val
                    elif len(val) == 3:
                        iso3 = val
                elif p_local == "languageTag" and isinstance(obj, Literal):
                    internal_tag = str(obj)
            if iso3:
                existing_by_iso3[iso3] = ExistingLanguage(
                    iri=subject, iso639_1=iso1, iso639_3=iso3, internal_tag=internal_tag
                )
            # Only natural gmeow:Language individuals count as ISO 639-1 coverage.
            # Programming/Formal languages may reuse 2-letter strings (e.g. "ts")
            # without satisfying the ISO 639-1 natural-language requirement.
            if iso1 and cls == natural_language_class:
                existing_by_iso1[iso1] = ExistingLanguage(
                    iri=subject, iso639_1=iso1, iso639_3=iso3, internal_tag=internal_tag
                )

    for subject in graph.subjects(RDF.type, URIRef(GMEOW_NS + "WritingSystem")):
        if not isinstance(subject, URIRef):
            continue
        local_name = str(subject).replace(GMEOW_NS, "")
        for pred, obj in graph.predicate_objects(subject):
            p_local = str(pred).replace(GMEOW_NS, "")
            if p_local == "scriptCode" and isinstance(obj, Literal):
                code = str(obj)
                existing_scripts.add(code)
                script_iri_by_code[code] = local_name

    return existing_by_iso3, existing_by_iso1, existing_scripts, script_iri_by_code


def build_seed(
    refresh: bool = False,
) -> tuple[list[LanguageSeed], set[str], dict[str, str]]:
    """Build the language seed and the set of required script codes."""
    (
        existing_by_iso3,
        existing_by_iso1,
        existing_scripts,
        script_iri_by_code,
    ) = parse_existing_catalog()

    if not refresh and SEED_PATH.exists():
        loaded_seed = [
            lang
            for lang in load_seed()
            if lang.iso639_3 not in existing_by_iso3
            and (lang.iso639_1 is None or lang.iso639_1 not in existing_by_iso1)
        ]
        required_scripts: set[str] = set(existing_scripts)
        for lang in loaded_seed:
            required_scripts.update(lang.script_codes)
        return loaded_seed, required_scripts, script_iri_by_code

    iso_rows = fetch_iso6393_table()
    iso1_rows = [row for row in iso_rows if row.get("Part1")]
    _log.info("Found %d ISO 639-1 mapped rows", len(iso1_rows))

    iso3_to_part1: dict[str, str] = {row["Id"]: row["Part1"] for row in iso1_rows}
    overrides = load_overrides()

    # Query Wikidata in chunks to avoid overly long SPARQL URLs.
    all_iso3 = [row["Id"] for row in iso1_rows]
    all_iso3 += [
        iso3
        for iso3 in overrides
        if iso3 not in all_iso3 and _is_enabled_flag(overrides[iso3].get("include"))
    ]
    wikidata: dict[str, dict[str, str]] = {}
    chunk_size = 100
    with httpx.Client(timeout=120.0) as client:
        for i in range(0, len(all_iso3), chunk_size):
            chunk = all_iso3[i : i + chunk_size]
            wikidata.update(query_wikidata(client, chunk, fail_on_error=refresh))
            _log.info(
                "Looked up %d/%d ISO 639-3 codes on Wikidata",
                min(i + chunk_size, len(all_iso3)),
                len(all_iso3),
            )

    seed: list[LanguageSeed] = []
    seen_internal: set[str] = set()
    required_scripts = set(existing_scripts)

    for row in iso1_rows:
        iso1 = row["Part1"]
        iso3 = row["Id"]
        if iso3 in existing_by_iso3 or iso1 in existing_by_iso1:
            _log.debug("Skipping existing ISO 639-1 language %s (%s)", iso1, iso3)
            continue
        ref_name = row.get("Ref_Name", iso3)
        label = _title_case(ref_name)

        override = overrides.get(iso3, {})
        if override.get("label"):
            label = override["label"]

        internal_tag = _internal_tag(label)
        if internal_tag in seen_internal:
            internal_tag = f"{internal_tag}-{iso3}"
        seen_internal.add(internal_tag)

        w = wikidata.get(iso3, {})
        qid = override.get("wikidata_qid") or w.get("qid")
        glotto = override.get("glottolog_id") or w.get("glottolog_id")
        endonym = override.get("endonym") or None

        script_codes = [
            c.strip() for c in override.get("script_codes", "").split(",") if c.strip()
        ]
        if not script_codes:
            script_codes = DEFAULT_SCRIPTS.get(iso1, ["Latn"])

        seed.append(
            LanguageSeed(
                iso639_1=iso1,
                iso639_3=iso3,
                label=label,
                definition=override.get("definition") or _definition_from_label(label),
                internal_tag=internal_tag,
                bcp47_tag=iso1,
                origin=override.get("origin", "originNatural"),
                modality=override.get("modality", "modalitySpoken, modalityWritten"),
                status=override.get("status", "statusLiving"),
                script_codes=script_codes,
                wikidata_qid=qid,
                glottolog_id=glotto,
                endonym=endonym,
            )
        )
        required_scripts.update(script_codes)

    # Add selected ISO 639-3-only languages from overrides.
    for iso3, override in overrides.items():
        if iso3 in iso3_to_part1 or not _is_enabled_flag(override.get("include")):
            continue
        if iso3 in existing_by_iso3:
            continue
        iso1_from_override = _optional_strip(override.get("iso639_1"))
        if iso1_from_override and iso1_from_override in existing_by_iso1:
            continue
        label = override.get("label", iso3)
        internal_tag = _internal_tag(label)
        if internal_tag in seen_internal:
            internal_tag = f"{internal_tag}-{iso3}"
        seen_internal.add(internal_tag)
        w = wikidata.get(iso3, {})
        qid = override.get("wikidata_qid") or w.get("qid")
        glotto = override.get("glottolog_id") or w.get("glottolog_id")
        script_codes = [
            c.strip() for c in override.get("script_codes", "").split(",") if c.strip()
        ]
        if not script_codes:
            script_codes = ["Latn"]
        seed.append(
            LanguageSeed(
                iso639_1=iso1_from_override,
                iso639_3=iso3,
                label=label,
                definition=override.get("definition") or _definition_from_label(label),
                internal_tag=internal_tag,
                bcp47_tag=override.get("bcp47_tag") or iso1_from_override or iso3,
                origin=override.get("origin", "originNatural"),
                modality=override.get("modality", "modalitySpoken, modalityWritten"),
                status=override.get("status", "statusLiving"),
                script_codes=script_codes,
                wikidata_qid=qid,
                glottolog_id=glotto,
                endonym=override.get("endonym") or None,
            )
        )
        required_scripts.update(script_codes)

    return seed, required_scripts, script_iri_by_code


_CATALOG_IRI = "https://blackcatinformatics.ca/gmeow/imports/languages-reference"


def render_language(lang: LanguageSeed, script_iri_by_code: dict[str, str]) -> str:
    """Render one language block including its appellations."""
    suffix = lang.iri_suffix
    label_esc = _escape_turtle_string(lang.label)
    def_esc = _escape_turtle_string(lang.definition)

    lines: list[str] = [
        f"gmeow:lang{suffix} a gmeow:Language ;",
        f'    rdfs:label "{label_esc}"@x-gmeow-english ;',
        f'    skos:definition "{def_esc}"@x-gmeow-english ;',
        f'    gmeow:languageTag "{lang.internal_tag}" ;',
    ]
    if lang.bcp47_tag:
        lines.append(f'    gmeow:bcp47Tag "{lang.bcp47_tag}"^^xsd:language ;')
    if lang.iso639_1:
        lines.append(f'    gmeow:languageCode "{lang.iso639_1}" ;')
    lines.append(f'    gmeow:languageCode "{lang.iso639_3}" ;')
    lines.append(f"    gmeow:languageOrigin gmeow:{lang.origin} ;")
    modalities = ", ".join(f"gmeow:{m.strip()}" for m in lang.modality.split(","))
    lines.append(f"    gmeow:languageModality {modalities} ;")
    lines.append(f"    gmeow:languageStatus gmeow:{lang.status} ;")
    scripts = ", ".join(
        f"gmeow:{script_iri_by_code.get(code, f'ws{code}')}"
        for code in lang.script_codes
    )
    lines.append(f"    gmeow:usesWritingSystem {scripts} ;")

    app_refs: list[str] = []
    app_blocks: list[str] = []

    if lang.endonym and lang.endonym != lang.label:
        endonym_suffix = _iri_suffix(lang.endonym) or f"{suffix}Endonym"
        app_iri = f"gmeow:app{endonym_suffix}Endonym"
        app_refs.append(app_iri)
        endonym_esc = _escape_turtle_string(lang.endonym)
        app_blocks.append(
            f"{app_iri} a gmeow:Appellation ;\n"
            f'    rdfs:label "{endonym_esc}"@x-gmeow-english ;\n'
            "    skos:definition "
            f"\"Appellation '{endonym_esc}' (namePurposeEndonym) in the "
            'GMEOW languages reference catalog."@x-gmeow-english ;\n'
            f'    gmeow:fullName "{endonym_esc}"@{lang.internal_tag} ;\n'
            f"    gmeow:nameLanguage gmeow:lang{suffix} ;\n"
            "    gmeow:namePurpose gmeow:namePurposeEndonym ;\n"
            "    gmeow:displayable true ;\n"
            f"    rdfs:isDefinedBy <{_CATALOG_IRI}> ."
        )

    exonym_suffix = _iri_suffix(lang.label) or f"{suffix}Exonym"
    app_iri = f"gmeow:app{exonym_suffix}Exonym"
    app_refs.append(app_iri)
    app_blocks.append(
        f"{app_iri} a gmeow:Appellation ;\n"
        f'    rdfs:label "{label_esc}"@x-gmeow-english ;\n'
        "    skos:definition "
        f"\"Appellation '{label_esc}' (namePurposeExonym) in the "
        'GMEOW languages reference catalog."@x-gmeow-english ;\n'
        f'    gmeow:fullName "{label_esc}"@{lang.internal_tag} ;\n'
        f"    gmeow:nameLanguage gmeow:lang{suffix} ;\n"
        "    gmeow:namePurpose gmeow:namePurposeExonym ;\n"
        "    gmeow:displayable true ;\n"
        f"    rdfs:isDefinedBy <{_CATALOG_IRI}> ."
    )

    lines.append(f"    gmeow:hasAppellation {', '.join(app_refs)} ;")

    if lang.wikidata_qid:
        lines.append(f"    skos:exactMatch wd:{lang.wikidata_qid} ;")
    lines.append(
        f"    skos:exactMatch <http://lexvo.org/id/iso639-3/{lang.iso639_3}> ;"
    )
    if lang.glottolog_id:
        lines.append(
            "    skos:exactMatch "
            f"<https://glottolog.org/resource/languoid/id/{lang.glottolog_id}> ;"
        )
    lines.append(f"    rdfs:isDefinedBy <{_CATALOG_IRI}> .")

    return "\n".join(lines) + "\n\n" + "\n\n".join(app_blocks) + "\n"


def render_writing_system(ws: WritingSystemSeed) -> str:
    """Render one gmeow:WritingSystem individual."""
    label_esc = _escape_turtle_string(ws.label)
    return (
        f"gmeow:ws{ws.code} a gmeow:WritingSystem ;\n"
        f'    rdfs:label "{label_esc} writing system"@x-gmeow-english ;\n'
        f'    skos:definition "The {label_esc} script."@x-gmeow-english ;\n'
        f'    gmeow:scriptCode "{ws.code}" ;\n'
        f"    gmeow:writingSystemType gmeow:{ws.ws_type} ;\n"
        f"    gmeow:textDirection gmeow:{ws.direction} ;\n"
        f"    skos:exactMatch wd:{ws.wikidata_qid} ;\n"
        f"    rdfs:isDefinedBy <{_CATALOG_IRI}> .\n"
    )


def insert_into_file(
    file_text: str,
    writing_systems: list[WritingSystemSeed],
    languages: list[LanguageSeed],
    script_iri_by_code: dict[str, str],
) -> str:
    """Insert new writing systems and languages into the existing Turtle text."""
    natural_marker = "# Natural Languages\n"
    if writing_systems:
        if natural_marker in file_text:
            ws_block = (
                "\n".join(render_writing_system(ws) for ws in writing_systems) + "\n"
            )
            file_text = file_text.replace(natural_marker, ws_block + natural_marker, 1)
        else:
            _log.warning("Could not find Natural Languages section marker")

    if languages:
        prog_marker = "# Programming & Formal Languages\n"
        lang_block = "\n".join(
            render_language(lang, script_iri_by_code) for lang in languages
        )
        if prog_marker in file_text:
            file_text = file_text.replace(
                prog_marker, lang_block + "\n" + prog_marker, 1
            )
        else:
            backfill_marker = "# Backfilled annotations\n"
            if backfill_marker in file_text:
                file_text = file_text.replace(
                    backfill_marker, lang_block + "\n" + backfill_marker, 1
                )
            else:
                file_text = file_text.rstrip() + "\n\n" + lang_block + "\n"

    return file_text


def main(argv: list[str] | None = None) -> int:
    """Run the generator."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Refetch external data and rebuild the seed from scratch.",
    )
    parser.add_argument(
        "--preview",
        action="store_true",
        help="Write to stdout instead of modifying files.",
    )
    args = parser.parse_args(argv)

    seed, required_scripts, script_iri_by_code = build_seed(refresh=args.refresh)
    _log.info(
        "Adding %d new languages; %d script codes required",
        len(seed),
        len(required_scripts),
    )

    # Build writing-system seeds for any required scripts not already in the file.
    _, _, existing_scripts, _ = parse_existing_catalog()
    new_writing_systems: list[WritingSystemSeed] = []
    for code in sorted(required_scripts):
        if code in existing_scripts:
            continue
        if code not in WRITING_SYSTEMS:
            _log.warning("No metadata for script %s; skipping", code)
            continue
        label, ws_type, direction, qid = WRITING_SYSTEMS[code]
        new_writing_systems.append(
            WritingSystemSeed(code, label, ws_type, direction, qid)
        )

    if args.preview:
        print("# Writing systems to add:")
        for ws in new_writing_systems:
            print(render_writing_system(ws))
        print("# Languages to add:")
        for lang in seed:
            print(render_language(lang, script_iri_by_code))
        return 0

    if args.refresh or not SEED_PATH.exists():
        save_seed(seed)
        _log.info("Wrote seed: %s", SEED_PATH)

    existing_text = OUTPUT_PATH.read_text(encoding="utf-8")
    new_text = insert_into_file(
        existing_text, new_writing_systems, seed, script_iri_by_code
    )
    OUTPUT_PATH.write_text(new_text, encoding="utf-8")
    _log.info("Updated catalog: %s", OUTPUT_PATH)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
