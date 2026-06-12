// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { Graph, TermKind } from "./model.js";

const RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

function escape(lex: string): string {
    let out = "";
    for (const ch of lex) {
        switch (ch) {
            case "\\":
                out += "\\\\";
                break;
            case '"':
                out += '\\"';
                break;
            case "\n":
                out += "\\n";
                break;
            case "\r":
                out += "\\r";
                break;
            case "\t":
                out += "\\t";
                break;
            default: {
                const code = ch.codePointAt(0) ?? 0;
                if (code < 0x20) {
                    out += `\\u${code.toString(16).padStart(4, "0").toUpperCase()}`;
                } else {
                    out += ch;
                }
            }
        }
    }
    return out;
}

function render(g: Graph, tid: number): string {
    if (tid < 0 || tid >= g.terms.length) {
        return `_:out_of_range_${tid}`;
    }
    const t = g.terms[tid];
    switch (t.kind) {
        case TermKind.Iri:
            return `<${t.value}>`;
        case TermKind.Bnode:
            if (t.value !== "") return `_:${t.value}`;
            return `_:b${tid}`;
        case TermKind.Literal: {
            let lit = `"${escape(t.value)}"`;
            if (t.lang) lit += `@${t.lang}`;
            else if (t.datatype !== undefined)
                lit += `^^${render(g, t.datatype)}`;
            return lit;
        }
        case TermKind.Triple:
            if (t.reifier !== undefined) {
                const spo = g.reifier(t.reifier);
                if (spo) {
                    return `<<( ${render(g, spo.s)} ${render(g, spo.p)} ${render(g, spo.o)} )>>`;
                }
            }
            return `_:unbound_triple_${tid}`;
    }
}

/** Serialise a folded Graph to N-Quads text. */
export function toNQuads(g: Graph): string {
    const lines: string[] = [];
    for (const q of g.quads) {
        const triple = `${render(g, q.s)} ${render(g, q.p)} ${render(g, q.o)}`;
        if (q.g !== undefined) {
            lines.push(`${triple} ${render(g, q.g)} .`);
        } else {
            lines.push(`${triple} .`);
        }
    }
    for (const r of g.reifiers) {
        const quoted = `<<( ${render(g, r.spo.s)} ${render(g, r.spo.p)} ${render(g, r.spo.o)} )>>`;
        lines.push(`${render(g, r.rid)} <${RDF_REIFIES}> ${quoted} .`);
    }
    for (const a of g.annotations) {
        lines.push(`${render(g, a.s)} ${render(g, a.p)} ${render(g, a.o)} .`);
    }
    if (lines.length === 0) return "";
    return lines.join("\n") + "\n";
}
