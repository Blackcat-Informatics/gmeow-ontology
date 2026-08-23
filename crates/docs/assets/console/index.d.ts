// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Types for the package's `.` entry — `element.mjs`. Importing it for its side effect
// registers `<gmeow-console>` with `customElements` (guarded, so a second import is a
// no-op). The DOM-free session surface has its own entry, `./session.mjs`.

/** One node of a derivation DAG, as `derivationStructure` returns it. */
export interface DerivationNode {
  id: string;
  label: string;
}

/** One cover edge of a derivation DAG or a Hasse diagram. */
export interface DerivationEdge {
  from: string;
  to: string;
}

/** An anchor cluster: the anchor and the calls that hang off it. */
export interface AnchorCluster {
  anchor: string;
  members: string[];
}

/**
 * The derivation structure of a recorded session, read out of its N-Quads: the DAG, its
 * minimal fatal cut, the anchor clusters, and any Belnap gluts.
 */
export interface DerivationStructure {
  nodes: DerivationNode[];
  edges: DerivationEdge[];
  cut: string[];
  anchors: AnchorCluster[];
  gluts: string[];
}

/**
 * `<gmeow-console>` — the whole standalone console as one custom element.
 *
 * Shadow DOM with co-located styles, its own engine worker, and a pane set DERIVED from
 * the shipped action policy rather than listed here. Registered on import.
 */
export class GmeowConsole extends HTMLElement {
  connectedCallback(): void;
  disconnectedCallback(): void;
}

/**
 * A layered Hasse diagram as inline SVG. Nodes are ranked by longest-path depth from a
 * minimal element, so a cover edge always points upward and the drawing is a genuine
 * order diagram rather than a spring layout.
 */
export function hasseSvg(nodes: DerivationNode[], edges: DerivationEdge[]): SVGSVGElement;

/** The derivation structure of a recorded session, from its N-Quads text. */
export function derivationStructure(nquads: string): DerivationStructure;
