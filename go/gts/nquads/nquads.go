// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package nquads implements the GTS -> N-Quads transform (§14).
package nquads

import (
	"fmt"
	"strings"

	"go.blackcatinformatics.ca/gts/model"
)

const rdfReifies = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"

func escape(lex string) string {
	var out strings.Builder
	for _, ch := range lex {
		switch ch {
		case '\\':
			out.WriteString("\\\\")
		case '"':
			out.WriteString("\\\"")
		case '\n':
			out.WriteString("\\n")
		case '\r':
			out.WriteString("\\r")
		case '\t':
			out.WriteString("\\t")
		default:
			if ch < 0x20 {
				out.WriteString(fmt.Sprintf("\\u%04X", ch))
			} else {
				out.WriteRune(ch)
			}
		}
	}
	return out.String()
}

func render(g *model.Graph, tid int) string {
	if tid < 0 || tid >= len(g.Terms) {
		return fmt.Sprintf("_:out_of_range_%d", tid)
	}
	t := &g.Terms[tid]
	switch t.Kind {
	case model.Iri:
		return fmt.Sprintf("<%s>", t.Value)
	case model.Bnode:
		if t.Value != "" {
			return fmt.Sprintf("_:%s", t.Value)
		}
		return fmt.Sprintf("_:b%d", tid)
	case model.Literal:
		lit := fmt.Sprintf("\"%s\"", escape(t.Value))
		if t.Lang != "" {
			return fmt.Sprintf("%s@%s", lit, t.Lang)
		}
		if t.Datatype != nil {
			return fmt.Sprintf("%s^^%s", lit, render(g, *t.Datatype))
		}
		return lit
	case model.Triple:
		if t.Reifier != nil {
			if spo, ok := g.Reifier(*t.Reifier); ok {
				return fmt.Sprintf("<<( %s %s %s )>>", render(g, spo.S), render(g, spo.P), render(g, spo.O))
			}
		}
		return fmt.Sprintf("_:unbound_triple_%d", tid)
	}
	return ""
}

// ToNQuads serialises a folded Graph to N-Quads text.
func ToNQuads(g *model.Graph) string {
	var lines []string
	for _, q := range g.Quads {
		triple := fmt.Sprintf("%s %s %s", render(g, q.S), render(g, q.P), render(g, q.O))
		if q.G != nil {
			lines = append(lines, fmt.Sprintf("%s %s .", triple, render(g, *q.G)))
		} else {
			lines = append(lines, fmt.Sprintf("%s .", triple))
		}
	}
	for _, r := range g.Reifiers {
		quoted := fmt.Sprintf("<<( %s %s %s )>>", render(g, r.SPO.S), render(g, r.SPO.P), render(g, r.SPO.O))
		lines = append(lines, fmt.Sprintf("%s <%s> %s .", render(g, r.RID), rdfReifies, quoted))
	}
	for _, a := range g.Annotations {
		lines = append(lines, fmt.Sprintf("%s %s %s .", render(g, a.S), render(g, a.P), render(g, a.O)))
	}
	if len(lines) == 0 {
		return ""
	}
	return strings.Join(lines, "\n") + "\n"
}
