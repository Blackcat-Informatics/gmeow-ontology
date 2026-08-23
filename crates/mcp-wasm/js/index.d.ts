// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Instantiate the wasm module. Idempotent and single-flighted: concurrent callers share
 * one instantiation. In Node the wasm bytes are read from the colocated file; in a
 * browser pass the bytes/URL, or omit to fetch the colocated `.wasm`. Must be awaited
 * once before any other export is used.
 */
export function ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;

/**
 * Install a `gmeow.gts` snapshot into the engine. Must be called once (after `ready()`)
 * before any frame is dispatched. Throws if the snapshot cannot be read.
 */
export function init(snapshot: Uint8Array): void;

/**
 * Whether a `gmeow.gts` snapshot has been installed by `init`. This is the wasm module's
 * OWN `ready()` export, renamed at this wrapper so `ready()` can keep the sibling shims'
 * instantiation meaning.
 */
export function loaded(): boolean;

/**
 * Dispatch ONE JSON-RPC MCP request frame and return the response frame, as strings.
 * Throws if no snapshot is installed or the frame cannot be processed.
 */
export function mcp(request_json: string): string;

/** The engine version (the crate's SemVer). */
export function version(): string;
