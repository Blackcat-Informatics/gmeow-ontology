#!/usr/bin/env python3
"""One-off migration: transcribe mappings/*.sssom.tsv → mapping-dsl/equivalences/.

Each SSSOM data row becomes a gmeow:TermEquivalence cell; each file's YAML header
becomes a gmeow:MappingSet resource (carrying any trailing commented-out rows as a
setTrailer). Run once to bootstrap the DSL source from the hand-authored TSVs; the
result is verified by compiling it back and comparing to the originals. Throwaway
tooling — not part of the runtime.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import MAPPINGS_DIR, PREFIXES

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "mapping-dsl" / "equivalences"
_ALWAYS = ("gmeow", "owl", "rdfs", "skos", "semapv")


def _camel(text: str) -> str:
    return "".join(part.capitalize() for part in text.replace("_", "-").split("-"))


def _used_prefixes(curies: set[str]) -> list[str]:
    used: set[str] = set(_ALWAYS)
    for curie in curies:
        if ":" in curie:
            used.add(curie.split(":", 1)[0])
    return [p for p in PREFIXES if p in used]


def _ttl_literal(text: str) -> str:
    escaped = (
        text.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\t", "\\t")
        .replace("\r", "")
        .replace("\n", "\\n")
    )
    return '"' + escaped + '"'


def bootstrap() -> list[Path]:
    """Transcribe every SSSOM TSV into a mapping-dsl/equivalences/*.ttl file."""
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    for tsv in sorted(MAPPINGS_DIR.glob("*.sssom.tsv")):
        lines = tsv.read_text(encoding="utf-8").splitlines()
        header: dict[str, str] = {}
        data_lines: list[str] = []
        trailer: list[str] = []
        seen_columns = False
        columns: list[str] = []
        for line in lines:
            if line.startswith("#"):
                if not seen_columns and ":" in line:
                    key, _, val = line[1:].partition(":")
                    header[key.strip()] = val.strip()
                else:
                    trailer.append(line)  # forward-looking commented rows
                continue
            if not seen_columns:
                columns = line.split("\t")
                required = {"subject_id", "predicate_id", "object_id"}
                missing = required.difference(columns)
                if missing:
                    raise ValueError(
                        f"{tsv.name}: missing required SSSOM columns: {sorted(missing)}"
                    )
                seen_columns = True
                continue
            if line.strip():
                data_lines.append(line)

        domain = tsv.name.removeprefix("gmeow-").removesuffix(".sssom.tsv")
        cells: list[str] = []
        all_curies: set[str] = {"semapv:ManualMappingCuration"}
        for i, row in enumerate(data_lines, start=1):
            fields = dict(zip(columns, row.split("\t"), strict=False))
            subj = fields["subject_id"].strip()
            pred = fields["predicate_id"].strip()
            obj = fields["object_id"].strip()
            just = (
                fields.get("mapping_justification", "").strip()
                or "semapv:ManualMappingCuration"
            )
            conf = fields.get("confidence", "").strip()
            comment = fields.get("comment", "").strip()
            all_curies.update({subj, pred, obj, just})
            cell = [
                f"gmeow:eq{_camel(domain)}{i:03d} a gmeow:TermEquivalence ;",
                f"    gmeow:alignSubject {subj} ;",
                f"    gmeow:alignPredicate {pred} ;",
                f"    gmeow:alignObject {obj} ;",
                f"    gmeow:justification {just} ;",
            ]
            if conf:
                cell.append(f"    gmeow:confidence {conf} ;")
            if comment:
                cell.append(f"    gmeow:comment {_ttl_literal(comment)} ;")
            cell.append(f'    gmeow:sssomFile "{tsv.name}" .')
            cells.append("\n".join(cell))

        # MappingSet header resource.
        set_lines = [
            f"gmeow:mapset{_camel(domain)} a gmeow:MappingSet ;",
            f'    gmeow:sssomFile "{tsv.name}" ;',
            f"    gmeow:setId {_ttl_literal(header.get('mapping_set_id', ''))} ;",
            f"    gmeow:license {_ttl_literal(header.get('license', ''))} ;",
        ]
        if trailer:
            set_lines.append(
                f"    gmeow:setComment {_ttl_literal(header.get('comment', ''))} ;"
            )
            set_lines.append(
                f"    gmeow:setTrailer {_ttl_literal(chr(10).join(trailer))} ."
            )
        else:
            set_lines.append(
                f"    gmeow:setComment {_ttl_literal(header.get('comment', ''))} ."
            )

        prefixes = "\n".join(
            f"@prefix {p}: <{PREFIXES[p]}> ." for p in _used_prefixes(all_curies)
        )
        body = "\n".join(
            [
                prefixes,
                "",
                f"# Term equivalences for the {domain} domain (GMEOW source).",
                f"# Compiled to mappings/{tsv.name} by `gmeow compile-mappings`.",
                "",
                "\n".join(set_lines),
                "",
                "\n\n".join(cells),
                "",
            ]
        )
        out = OUT_DIR / f"{domain}.ttl"
        out.write_text(body, encoding="utf-8")
        written.append(out)
        print(f"  {out.relative_to(ROOT)}  ({len(cells)} cells)")
    return written


if __name__ == "__main__":
    paths = bootstrap()
    print(f"✓ wrote {len(paths)} equivalence files")
