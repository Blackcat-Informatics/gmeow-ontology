// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import {
    mkdirSync,
    readFileSync,
    readdirSync,
    statSync,
    writeFileSync,
    chmodSync,
    utimesSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import * as wire from "./wire.js";
import { Writer, digestString } from "./writer.js";
import { Graph, Quad, Term, TermKind } from "./model.js";

const filesNS = "https://w3id.org/gts/files#";
const rdfType = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const xsdInteger = "http://www.w3.org/2001/XMLSchema#integer";
const xsdDateTime = "http://www.w3.org/2001/XMLSchema#dateTime";

function iriTerm(value: string): Term {
    return { kind: TermKind.Iri, value };
}

function literalTerm(value: string, datatype?: number): Term {
    return { kind: TermKind.Literal, value, datatype };
}

function bnodeTerm(label: string): Term {
    return { kind: TermKind.Bnode, value: label };
}

function safeArchivePath(name: string): void {
    if (name === "") throw new Error("empty archive path");
    if (name.startsWith("/"))
        throw new Error(`absolute path not allowed in archive: ${name}`);
    for (const part of name.split("/")) {
        if (part === "..")
            throw new Error(`path traversal not allowed in archive: ${name}`);
    }
}

function walkDirSorted(dir: string): string[] {
    const out: string[] = [];
    function walk(path: string) {
        const entries = readdirSync(path, { withFileTypes: true });
        entries.sort((a, b) => a.name.localeCompare(b.name));
        for (const ent of entries) {
            const full = join(path, ent.name);
            if (ent.isSymbolicLink()) {
                throw new Error(`symlink not supported: ${full}`);
            }
            if (ent.isDirectory()) {
                walk(full);
            } else if (ent.isFile()) {
                out.push(full);
            }
        }
    }
    walk(dir);
    out.sort();
    return out;
}

function resolveSources(sources: string[]): Array<[string, string]> {
    const entries: Array<[string, string]> = [];
    const seen = new Set<string>();
    for (const src of sources) {
        const info = statSync(src);
        if (info.isDirectory()) {
            const files = walkDirSorted(src);
            for (const fpath of files) {
                const rel = relative(src, fpath);
                const relpath = rel.replaceAll("\\", "/");
                safeArchivePath(relpath);
                if (seen.has(relpath))
                    throw new Error(`duplicate archive path: ${relpath}`);
                seen.add(relpath);
                entries.push([fpath, relpath]);
            }
        } else {
            const name = basename(src);
            safeArchivePath(name);
            if (seen.has(name))
                throw new Error(`duplicate archive path: ${name}`);
            seen.add(name);
            entries.push([src, name]);
        }
    }
    entries.sort((a, b) => a[1].localeCompare(b[1]));
    return entries;
}

function guessMediaType(path: string): string {
    const ext = path.slice(path.lastIndexOf(".")).toLowerCase();
    switch (ext) {
        case ".txt":
            return "text/plain";
        case ".html":
        case ".htm":
            return "text/html";
        case ".json":
            return "application/json";
        case ".xml":
            return "application/xml";
        case ".png":
            return "image/png";
        case ".jpg":
        case ".jpeg":
            return "image/jpeg";
        case ".gif":
            return "image/gif";
        case ".webp":
            return "image/webp";
        case ".pdf":
            return "application/pdf";
        case ".zip":
            return "application/zip";
        case ".gz":
            return "application/gzip";
        case ".tar":
            return "application/x-tar";
        default:
            return "application/octet-stream";
    }
}

function formatDateTime(ts: number): string {
    const d = new Date(ts * 1000);
    return d.toISOString().replace("+00:00", "Z");
}

/** Pack files/directories into a deterministic GTS files-profile archive. */
export function pack(sources: string[]): Uint8Array {
    const w = new Writer("files");

    const shared: Term[] = [
        iriTerm(filesNS + "FileEntry"),
        iriTerm(filesNS + "path"),
        iriTerm(filesNS + "digest"),
        iriTerm(filesNS + "size"),
        iriTerm(filesNS + "mode"),
        iriTerm(filesNS + "modified"),
        iriTerm(filesNS + "mediaType"),
        iriTerm(rdfType),
        iriTerm(xsdInteger),
        iriTerm(xsdDateTime),
    ];
    w.addTerms(shared);
    const fileEntryID = 0;
    const pathID = 1;
    const digestID = 2;
    const sizeID = 3;
    const modeID = 4;
    const modifiedID = 5;
    const mediaTypeID = 6;
    const typeID = 7;
    const xsdIntegerID = 8;
    const xsdDateTimeID = 9;

    const entries = resolveSources(sources);
    const fileTerms: Term[] = [];
    const quads: Quad[] = [];
    const blobs = new Map<string, { data: Uint8Array; mt: string }>();
    const blobOrder: string[] = [];

    for (let idx = 0; idx < entries.length; idx++) {
        const [fpath, relpath] = entries[idx];
        const data = readFileSync(fpath);
        const digest = digestString(data);
        const info = statSync(fpath);
        const size = info.size;
        const mode = info.mode & 0o7777;
        const mtime = Math.floor(info.mtime.getTime() / 1000);
        const mt = guessMediaType(fpath);

        const entryLabel = `f${idx}`;
        const base = shared.length + fileTerms.length;
        fileTerms.push(
            bnodeTerm(entryLabel),
            literalTerm(relpath),
            literalTerm(digest),
            literalTerm(String(size), xsdIntegerID),
            literalTerm(mode.toString(8), xsdIntegerID),
            literalTerm(formatDateTime(mtime), xsdDateTimeID),
            literalTerm(mt),
        );
        const entryID = base;
        quads.push(
            { s: entryID, p: typeID, o: fileEntryID },
            { s: entryID, p: pathID, o: base + 1 },
            { s: entryID, p: digestID, o: base + 2 },
            { s: entryID, p: sizeID, o: base + 3 },
            { s: entryID, p: modeID, o: base + 4 },
            { s: entryID, p: modifiedID, o: base + 5 },
            { s: entryID, p: mediaTypeID, o: base + 6 },
        );
        if (!blobs.has(digest)) {
            blobs.set(digest, { data, mt });
            blobOrder.push(digest);
        }
    }

    if (fileTerms.length > 0) w.addTerms(fileTerms);
    if (quads.length > 0) w.addQuads(quads);
    for (const digest of blobOrder) {
        const b = blobs.get(digest)!;
        w.addBlob(b.data, b.mt);
    }

    return w.toBytes();
}

interface FileEntries {
    [path: string]: { [field: string]: string };
}

function readFileEntries(g: Graph): FileEntries {
    let typeID: number | undefined;
    let fileEntryID: number | undefined;
    const fieldIDs: { [name: string]: number } = {};
    for (let idx = 0; idx < g.terms.length; idx++) {
        const term = g.terms[idx];
        if (term.kind !== TermKind.Iri) continue;
        switch (term.value) {
            case rdfType:
                typeID = idx;
                break;
            case filesNS + "FileEntry":
                fileEntryID = idx;
                break;
            default:
                if (term.value.startsWith(filesNS)) {
                    fieldIDs[term.value.slice(filesNS.length)] = idx;
                }
        }
    }
    if (typeID === undefined) {
        throw new Error("not a files-profile archive: missing rdf:type");
    }
    if (fileEntryID === undefined) {
        throw new Error("not a files-profile archive: missing FileEntry");
    }

    const entries: { [s: number]: { [field: string]: string } } = {};
    const fileEntrySubjects = new Set<number>();
    for (const q of g.quads) {
        if (q.p === typeID && q.o === fileEntryID) {
            fileEntrySubjects.add(q.s);
            if (!entries[q.s]) entries[q.s] = {};
        } else {
            for (const [name, id] of Object.entries(fieldIDs)) {
                if (id === q.p) {
                    if (q.o < 0 || q.o >= g.terms.length) {
                        throw new Error(
                            `invalid term reference ${q.o} for files:${name}`,
                        );
                    }
                    if (!entries[q.s]) entries[q.s] = {};
                    entries[q.s][name] = g.terms[q.o].value;
                }
            }
        }
    }

    const byPath: FileEntries = {};
    for (const [s, entry] of Object.entries(entries)) {
        if (!fileEntrySubjects.has(Number(s))) continue;
        const path = entry.path;
        if (path === undefined) continue;
        if (byPath[path])
            throw new Error(`duplicate files:path in archive: ${path}`);
        byPath[path] = entry;
    }
    return byPath;
}

function normalizeDigest(digest: string): string {
    return digest.startsWith("blake3:") ? digest : "blake3:" + digest;
}

function digestFromValue(v: unknown): string {
    const s = wire.asText(v);
    if (s !== undefined) return normalizeDigest(s);
    const b = wire.asBytes(v);
    if (b) return "blake3:" + wire.hex(b);
    return "";
}

function suppressedBlobDigests(g: Graph): Set<string> {
    const out = new Set<string>();
    for (const sup of g.suppressions) {
        for (const target of sup.targets) {
            if (!(target instanceof Map)) continue;
            let kind = "";
            let digest: string | undefined;
            for (const [k, v] of target) {
                const key = wire.textOr(k, "");
                if (key === "kind") kind = wire.textOr(v, "");
                else if (key === "digest") digest = digestFromValue(v);
            }
            if (kind === "blob" && digest) out.add(digest);
        }
    }
    return out;
}

function destPath(dest: string, archivePath: string): string {
    const normalized = archivePath.replace(/\\/g, "/");
    if (normalized.startsWith("/") || /^[a-zA-Z]:/.test(normalized))
        throw new Error(
            `absolute or drive-relative path not allowed in archive: ${archivePath}`,
        );
    for (const part of normalized.split("/")) {
        if (part === "..")
            throw new Error(`path traversal in archive: ${archivePath}`);
    }
    return resolve(dest, normalized);
}

/** Extract FileEntry quads from a folded graph into dest. */
export function unpack(
    g: Graph,
    dest: string,
    includeSuppressed = false,
): void {
    const entries = readFileEntries(g);
    const blobByDigest = new Map<string, Uint8Array>();
    for (const b of g.blobs) blobByDigest.set(b.digest, b.data);
    const suppressed = includeSuppressed
        ? new Set<string>()
        : suppressedBlobDigests(g);

    mkdirSync(dest, { recursive: true });
    const destCanon = resolve(dest);
    const prefix = destCanon.replace(/\/$/, "") + sep;

    for (const [path, entry] of Object.entries(entries)) {
        const target = destPath(dest, path);
        const digest = entry.digest;
        if (digest === undefined) throw new Error(`missing digest for ${path}`);
        if (suppressed.has(digest)) continue;
        const data = blobByDigest.get(digest);
        if (!data)
            throw new Error(`missing inline blob for ${path}: ${digest}`);
        if (digestString(data) !== digest) {
            throw new Error(`integrity failure for ${path}: ${digest}`);
        }

        const targetCanon = resolve(target);
        if (!targetCanon.startsWith(prefix)) {
            throw new Error(`path escapes destination: ${path}`);
        }
        const parent = dirname(target);
        if (parent !== "" && parent !== ".") {
            mkdirSync(parent, { recursive: true });
        }
        writeFileSync(target, data);

        if (entry.mode) {
            const m = parseInt(entry.mode, 8);
            if (!Number.isNaN(m)) chmodSync(target, m);
        }
        if (entry.modified) {
            const ts = parseDateTime(entry.modified);
            if (ts !== undefined) {
                const d = new Date(ts * 1000);
                utimesSync(target, d, d);
            }
        }
    }
}

function parseDateTime(text: string): number | undefined {
    let d = Date.parse(text);
    if (!Number.isNaN(d)) return Math.floor(d / 1000);
    d = Date.parse(text + "Z");
    if (!Number.isNaN(d)) return Math.floor(d / 1000);
    return undefined;
}

/** Compare an archive to a directory by content digest. */
export function diff(g: Graph, directory: string): string[] {
    const entries = readFileEntries(g);
    const archiveDigests: { [path: string]: string } = {};
    for (const [path, entry] of Object.entries(entries)) {
        archiveDigests[path] = entry.digest;
    }

    statSync(directory);

    const diskDigests: { [path: string]: string } = {};
    const files = walkDirSorted(directory);
    for (const fpath of files) {
        const rel = relative(directory, fpath).replaceAll("\\", "/");
        const data = readFileSync(fpath);
        diskDigests[rel] = digestString(data);
    }

    const lines: string[] = [];
    for (const path of Object.keys(archiveDigests)) {
        if (!(path in diskDigests)) lines.push(`removed: ${path}`);
    }
    for (const path of Object.keys(diskDigests)) {
        if (!(path in archiveDigests)) lines.push(`added: ${path}`);
    }
    for (const [path, ad] of Object.entries(archiveDigests)) {
        const dd = diskDigests[path];
        if (dd !== undefined && ad !== dd) lines.push(`modified: ${path}`);
    }
    lines.sort();
    return lines;
}
