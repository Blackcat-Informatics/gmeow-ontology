// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { readFileSync, writeFileSync } from "node:fs";
import { Read, ReadFileSegments } from "../reader.js";
import { toNQuads } from "../nquads.js";
import { pack, unpack, diff, suppressedBlobDigests } from "../files.js";
import { compactStreamable, CompactRefusedError } from "../compact.js";
import { STREAM_NS } from "../stream.js";
import {
    hex,
    mapGet,
    asInt,
    asInt64,
    asText,
    textOr,
    normalizeDigest,
} from "../wire.js";
import type { Graph, FileSegments } from "../reader.js";
import { TermKind, type Quad, type Suppression } from "../model.js";

const usage = `usage: gts <command> [args]

commands:
  info <file>...            per-segment composition ledger
  fold <file>               fold to N-Quads on stdout
  verify <file>...          verify chains; ledger + diagnostics; exit 1 on any
  ls <file>                 list inline blobs: digest, size, declared media type
  extract <file> <digest> [-o out] [--mt TYPE] [--include-suppressed]
                            extract one blob by content digest
  cat -o <out> <file>...    validating composer: refuse degenerate inputs, then concatenate
  compact <file> -o <out> --streamable [--seal-original] [--timestamp ISO]
                            rewrite into the streamable layout state (§10.1)
  pack <dir|file>... -o out.gts
                            pack files/directories into a files-profile archive
  unpack <archive> [-C dir] [--include-suppressed]
                            unpack a files-profile archive
  diff <archive> <dir>      compare archive to directory by digest`;

function load(path: string): Uint8Array {
    return readFileSync(path);
}

function printLedger(path: string, fs: FileSegments): void {
    const tornSuffix = fs.torn >= 0 ? `, TORN at byte ${fs.torn}` : "";
    console.log(`${path}: ${fs.segments.length} segment(s)${tornSuffix}`);
    if (fs.fatal) {
        console.log(`  FATAL ${fs.fatal.code}: ${fs.fatal.detail}`);
        return;
    }
    for (let idx = 0; idx < fs.segments.length; idx++) {
        const seg = fs.segments[idx];
        const head = seg.segmentHeads[0] ? hex(seg.segmentHeads[0]) : "<none>";
        const profile = seg.segmentProfiles[0] ?? "<none>";
        const signers = seg.signatures.filter(
            (s) => s.status !== "invalid",
        ).length;
        console.log(
            `  segment ${idx}: head ${head} profile ${profile} terms ${seg.terms.length} quads ${seg.quads.length} reifies ${seg.reifiers.length} annot ${seg.annotations.length} blobs ${seg.blobs.length} suppress ${seg.suppressions.length} opaque ${seg.opaque.length} sigs ${signers}`,
        );
        const layout = seg.segmentStreamable[0];
        if (layout !== undefined && layout.claimed) {
            const headHex =
                layout.head !== undefined ? hex(layout.head) : "<none>";
            const tail = layout.tail
                ? `, accretive tail ${layout.tail} frame(s)`
                : "";
            console.log(
                `    layout: streamable through frame ${layout.covered} (head ${headHex})${tail}`,
            );
        }
        for (const o of seg.opaque) {
            console.log(`    opaque: ${o.frameType} (${o.reason})`);
        }
        for (const d of seg.diagnostics) {
            const idxStr =
                d.frameIndex !== undefined ? ` [item ${d.frameIndex}]` : "";
            console.log(`    diagnostic ${d.code}: ${d.detail}${idxStr}`);
        }
    }
}

function hasProblems(fs: FileSegments): boolean {
    if (fs.fatal || fs.torn >= 0) return true;
    for (const seg of fs.segments) {
        if (seg.diagnostics.length > 0) return true;
    }
    return false;
}

function blobMT(g: Graph, digest: string): string {
    for (const bm of g.blobMeta) {
        if (bm.digest !== digest) continue;
        if (!(bm.meta instanceof Map)) continue;
        const v = mapGet(bm.meta, "mt");
        const s = asText(v);
        if (s !== undefined) return s;
    }
    return "";
}

function targetKind(target: unknown): string {
    if (!(target instanceof Map)) return "";
    const v = mapGet(target, "kind");
    return v !== undefined ? textOr(v, "") : "";
}

function targetIdx(target: unknown): number | undefined {
    if (!(target instanceof Map)) return undefined;
    const v = mapGet(target, "id");
    return v !== undefined ? asInt(v) : undefined;
}

function quadKey(q: Quad): string {
    return q.g === undefined
        ? `${q.s},${q.p},${q.o}`
        : `${q.s},${q.p},${q.o},${q.g}`;
}

function collectSuppressed(
    sup: Suppression,
    termSup: Set<number>,
    quadSup: Set<string>,
): void {
    for (const target of sup.targets) {
        switch (targetKind(target)) {
            case "term":
            case "reifier": {
                const id = targetIdx(target);
                if (id !== undefined) termSup.add(id);
                break;
            }
            case "quad": {
                if (!(target instanceof Map)) continue;
                const v = mapGet(target, "q");
                const ids = Array.isArray(v) ? v : undefined;
                if (!ids) continue;
                const parts: string[] = [];
                let valid = true;
                for (const x of ids) {
                    const n = asInt64(x);
                    if (n === undefined) {
                        valid = false;
                        break;
                    }
                    parts.push(String(n));
                }
                if (valid) quadSup.add(parts.join(","));
                break;
            }
        }
    }
}

function allQuadsSuppressed(g: Graph): boolean {
    if (g.quads.length === 0 || g.suppressions.length === 0) return false;
    const termSup = new Set<number>();
    const quadSup = new Set<string>();
    for (const sup of g.suppressions) collectSuppressed(sup, termSup, quadSup);
    for (const q of g.quads) {
        if (quadSup.has(quadKey(q))) continue;
        if (termSup.has(q.s)) continue;
        if (termSup.has(q.p)) continue;
        if (termSup.has(q.o)) continue;
        if (q.g !== undefined && termSup.has(q.g)) continue;
        return false;
    }
    return true;
}

function cmdInfo(paths: string[]): number {
    if (paths.length === 0) {
        console.error(usage);
        return 2;
    }
    let problems = false;
    for (const path of paths) {
        const data = load(path);
        const fs = ReadFileSegments(data);
        printLedger(path, fs);
        if (hasProblems(fs)) problems = true;
    }
    return problems ? 1 : 0;
}

function cmdFold(paths: string[]): number {
    if (paths.length !== 1) {
        console.error(usage);
        return 2;
    }
    const g = Read(load(paths[0]), true);
    for (const d of g.diagnostics) {
        console.error(`gts: diagnostic ${d.code}: ${d.detail}`);
    }
    process.stdout.write(toNQuads(g));
    if (g.diagnostics.length > 0 || g.segmentHeads.length === 0) return 1;
    return 0;
}

/** Warn on `stream#` vocabulary in an unclaimed segment (§13.3).
 *
 * A warning, never an error: compaction-provenance quads legitimately
 * survive nq → gts round trips and re-accretion — the error class is
 * reserved for a claimed layout the bytes contradict (the reader's
 * StreamableLayoutError).
 */
/** Write to a path or stdout; IO failure is exit 2, never a traceback. */
function writeOut(outPath: string, data: Uint8Array): number {
    try {
        if (outPath) {
            writeFileSync(outPath, data);
        } else {
            process.stdout.write(data);
        }
    } catch (e) {
        console.error(
            `gts: cannot write ${outPath || "stdout"}: ${(e as Error).message}`,
        );
        return 2;
    }
    return 0;
}

/** Declared-vs-computed profile requirement checks (§14.1).
 *
 * Returns [message, isError] pairs: vocabulary used without its profile
 * declared is an error; a declared-but-unused profile is a warning.
 */
const PROFILE_VOCABS: Record<string, string> = {
    files: "https://w3id.org/gts/files#",
};

function namespaceOf(iri: string): string {
    const h = iri.lastIndexOf("#");
    if (h >= 0) return iri.slice(0, h + 1);
    const sl = iri.lastIndexOf("/");
    if (sl >= 0) return iri.slice(0, sl + 1);
    return iri;
}

function profileCheck(seg: Graph): Array<[string, boolean]> {
    const declared = new Set(seg.segmentProfiles);
    const vocabs = new Set(Object.values(PROFILE_VOCABS));
    const used = new Set<string>();
    const n = seg.terms.length;
    for (const q of seg.quads) {
        for (const tid of [q.s, q.p, q.o]) {
            if (tid < 0 || tid >= n) continue;
            const term = seg.terms[tid];
            if (term.kind !== TermKind.Iri || term.value === "") continue;
            const ns = namespaceOf(term.value);
            if (vocabs.has(ns)) used.add(ns);
        }
    }
    const out: Array<[string, boolean]> = [];
    for (const [prof, vocab] of Object.entries(PROFILE_VOCABS)) {
        const declares = declared.has(prof);
        const uses = used.has(vocab);
        if (uses && !declares) {
            out.push([
                `profile error: segment uses ${vocab} vocabulary ` +
                    `but does not declare '${prof}'`,
                true,
            ]);
        }
        if (declares && !uses) {
            out.push([
                `profile warning: segment declares '${prof}' ` +
                    `but uses no ${vocab} vocabulary`,
                false,
            ]);
        }
    }
    return out;
}

function streamVocabCheck(seg: Graph): string[] {
    const claimed =
        seg.segmentStreamable.length > 0 && seg.segmentStreamable[0].claimed;
    if (claimed) return [];
    const n = seg.terms.length;
    for (const q of seg.quads) {
        for (const tid of [q.s, q.p, q.o]) {
            // Never crash a report over a malformed reference.
            if (tid < 0 || tid >= n) continue;
            const term = seg.terms[tid];
            if (
                term.kind === TermKind.Iri &&
                term.value.startsWith(STREAM_NS)
            ) {
                return [
                    `layout warning: segment uses ${STREAM_NS} vocabulary but does ` +
                        "not claim layout 'streamable' (§13.3)",
                ];
            }
        }
    }
    return [];
}

function cmdVerify(paths: string[]): number {
    if (paths.length === 0) {
        console.error(usage);
        return 2;
    }
    let problems = false;
    for (const path of paths) {
        const fs = ReadFileSegments(load(path));
        printLedger(path, fs);
        if (hasProblems(fs)) problems = true;
        // §14.1: declared-vs-computed profile requirements + layout warnings.
        for (let idx = 0; idx < fs.segments.length; idx++) {
            for (const [msg, isErr] of profileCheck(fs.segments[idx])) {
                const prefix = isErr ? "error" : "warning";
                console.error(`  segment ${idx}: ${prefix}: ${msg}`);
                if (isErr) problems = true;
            }
            for (const msg of streamVocabCheck(fs.segments[idx])) {
                console.error(`  segment ${idx}: warning: ${msg}`);
            }
        }
    }
    return problems ? 1 : 0;
}

/** Rewrite a GTS file into the streamable layout state (§10.1, §14.1). */
function cmdCompact(args: string[]): number {
    let outPath = "";
    let streamable = false;
    let sealOriginal = false;
    let timestamp = "";
    const positional: string[] = [];
    for (let i = 0; i < args.length; i++) {
        const a = args[i];
        switch (a) {
            case "-o":
            case "--out":
                if (i + 1 >= args.length) {
                    console.error(`gts: -o requires a path\n${usage}`);
                    return 2;
                }
                outPath = args[++i];
                break;
            case "--streamable":
                streamable = true;
                break;
            case "--seal-original":
                sealOriginal = true;
                break;
            case "--timestamp":
                if (i + 1 >= args.length) {
                    console.error(
                        `gts: --timestamp requires a value\n${usage}`,
                    );
                    return 2;
                }
                timestamp = args[++i];
                break;
            default:
                positional.push(a);
        }
    }
    if (positional.length !== 1 || !outPath) {
        console.error(usage);
        return 2;
    }
    if (!streamable) {
        // The verb is reserved for layout rewrites; a future --snapshot mode
        // (§10) would land here. Without a mode the request is ambiguous.
        console.error("gts: compact requires --streamable");
        return 2;
    }
    // Default to now-UTC at whole-second precision (ISO 8601, trailing Z).
    const ts = timestamp || new Date().toISOString().slice(0, 19) + "Z";
    let data: Uint8Array;
    try {
        data = compactStreamable(load(positional[0]), {
            timestamp: ts,
            sealOriginal,
        });
    } catch (e) {
        if (e instanceof CompactRefusedError) {
            console.error(`gts: refusing compact: ${e.message}`);
            return 1;
        }
        throw e;
    }
    return writeOut(outPath, data);
}

function cmdLs(paths: string[]): number {
    if (paths.length !== 1) {
        console.error(usage);
        return 2;
    }
    const g = Read(load(paths[0]), true);
    for (const d of g.diagnostics) {
        console.error(`gts: diagnostic ${d.code}: ${d.detail}`);
    }
    for (const b of g.blobs) {
        let mt = blobMT(g, b.digest);
        if (mt === "") mt = "-";
        console.log(
            `${b.digest}  ${String(b.data.length).padStart(10)}  ${mt}`,
        );
    }
    if (g.diagnostics.length > 0 || g.segmentHeads.length === 0) return 1;
    return 0;
}

function cmdExtract(args: string[]): number {
    let outPath = "";
    let mt = "";
    let includeSuppressed = false;
    const positional: string[] = [];
    for (let i = 0; i < args.length; i++) {
        const a = args[i];
        switch (a) {
            case "-o":
            case "--out":
                if (i + 1 >= args.length) {
                    console.error(`gts: -o requires a path\n${usage}`);
                    return 2;
                }
                outPath = args[++i];
                break;
            case "--mt":
                if (i + 1 >= args.length) {
                    console.error(`gts: --mt requires a media type\n${usage}`);
                    return 2;
                }
                mt = args[++i];
                break;
            case "--include-suppressed":
                includeSuppressed = true;
                break;
            default:
                positional.push(a);
        }
    }
    if (positional.length !== 2) {
        console.error(usage);
        return 2;
    }
    const [path, digestArg] = positional;
    const g = Read(load(path), true);
    for (const d of g.diagnostics) {
        console.error(`gts: diagnostic ${d.code}: ${d.detail}`);
    }
    if (g.diagnostics.length > 0 || g.segmentHeads.length === 0) {
        console.error("gts: refusing extract: archive did not read cleanly");
        return 1;
    }
    const digest = normalizeDigest(digestArg);
    let blobData: Uint8Array | undefined;
    for (const b of g.blobs) {
        if (b.digest === digest) {
            blobData = b.data;
            break;
        }
    }
    if (!blobData) {
        console.error(`gts: no inline blob ${digest} in ${path}`);
        return 1;
    }
    if (!includeSuppressed && suppressedBlobDigests(g).has(digest)) {
        console.error(
            `gts: refusing ${digest}: suppressed (§11); pass --include-suppressed to extract anyway`,
        );
        return 1;
    }
    if (mt) {
        const declared = blobMT(g, digest);
        if (declared !== mt) {
            console.error(
                `gts: refusing ${digest}: declared media type '${declared}' does not match asserted '${mt}'`,
            );
            return 1;
        }
    }
    return writeOut(outPath, blobData);
}

function cmdCat(args: string[]): number {
    let outPath = "";
    const inputs: string[] = [];
    for (let i = 0; i < args.length; i++) {
        if (args[i] === "-o") {
            if (i + 1 >= args.length) {
                console.error(`gts: -o requires a path\n${usage}`);
                return 2;
            }
            outPath = args[++i];
        } else {
            inputs.push(args[i]);
        }
    }
    if (inputs.length < 2) {
        console.error(`gts: cat needs at least two inputs\n${usage}`);
        return 2;
    }
    const chunks: Uint8Array[] = [];
    for (const path of inputs) {
        const data = load(path);
        const fs = ReadFileSegments(data);
        if (hasProblems(fs)) {
            console.error(`gts: refusing ${path}: not a clean GTS input`);
            printLedger(path, fs);
            return 1;
        }
        for (let idx = 0; idx < fs.segments.length; idx++) {
            const seg = fs.segments[idx];
            const contributes =
                seg.quads.length > 0 ||
                seg.blobs.length > 0 ||
                seg.reifiers.length > 0 ||
                seg.annotations.length > 0 ||
                seg.suppressions.length > 0;
            if (!contributes) {
                console.error(
                    `gts: refusing ${path}: segment ${idx} folds to nothing (no quads/blobs/reifies/annot/suppress)`,
                );
                return 1;
            }
        }
        chunks.push(data);
    }
    const totalLength = chunks.reduce((sum, c) => sum + c.length, 0);
    const combined = new Uint8Array(totalLength);
    let offset = 0;
    for (const chunk of chunks) {
        combined.set(chunk, offset);
        offset += chunk.length;
    }
    const folded = Read(combined, true);
    if (allQuadsSuppressed(folded)) {
        console.error(
            "gts: refusing composition: suppressions hide every quad in the folded output",
        );
        return 1;
    }
    return writeOut(outPath, combined);
}

function cmdPack(args: string[]): number {
    let outPath = "";
    const sources: string[] = [];
    for (let i = 0; i < args.length; i++) {
        const a = args[i];
        switch (a) {
            case "-o":
            case "--out":
                if (i + 1 >= args.length) {
                    console.error(`gts: -o requires a path\n${usage}`);
                    return 2;
                }
                outPath = args[++i];
                break;
            default:
                sources.push(a);
        }
    }
    if (sources.length === 0) {
        console.error(usage);
        return 2;
    }
    if (!outPath) {
        console.error(`gts: pack requires -o\n${usage}`);
        return 2;
    }
    let data: Uint8Array;
    try {
        data = pack(sources);
    } catch (e) {
        console.error(`gts: refusing pack: ${(e as Error).message}`);
        return 1;
    }
    return writeOut(outPath, data);
}

function cmdUnpack(args: string[]): number {
    let dest = "";
    let includeSuppressed = false;
    const positional: string[] = [];
    for (let i = 0; i < args.length; i++) {
        const a = args[i];
        switch (a) {
            case "-C":
                if (i + 1 >= args.length) {
                    console.error(`gts: -C requires a directory\n${usage}`);
                    return 2;
                }
                dest = args[++i];
                break;
            case "--include-suppressed":
                includeSuppressed = true;
                break;
            default:
                positional.push(a);
        }
    }
    if (positional.length !== 1) {
        console.error(usage);
        return 2;
    }
    const path = positional[0];
    const g = Read(load(path), true);
    for (const d of g.diagnostics) {
        console.error(`gts: diagnostic ${d.code}: ${d.detail}`);
    }
    if (g.diagnostics.length > 0 || g.segmentHeads.length === 0) {
        console.error("gts: refusing unpack: archive did not read cleanly");
        return 1;
    }
    try {
        unpack(g, dest || ".", includeSuppressed);
    } catch (e) {
        console.error(`gts: refusing unpack: ${(e as Error).message}`);
        return 1;
    }
    return 0;
}

function cmdDiff(args: string[]): number {
    if (args.length !== 2) {
        console.error(usage);
        return 2;
    }
    const [archive, directory] = args;
    const g = Read(load(archive), true);
    for (const d of g.diagnostics) {
        console.error(`gts: diagnostic ${d.code}: ${d.detail}`);
    }
    if (g.diagnostics.length > 0 || g.segmentHeads.length === 0) {
        console.error("gts: refusing diff: archive did not read cleanly");
        return 1;
    }
    let lines: string[];
    try {
        lines = diff(g, directory);
    } catch (e) {
        console.error(`gts: refusing diff: ${(e as Error).message}`);
        return 1;
    }
    for (const line of lines) console.log(line);
    return lines.length > 0 ? 1 : 0;
}

function main(argv: string[]): number {
    if (argv.length === 0) {
        console.log(usage);
        return 2;
    }
    const cmd = argv[0];
    const args = argv.slice(1);
    switch (cmd) {
        case "info":
            return cmdInfo(args);
        case "fold":
            return cmdFold(args);
        case "verify":
            return cmdVerify(args);
        case "ls":
            return cmdLs(args);
        case "extract":
            return cmdExtract(args);
        case "cat":
            return cmdCat(args);
        case "compact":
            return cmdCompact(args);
        case "pack":
            return cmdPack(args);
        case "unpack":
            return cmdUnpack(args);
        case "diff":
            return cmdDiff(args);
        case "-h":
        case "--help":
        case "help":
            console.log(usage);
            return 0;
        default:
            console.error(`gts: unknown command '${cmd}'\n${usage}`);
            return 2;
    }
}

process.exitCode = main(process.argv.slice(2));
