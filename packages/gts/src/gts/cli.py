# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The ``gts`` command-line tool: inspect, fold, verify, and compose GTS files.

``cat`` and ``verify`` implement the §14.1 composition-tooling contract: raw
byte concatenation is always valid GTS (§3.1), but a publish-class tool
refuses pathological states instead of trusting them to be intentional. The
Rust engine ships a binary with the IDENTICAL command surface; this entry
point keeps the contract while the native wheel lands.

Exit codes: 0 clean; 1 diagnostics found or input refused; 2 usage/IO error.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from gts.model import Graph
from gts.nquads import to_nquads
from gts.reader import read, read_segments


def _load(path: str) -> bytes:
    try:
        return Path(path).read_bytes()
    except OSError as exc:
        print(f"gts: cannot read {path}: {exc}", file=sys.stderr)
        raise SystemExit(2) from exc


def _print_ledger(path: str, segments: list[Graph], torn: int | None) -> None:
    """Print the per-segment composition ledger (§14.1 "SHOULD report")."""
    suffix = f", TORN at byte {torn}" if torn is not None else ""
    print(f"{path}: {len(segments)} segment(s){suffix}")
    for idx, seg in enumerate(segments):
        head = seg.segment_heads[0].hex() if seg.segment_heads else "<none>"
        profile = seg.segment_profiles[0] if seg.segment_profiles else "<none>"
        signers = sum(1 for s in seg.signatures if s.status != "invalid")
        print(
            f"  segment {idx}: head {head} profile {profile} "
            f"terms {len(seg.terms)} quads {len(seg.quads)} "
            f"reifies {len(seg.reifiers)} annot {len(seg.annotations)} "
            f"blobs {len(seg.blobs)} suppress {len(seg.suppressions)} "
            f"opaque {len(seg.opaque)} sigs {signers}"
        )
        for o in seg.opaque:
            print(f"    opaque: {o.frame_type} ({o.reason})")
        for d in seg.diagnostics:
            where = f" [item {d.frame_index}]" if d.frame_index is not None else ""
            print(f"    diagnostic {d.code}: {d.detail}{where}")


def _has_problems(
    segments: list[Graph], torn: int | None, fatal: object | None
) -> bool:
    return (
        fatal is not None
        or torn is not None
        or any(seg.diagnostics for seg in segments)
    )


def _cmd_info(paths: list[str]) -> int:
    for path in paths:
        segments, torn, fatal = read_segments(_load(path))
        if fatal is not None:
            print(f"{path}: 0 segment(s)")
            print(f"  FATAL {fatal.code}: {fatal.detail}")
            continue
        _print_ledger(path, segments, torn)
    return 0


def _cmd_fold(path: str) -> int:
    g = read(_load(path))
    for d in g.diagnostics:
        print(f"gts: diagnostic {d.code}: {d.detail}", file=sys.stderr)
    sys.stdout.write(to_nquads(g))
    # never reached segmentation — empty file or no leading header
    return 1 if not g.segment_heads else 0


def _cmd_verify(paths: list[str]) -> int:
    problems = False
    for path in paths:
        segments, torn, fatal = read_segments(_load(path))
        if fatal is not None:
            print(f"{path}: 0 segment(s)")
            print(f"  FATAL {fatal.code}: {fatal.detail}")
            problems = True
            continue
        _print_ledger(path, segments, torn)
        problems = problems or _has_problems(segments, torn, fatal)
    return 1 if problems else 0


def _all_quads_suppressed(g: Graph) -> bool:
    """True iff the fold has quads and EVERY one is hidden by a suppression.

    A quad is hidden by a direct quad target or a term target on any of its
    components (§11) — the union graph is value-interned, so id matching IS
    value matching.
    """
    if not g.quads or not g.suppressions:
        return False
    term_sup: set[int] = set()
    quad_sup: set[tuple[int, ...]] = set()
    for sup in g.suppressions:
        for target in sup.targets:
            kind = target.get("kind")
            tid = target.get("id")
            if kind in ("term", "reifier") and isinstance(tid, int):
                term_sup.add(tid)
            elif kind == "quad":
                q = target.get("q")
                if isinstance(q, list) and all(isinstance(x, int) for x in q):
                    quad_sup.add(tuple(q))
    return all(
        (s, p, o) in quad_sup
        or ((s, p, o, gq) in quad_sup if gq is not None else False)
        or term_sup & ({s, p, o} | ({gq} if gq is not None else set()))
        for s, p, o, gq in g.quads
    )


def _cmd_cat(paths: list[str], out: str | None) -> int:
    """The validating composer (§14.1): refuse-don't-trust, then ``cat``."""
    if len(paths) < 2:
        print("gts: cat needs at least two inputs", file=sys.stderr)
        return 2
    combined = bytearray()
    for path in paths:
        data = _load(path)
        segments, torn, fatal = read_segments(data)
        if _has_problems(segments, torn, fatal):
            print(f"gts: refusing {path}: not a clean GTS input", file=sys.stderr)
            return 1
        # §14.1: a segment that contributes NOTHING (no quads, blobs, reifier
        # bindings, annotations, or suppressions) is almost always a wiring
        # bug — never a real package. Refuse, don't trust.
        for idx, seg in enumerate(segments):
            contributes = bool(
                seg.quads
                or seg.blobs
                or seg.reifiers
                or seg.annotations
                or seg.suppressions
            )
            if not contributes:
                print(
                    f"gts: refusing {path}: segment {idx} folds to nothing "
                    "(no quads/blobs/reifies/annot/suppress) — wiring bug?",
                    file=sys.stderr,
                )
                return 1
        combined += data

    # §14.1: refuse an output in which suppressions would hide every quad.
    folded = read(bytes(combined))
    if _all_quads_suppressed(folded):
        print(
            "gts: refusing composition: suppressions hide every quad in the "
            "folded output",
            file=sys.stderr,
        )
        return 1

    if out is not None:
        Path(out).write_bytes(bytes(combined))
    else:
        sys.stdout.buffer.write(bytes(combined))
    return 0


def main(argv: list[str] | None = None) -> int:
    """Entry point for the ``gts`` console script."""
    parser = argparse.ArgumentParser(
        prog="gts",
        description="Inspect, fold, verify, and compose GTS files.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_info = sub.add_parser("info", help="per-segment composition ledger (§14.1)")
    p_info.add_argument("files", nargs="+")

    p_fold = sub.add_parser("fold", help="fold to N-Quads on stdout")
    p_fold.add_argument("file")

    p_verify = sub.add_parser(
        "verify", help="verify chains; ledger + diagnostics; exit 1 on any"
    )
    p_verify.add_argument("files", nargs="+")

    p_cat = sub.add_parser(
        "cat",
        help="validating composer: refuse degenerate inputs, then "
        "byte-concatenate (§3.1, §14.1)",
    )
    p_cat.add_argument("files", nargs="+")
    p_cat.add_argument("-o", "--out", default=None)

    args = parser.parse_args(argv)
    if args.command == "info":
        return _cmd_info(args.files)
    if args.command == "fold":
        return _cmd_fold(args.file)
    if args.command == "verify":
        return _cmd_verify(args.files)
    return _cmd_cat(args.files, args.out)


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
