// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/** An IRI term, declared as one by the caller. */
export interface IriTerm {
  iri: string;
}

/** A literal term, declared as one by the caller. At most one of `datatype`/`language`. */
export interface LiteralTerm {
  literal: string;
  datatype?: string;
  language?: string;
}

/** A blank node, declared as one by the caller. */
export interface BlankNodeTerm {
  bnode: string;
}

/** An RDF-1.2 triple term (`<<( s p o )>>`) over three declared components. */
export interface TripleTermValue {
  triple: [DeclaredTerm, DeclaredTerm, DeclaredTerm];
}

/**
 * One term in a declared position.
 *
 * The kind is DECLARED, never inferred: `{iri}` is an IRI, `{literal}` is a literal,
 * `{bnode}` is a blank node, `{triple}` is an RDF-1.2 triple term, and a bare string is the
 * shorthand for a plain literal. There is deliberately no rule that reads a term's kind out
 * of its text — a prose answer that quotes a URL is a literal, and a `urn:`/`did:` IRI is
 * an IRI, and only the caller knows which it produced.
 */
export type DeclaredTerm = IriTerm | LiteralTerm | BlankNodeTerm | TripleTermValue | string;

/** A term that may occupy the subject position: an IRI or a blank node, never a literal. */
export type SubjectTerm = IriTerm | BlankNodeTerm;

/** One `{subject, predicate, object}` statement, every term declared. */
export interface DeclaredStatement {
  subject: SubjectTerm;
  predicate: IriTerm;
  object: DeclaredTerm;
}

/**
 * One antecedent a derived statement rests on.
 *
 * A term names an entity the conclusion was derived from; `{statement}` names a QUAD — the
 * shape a proof tree hands back, where the premise is itself a statement. A statement
 * antecedent is reified through its own RDF-1.2 triple term and cited by that reifier, so a
 * reader recovers the premise rather than a name for it.
 */
export type Antecedent = DeclaredTerm | { statement: DeclaredStatement };

/** One result statement a recorded invocation produced. */
export interface DerivedStatement extends DeclaredStatement {
  antecedents?: Antecedent[];
}

/** One recorded invocation, as `ConsoleSession.record` returns it. */
export interface RecordedCall {
  index: number;
  iri: string;
  tool: string;
  schema: string;
  args: Record<string, unknown>;
  result: unknown;
  derived: DerivedStatement[];
  judgment: DeclaredStatement[];
  atTime: string;
  storeSegment: string;
}

/** The arguments `ConsoleSession.record` takes. */
export interface RecordInput {
  tool: string;
  schema: string;
  args?: Record<string, unknown>;
  result?: unknown;
  /** The result statements the call derived, read off its answer by `derivationsFrom`. */
  derived?: DerivedStatement[];
  /** The engine's own judgment about its derivation record, recorded verbatim. */
  judgment?: DeclaredStatement[];
  storeSegment?: string | null;
}

/** A decoded permalink payload: the invocation list only, never the results. */
export interface DecodedPermalink {
  v: 1;
  id: string;
  calls: Array<{ tool: string; schema: string; args: Record<string, unknown> }>;
}

/** The example base every console-minted IRI lives under. Never a `gmeow:` term. */
export const SESSION_BASE: string;

/** The single temporal frame a console session stamps on every recorded call. */
export const SESSION_TEMPORAL_FRAME: string;

/** The named graph the exported session store segment rides in. */
export const SESSION_STORE_GRAPH: string;

/** URL-safe base64 of a UTF-8 string, without padding. */
export function base64UrlEncode(text: string): string;

/** The inverse of `base64UrlEncode`. */
export function base64UrlDecode(text: string): string;

/** The console's content address of a string — `fnv1a128:<32 hex>`. */
export function contentAddress(text: string): string;

/**
 * Decode a permalink fragment back into its invocation list. A digest mismatch is a HARD
 * failure naming both addresses — a tampered or truncated permalink is never replayed on
 * a best-effort basis.
 */
export function decodePermalink(fragment: string): DecodedPermalink;

/** The engine's store reading: its own serialization, and which reads found real state. */
export interface StoreReading {
  /** The engine's serialization of its claim store, as `store_segment` returned it. */
  nquads: string;
  /** The tools that reported actual stored state (`store_segment`, `list_candidates`). */
  heldBy: string[];
  /** The subset of `heldBy` whose state `nquads` actually carries. */
  carriedBy: string[];
}

/**
 * The engine's store reading, taken off the `store_segment` and `list_candidates` results.
 * Mints nothing: the RDF shape of a stored claim belongs to the engine's store, and
 * `store_segment` is the one tool that serializes it.
 */
export function storeReading(store: unknown, candidates: unknown): StoreReading;

/**
 * The exportable `.gts` segment text for a session: the trajectory in the default graph
 * plus, when the store holds state, the engine's claim/candidate store in the
 * `SESSION_STORE_GRAPH` named graph.
 *
 * The `store` reading is required — an export that never asked the store what it held
 * cannot know whether it dropped anything. An EMPTY store is not an error: the export
 * succeeds carrying the trajectory alone, and emits no store graph rather than an empty
 * one. A holder in `heldBy` that is not in `carriedBy` IS a hard failure, naming the tools
 * whose state cannot be carried.
 */
export function exportSegment(session: ConsoleSession, store: StoreReading): string;

/** Rebuild a session from a permalink fragment (identity round trip for the invocations). */
export function sessionFromPermalink(
  fragment: string,
  options?: { now?: (index: number) => string },
): ConsoleSession;

/** An RDF-1.2 triple term (`<<( s p o )>>`) over three already-serialized terms. */
export function tripleTerm(subject: string, predicate: string, object: string): string;

/** One console session: an ordered run of recorded invocations under ONE trajectory anchor. */
export class ConsoleSession {
  constructor(options?: { id?: string; now?: (index: number) => string });
  id: string;
  now: (index: number) => string;
  calls: RecordedCall[];
  anchor: string;
  startState: string;
  frame: string;
  /** The IRI of the `n`-th recorded call. Content-addressed over what the call IS. */
  callIri(index: number, tool: string, args: Record<string, unknown>): string;
  /** Record one invocation. `schema` is REQUIRED — an unbound call is invisible to the auditor. */
  record(input: RecordInput): RecordedCall;
  /** The recorded trajectory as N-Quads, in the exact shape the shipped auditor discovers. */
  trajectoryNQuads(): string;
  /** The reifier IRI of one statement — content-addressed over the statement itself. */
  statementIri(subject: string, predicate: string, object: string): string;
  /** The RDF-1.2 quoted-triple annotations for one recorded call. */
  annotationsFor(call: RecordedCall): string[];
  /** One antecedent as the term `gmeow:wasDerivedFrom` points at, plus the lines it needs. */
  citeAntecedent(antecedent: Antecedent, where: string): { term: string; lines: string[] };
  /** `<content-address>.<base64url payload>` over the invocation list only. */
  permalink(): string;
}
