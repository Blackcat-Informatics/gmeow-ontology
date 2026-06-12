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
    # The (possibly partial) fold is still emitted, but any diagnostic —
    # or never reaching segmentation at all — is a nonzero exit, so
    # `gts fold … && publish` pipelines fail on damage.
    return 1 if g.diagnostics or not g.segment_heads else 0


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
        or (gq is not None and (s, p, o, gq) in quad_sup)
        or s in term_sup
        or p in term_sup
        or o in term_sup
        or (gq is not None and gq in term_sup)
        for s, p, o, gq in g.quads
    )


def _write_out(out: str | None, data: bytes) -> int:
    """Write to a path or stdout; IO failure is exit 2, never a traceback."""
    try:
        if out is not None:
            Path(out).write_bytes(data)
        else:
            sys.stdout.buffer.write(data)
    except OSError as exc:  # includes BrokenPipeError
        print(f"gts: cannot write {out or 'stdout'}: {exc}", file=sys.stderr)
        return 2
    return 0


def _cmd_ls(path: str) -> int:
    """List inline blobs: digest, size, declared media type (tar's ``t``)."""
    g = read(_load(path))
    for d in g.diagnostics:
        print(f"gts: diagnostic {d.code}: {d.detail}", file=sys.stderr)
    for digest, data in g.blobs.items():
        mt = g.blob_meta.get(digest, {}).get("mt")
        mt_text = mt if isinstance(mt, str) else "-"
        print(f"{digest}  {len(data):>10}  {mt_text}")
    return 0


def _normalize_digest(digest: str) -> str:
    return digest if digest.startswith("blake3:") else f"blake3:{digest}"


def _suppressed_blob_digests(g: Graph) -> set[str]:
    """Digests hidden by ``{"kind": "blob", "digest": …}`` targets (§11)."""
    out: set[str] = set()
    for sup in g.suppressions:
        for target in sup.targets:
            if target.get("kind") != "blob":
                continue
            d = target.get("digest")
            if isinstance(d, bytes):
                out.add(f"blake3:{d.hex()}")
            elif isinstance(d, str):
                out.add(_normalize_digest(d))
    return out


def _cmd_extract(
    path: str,
    digest: str,
    out: str | None,
    mt: str | None,
    include_suppressed: bool,
) -> int:
    """Extract one blob by content digest (tar's ``x``), refuse-don't-trust.

    Verifies the bytes against the requested digest on the way out, honours
    blob suppression (§11) unless overridden, and treats ``--mt`` as an
    ASSERTION against the blob's declared media type — never a conversion.
    """
    g = read(_load(path))
    digest = _normalize_digest(digest)
    data = g.blobs.get(digest)
    if data is None:
        print(f"gts: no inline blob {digest} in {path}", file=sys.stderr)
        return 1
    if digest in _suppressed_blob_digests(g) and not include_suppressed:
        print(
            f"gts: refusing {digest}: suppressed (§11); "
            "pass --include-suppressed to extract anyway",
            file=sys.stderr,
        )
        return 1
    if mt is not None:
        declared = g.blob_meta.get(digest, {}).get("mt")
        if declared != mt:
            print(
                f"gts: refusing {digest}: declared media type "
                f"{declared!r} does not match asserted {mt!r}",
                file=sys.stderr,
            )
            return 1
    from gts.wire import digest_str

    if digest_str(data) != digest:
        print(
            f"gts: integrity failure: {digest} bytes re-hash differently",
            file=sys.stderr,
        )
        return 1
    return _write_out(out, data)


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

    return _write_out(out, bytes(combined))


def _cmd_pack(sources: list[str], out: str, external_over: int | None) -> int:
    """Pack files/directories into a files-profile GTS archive (tar's ``c``)."""
    from gts.files import pack

    try:
        data = pack([Path(s) for s in sources], external_over=external_over)
    except (OSError, ValueError) as exc:
        print(f"gts: refusing pack: {exc}", file=sys.stderr)
        return 1
    return _write_out(out, data)


def _cmd_unpack(path: str, dest: str | None, include_suppressed: bool) -> int:
    """Unpack a files-profile GTS archive (tar's ``x``), verifying digests."""
    from gts.files import unpack

    g = read(_load(path))
    for d in g.diagnostics:
        print(f"gts: diagnostic {d.code}: {d.detail}", file=sys.stderr)
    try:
        unpack(g, Path(dest or "."), include_suppressed=include_suppressed)
    except (OSError, ValueError) as exc:
        print(f"gts: refusing unpack: {exc}", file=sys.stderr)
        return 1
    return 0


def _cmd_diff(path: str, directory: str) -> int:
    """Compare an archive to a directory by content digest (tar's ``d``)."""
    from gts.files import diff

    g = read(_load(path))
    for d in g.diagnostics:
        print(f"gts: diagnostic {d.code}: {d.detail}", file=sys.stderr)
    try:
        lines = diff(g, Path(directory))
    except (OSError, ValueError) as exc:
        print(f"gts: refusing diff: {exc}", file=sys.stderr)
        return 1
    for line in lines:
        print(line)
    return 1 if lines else 0


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

    p_ls = sub.add_parser(
        "ls", help="list inline blobs: digest, size, declared media type"
    )
    p_ls.add_argument("file")

    p_extract = sub.add_parser(
        "extract",
        help="extract one blob by content digest; --mt asserts the declared "
        "media type (never converts)",
    )
    p_extract.add_argument("file")
    p_extract.add_argument("digest")
    p_extract.add_argument("-o", "--out", default=None)
    p_extract.add_argument("--mt", default=None)
    p_extract.add_argument("--include-suppressed", action="store_true")

    p_cat = sub.add_parser(
        "cat",
        help="validating composer: refuse degenerate inputs, then "
        "byte-concatenate (§3.1, §14.1)",
    )
    p_cat.add_argument("files", nargs="+")
    p_cat.add_argument("-o", "--out", default=None)

    p_pack = sub.add_parser(
        "pack", help="pack files/directories into a files-profile GTS archive"
    )
    p_pack.add_argument("sources", nargs="+")
    p_pack.add_argument("-o", "--out", required=True)
    p_pack.add_argument(
        "--external-over",
        type=int,
        default=None,
        help="store files larger than N bytes as external blobs",
    )

    p_unpack = sub.add_parser("unpack", help="unpack a files-profile GTS archive")
    p_unpack.add_argument("file")
    p_unpack.add_argument("-C", dest="dest", default=None)
    p_unpack.add_argument(
        "--include-suppressed",
        action="store_true",
        help="extract digest-suppressed entries anyway",
    )

    p_diff = sub.add_parser(
        "diff",
        help="compare a files-profile GTS archive to a directory by digest",
    )
    p_diff.add_argument("archive")
    p_diff.add_argument("directory")

    args = parser.parse_args(argv)
    if args.command == "info":
        return _cmd_info(args.files)
    if args.command == "fold":
        return _cmd_fold(args.file)
    if args.command == "verify":
        return _cmd_verify(args.files)
    if args.command == "ls":
        return _cmd_ls(args.file)
    if args.command == "extract":
        return _cmd_extract(
            args.file, args.digest, args.out, args.mt, args.include_suppressed
        )
    if args.command == "pack":
        return _cmd_pack(args.sources, args.out, args.external_over)
    if args.command == "unpack":
        return _cmd_unpack(args.file, args.dest, args.include_suppressed)
    if args.command == "diff":
        return _cmd_diff(args.archive, args.directory)
    return _cmd_cat(args.files, args.out)


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
