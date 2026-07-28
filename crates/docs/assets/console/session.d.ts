// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/** One result statement a recorded invocation produced. */
export interface DerivedStatement {
  subject: string;
  predicate: string;
  object: string;
  antecedents?: string[];
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
  atTime: string;
  storeSegment: string;
}

/** The arguments `ConsoleSession.record` takes. */
export interface RecordInput {
  tool: string;
  schema: string;
  args?: Record<string, unknown>;
  result?: unknown;
  derived?: DerivedStatement[];
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

/**
 * The exportable `.gts` segment text for a session: the trajectory in the default graph
 * plus the engine's claim/candidate store in the `SESSION_STORE_GRAPH` named graph. An
 * absent `store` is a hard failure — half a session snapshot is not a session snapshot.
 */
export function exportSegment(session: ConsoleSession, store: string): string;

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
  /** The RDF-1.2 quoted-triple annotations for one recorded call. */
  annotationsFor(call: RecordedCall): string;
  /** `<content-address>.<base64url payload>` over the invocation list only. */
  permalink(): string;
}
