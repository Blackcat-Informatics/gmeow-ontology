/* tslint:disable */
/* eslint-disable */

/**
 * An immutable JSON-LD 1.1 context compiled once and reusable across datasets.
 */
export class CompiledJsonLdContext {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Return the recursively canonical context document as JSON.
     */
    canonicalContextJson(): string;
    /**
     * Compile the context branch of a versioned JSON-LD options document.
     */
    constructor(options_json: string);
}

/**
 * An RDF/JS `DataFactory`. Stateless except for the auto-generated blank-node
 * counter (`blankNode()` with no argument mints a fresh label).
 */
export class DataFactory {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `blankNode(value?)` → a `BlankNode` term; a fresh label is minted when omitted.
     */
    blankNode(value?: string | null): Term;
    /**
     * `defaultGraph()` → the `DefaultGraph` term.
     */
    defaultGraph(): Term;
    /**
     * `directionalLiteral(value, language, direction)` → an RDF-1.2 base-direction
     * literal (`direction` is `"ltr"` or `"rtl"`). The deliberate extension to stock
     * RDF/JS — no incumbent library carries base direction.
     */
    directionalLiteral(value: string, language: string, direction: string): Term;
    /**
     * `fromQuad(original)` → a copy of `original`.
     */
    fromQuad(original: Quad): Quad;
    /**
     * `fromTerm(original)` → a copy of `original` (RDF/JS structural clone).
     */
    fromTerm(original: Term): Term;
    /**
     * `literal(value, language?)` → a plain (`xsd:string`) or language-tagged literal.
     *
     * The RDF/JS spec's unified `literal(value, languageOrDatatype)` — where the second
     * argument may be a string *or* a `NamedNode` — is presented by the TypeScript
     * wrapper, which dispatches the `NamedNode` case to [`DataFactory::typed_literal`].
     * (A `#[wasm_bindgen]`-exported type cannot be recovered from an untyped `JsValue`
     * in Rust, so the polymorphism lives one layer out, in JS.) For base-direction
     * literals (RDF 1.2) use [`DataFactory::directional_literal`].
     */
    literal(value: string, language?: string | null): Term;
    /**
     * `namedNode(value)` → a `NamedNode` term.
     */
    namedNode(value: string): Term;
    /**
     * `new DataFactory()` — a fresh factory with its blank-node counter at zero.
     */
    constructor();
    /**
     * `quad(subject, predicate, object, graph?)` → a `Quad`. The graph defaults to the
     * default graph. A quoted-triple term (from [`DataFactory::quoted_triple`]) may be
     * passed as `subject` or `object` (the RDF-1.2 wedge).
     */
    quad(subject: Term, predicate: Term, object: Term, graph?: Term | null): Quad;
    /**
     * `quotedTriple(subject, predicate, object)` → a quoted-triple `Term`
     * (`termType: "Quad"`) — the RDF-1.2 wedge. Embed it by passing it as the
     * `subject`/`object` of another quad.
     */
    quotedTriple(subject: Term, predicate: Term, object: Term): Term;
    /**
     * `typedLiteral(value, datatype)` → a datatyped literal. `datatype` must be a
     * `NamedNode`. (The RDF/JS `literal(value, datatype)` form, surfaced by the TS
     * wrapper.)
     */
    typedLiteral(value: string, datatype: Term): Term;
    /**
     * `variable(value)` → a `Variable` term.
     */
    variable(value: string): Term;
}

/**
 * An RDF/JS `DatasetCore` backed by the engine's COW mutable dataset.
 */
export class Dataset {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `add(quad)` → insert a quad. Returns `true` if the effective set changed.
     */
    add(quad: Quad): boolean;
    /**
     * `canonicalize()` → the dataset as canonical, flat N-Quads under RDFC-1.0
     * (SHA-256).
     *
     * The deterministic identity string for the graph: two datasets denote the same
     * RDF graph (under blank-node relabeling) iff their canonical forms are
     * byte-identical. This is the same RDFC-1.0 output the conformance gate pins.
     */
    canonicalize(): string;
    /**
     * `delete(quad)` → remove a quad. Returns `true` if the effective set changed.
     */
    delete(quad: Quad): boolean;
    /**
     * `has(quad)` → whether the quad is in the dataset.
     */
    has(quad: Quad): boolean;
    /**
     * `isomorphic(other)` → whether this dataset and `other` are the same RDF graph
     * under blank-node relabeling.
     *
     * The formal RDF graph-identity check, backed by full RDFC-1.0 canonicalization:
     * an exact oracle with no false positives or false negatives. Equivalent to
     * comparing the two [`canonicalize`](Self::canonicalize) strings, but avoids
     * materializing them for obviously-different inputs.
     */
    isomorphic(other: Dataset): boolean;
    /**
     * `match(subject?, predicate?, object?, graph?)` → a new dataset of the matching
     * quads. An omitted (`undefined`) position is a wildcard; `defaultGraph()` matches
     * only the default graph, a named node matches that graph.
     */
    match(subject?: Term | null, predicate?: Term | null, object?: Term | null, graph?: Term | null): Dataset;
    /**
     * An empty dataset.
     */
    constructor();
    /**
     * `parse(input, format, base?)` → a dataset of the parsed quads.
     *
     * `format` is a media type or short name
     * (turtle/ntriples/nquads/trig/rdfxml/jsonld/yamlld).
     * Ill-typed literals are preserved verbatim (RDFLib parity), not rejected.
     */
    static parse(input: string, format: string, base?: string | null): Dataset;
    /**
     * Project this dataset into a deterministic graph, tabular, or research-object USTAR package.
     */
    project(profile: string, config_json: string): ProjectionPackage;
    /**
     * Project this dataset plus a canonical payload-only USTAR into an attached RO-Crate.
     */
    projectWithAssets(profile: string, config_json: string, assets_archive: Uint8Array): ProjectionPackage;
    /**
     * `quads()` → every effective quad, as a JS array.
     */
    quads(): Quad[];
    /**
     * `query(sparql, base?)` → run a SPARQL query against this dataset, offline.
     *
     * Returns **SPARQL Results JSON** for SELECT / ASK and **Turtle** for
     * CONSTRUCT / DESCRIBE. A parse error, an evaluation error, or a `SERVICE` /
     * `LOAD` clause (unresolvable in-browser) throws a JsError — never a silent
     * empty result.
     */
    query(sparql: string, base?: string | null): string;
    /**
     * `serialize(format)` → the dataset rendered in `format` (a UTF-8 string).
     *
     * Formats: `turtle` / `ntriples` / `nquads` / `trig` / `rdfxml` / `jsonld`
     * (JSON-LD-star) / `yamlld` (YAML-LD-star), and their media types — all resolved
     * through the one core registry.
     *
     * Object-position quoted-triple terms (RDF-1.2 triple terms) are preserved
     * through N-Quads, JSON-LD, and YAML-LD; the other text syntaxes (Turtle,
     * N-Triples, TriG, RDF/XML) flatten them.
     */
    serialize(format: string): string;
    /**
     * Serialize JSON-LD/YAML-LD using the shared versioned options decoder.
     */
    serializeConfigured(format: string, options_json: string): string;
    /**
     * Serialize JSON-LD/YAML-LD using a reusable compiled context.
     */
    serializeWithContext(format: string, context: CompiledJsonLdContext, yaml_schema_url?: string | null): string;
    /**
     * `visualExportJson(optionsJson?)` -> model, scene, geometry, and index as JSON.
     */
    visualExportJson(options_json?: string | null): string;
    /**
     * `visualModelJson(optionsJson?)` -> the renderer-neutral RDF 1.2 model as JSON.
     */
    visualModelJson(options_json?: string | null): string;
    /**
     * `visualSvgJson(optionsJson?)` -> deterministic SVG and its complete export.
     */
    visualSvgJson(options_json?: string | null): string;
    /**
     * `size` — the number of effective quads.
     */
    readonly size: number;
}

/**
 * Result of lifting a strict carrier package into an in-memory RDF dataset.
 */
export class ProjectionLift {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Move the lifted dataset out of this result. The dataset can be taken once.
     */
    takeDataset(): Dataset | undefined;
    /**
     * Canonical, versioned runtime loss-ledger JSON.
     */
    readonly lossLedgerJson: string;
}

/**
 * A deterministic USTAR projection package and its canonical runtime ledger.
 */
export class ProjectionPackage {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Canonical deterministic USTAR bytes.
     */
    readonly archive: Uint8Array;
    /**
     * Canonical, versioned runtime loss-ledger JSON.
     */
    readonly lossLedgerJson: string;
    /**
     * Stable carrier profile name.
     */
    readonly profile: string;
}

/**
 * An RDF/JS [Quad](https://rdf.js.org/data-model-spec/#quad-interface) — a statement
 * `(subject, predicate, object, graph)` with `termType: "Quad"`.
 */
export class Quad {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * A quoted-triple [`Term`] (`termType: "Quad"`) viewing this quad's `(s, p, o)` —
     * the RDF-1.2 wedge: pass the result as a subject/object to embed it.
     */
    asTerm(): Term;
    /**
     * Structural RDF/JS quad equality.
     */
    equals(other: Quad): boolean;
    /**
     * The graph [`Term`] of the quad (`DefaultGraph` when unnamed).
     */
    readonly graph: Term;
    /**
     * The object [`Term`] of the quad.
     */
    readonly object: Term;
    /**
     * The predicate [`Term`] of the quad.
     */
    readonly predicate: Term;
    /**
     * The subject [`Term`] of the quad.
     */
    readonly subject: Term;
    /**
     * Always `"Quad"` (a Quad is itself an RDF/JS term).
     */
    readonly termType: string;
    /**
     * Empty for a Quad (per RDF/JS).
     */
    readonly value: string;
}

/**
 * A reusable SPARQL engine that keeps the native plan cache alive across calls.
 */
export class QueryEngine {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Run an ASK query and return the boolean result.
     */
    ask(dataset: Dataset, sparql: string, base?: string | null): boolean;
    /**
     * Run a CONSTRUCT query and return its result dataset.
     */
    construct(dataset: Dataset, sparql: string, base?: string | null): Dataset;
    /**
     * Run a DESCRIBE query and return its result dataset.
     */
    describe(dataset: Dataset, sparql: string, base?: string | null): Dataset;
    /**
     * Create a reusable offline SPARQL engine.
     */
    constructor();
    /**
     * Run any SPARQL query and return a typed raw wasm result wrapper.
     */
    query(dataset: Dataset, sparql: string, base?: string | null): QueryResult;
    /**
     * Run any SPARQL query and serialize its raw result.
     */
    queryRaw(dataset: Dataset, sparql: string, base?: string | null, format?: string | null): string;
    /**
     * Serialize a CONSTRUCT/DESCRIBE result with configured JSON-LD/YAML-LD.
     */
    queryRawConfigured(dataset: Dataset, sparql: string, base: string | null | undefined, format: string, options_json: string): string;
    /**
     * Serialize a CONSTRUCT/DESCRIBE result with a reusable compiled context.
     */
    queryRawWithContext(dataset: Dataset, sparql: string, base: string | null | undefined, format: string, context: CompiledJsonLdContext, yaml_schema_url?: string | null): string;
    /**
     * Run a SELECT query and return typed rows.
     */
    select(dataset: Dataset, sparql: string, base?: string | null): SelectResult;
    /**
     * Apply a SPARQL UPDATE atomically to the supplied dataset.
     */
    update(dataset: Dataset, sparql: string, base?: string | null): void;
}

/**
 * A typed SPARQL result returned by the raw wasm binding.
 */
export class QueryResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Move the graph dataset out of this wrapper.
     */
    takeDataset(): Dataset | undefined;
    /**
     * Move the SELECT result out of this wrapper.
     */
    takeSelect(): SelectResult | undefined;
    /**
     * The ASK boolean when `kind === "ask"`, otherwise `undefined`.
     */
    readonly boolean: boolean | undefined;
    /**
     * The result discriminator: `"select"`, `"ask"`, or `"graph"`.
     */
    readonly kind: string;
}

/**
 * A typed SELECT result returned by the raw wasm binding.
 */
export class SelectResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Move the next unconsumed row out of the result.
     */
    nextRow(): SelectRow | undefined;
    /**
     * Move a row out by result index. Each row can be consumed once.
     */
    takeRow(index: number): SelectRow | undefined;
    /**
     * The result discriminator.
     */
    readonly kind: string;
    /**
     * Number of rows that have not yet been consumed.
     */
    readonly remaining: number;
    /**
     * Total number of SELECT rows, including rows already consumed.
     */
    readonly rowCount: number;
    /**
     * Projected variables, in SELECT projection order.
     */
    readonly variables: string[];
}

/**
 * One SELECT binding row.
 */
export class SelectRow {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Return the bound term for a variable name, or `undefined` for unbound/absent.
     */
    get(variable: string): Term | undefined;
    /**
     * Move one value out by projection index, or return `undefined` when the
     * cell is unbound, absent, or was already consumed.
     */
    takeValue(index: number): Term | undefined;
    /**
     * Variables projected by this row, in SELECT projection order.
     */
    readonly variables: string[];
}

/**
 * An RDF/JS `Sink` — a streaming consumer that interns pushed quads through the
 * `purrdf-events` protocol and freezes them at `finish()`.
 */
export class Sink {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `finish()` — run the protocol's forward-reference resolution and return the
     * resulting dataset. The sink is consumed; further `push`/`finish` is an error.
     */
    finish(): Dataset;
    /**
     * `new Sink()` — an empty sink ready to accept quads via `push`.
     */
    constructor();
    /**
     * `push(quad)` — stream one quad into the sink (interned via the event protocol).
     */
    push(quad: Quad): void;
}

/**
 * An RDF/JS [Term](https://rdf.js.org/data-model-spec/#term-interface).
 */
export class Term {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Structural RDF/JS term equality.
     */
    equals(other: Term): boolean;
    /**
     * `datatype` — the literal's datatype as a `NamedNode`, or `undefined` for a
     * non-literal.
     */
    readonly datatype: Term | undefined;
    /**
     * `direction` — the RDF-1.2 base direction (`"ltr"`/`"rtl"`), or `""` when absent.
     * The deliberate extension to stock RDF/JS (`.goals`: overcome, don't inherit).
     */
    readonly direction: string;
    /**
     * `graph` of a quoted-triple term — always the default graph (a quoted triple has
     * no graph slot), else `undefined`.
     */
    readonly graph: Term | undefined;
    /**
     * `language` — the literal's language tag, or `""` for a non-language-tagged term
     * (RDF/JS uses the empty string, not `undefined`).
     */
    readonly language: string;
    /**
     * `object` of a quoted-triple term, else `undefined`.
     */
    readonly object: Term | undefined;
    /**
     * `predicate` of a quoted-triple term as a `NamedNode`, else `undefined`.
     */
    readonly predicate: Term | undefined;
    /**
     * `subject` of a quoted-triple term (`termType: "Quad"`), else `undefined`.
     */
    readonly subject: Term | undefined;
    /**
     * `termType` — the RDF/JS discriminator.
     */
    readonly termType: string;
    /**
     * `value` — the IRI, blank label, lexical form, or variable name. Empty for a
     * quoted triple and the default graph (per RDF/JS).
     */
    readonly value: string;
}

/**
 * Lift a strict bidirectional USTAR package into an in-memory RDF dataset.
 */
export function liftProjection(archive: Uint8Array, profile: string, config_json: string): ProjectionLift;

/**
 * `shaclEntail(shapesTtl, dataNt)` → the materialized dataset as an N-Triples
 * string (the base graph plus every inferred triple).
 *
 * `shapesTtl` is a Turtle shapes graph; `dataNt` is an N-Triples data graph.
 * Throws (rejects) if either graph fails to parse or if rule application fails.
 */
export function shaclEntail(shapes_ttl: string, data_nt: string): string;

/**
 * `shaclValidateToSarif(shapesTtl, dataNt)` → a SARIF 2.1.0 JSON string.
 *
 * `shapesTtl` is a Turtle shapes graph; `dataNt` is an N-Triples data graph.
 * Throws (rejects) if either graph fails to parse.
 */
export function shaclValidateToSarif(shapes_ttl: string, data_nt: string): string;

/**
 * The purrdf engine version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build + the npm package: importing `purrdf` and
 * calling `version()` proves the module instantiated and the engine linked.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_compiledjsonldcontext_free: (a: number, b: number) => void;
    readonly __wbg_datafactory_free: (a: number, b: number) => void;
    readonly __wbg_dataset_free: (a: number, b: number) => void;
    readonly __wbg_projectionlift_free: (a: number, b: number) => void;
    readonly __wbg_projectionpackage_free: (a: number, b: number) => void;
    readonly __wbg_quad_free: (a: number, b: number) => void;
    readonly __wbg_queryengine_free: (a: number, b: number) => void;
    readonly __wbg_queryresult_free: (a: number, b: number) => void;
    readonly __wbg_selectresult_free: (a: number, b: number) => void;
    readonly __wbg_selectrow_free: (a: number, b: number) => void;
    readonly __wbg_sink_free: (a: number, b: number) => void;
    readonly __wbg_term_free: (a: number, b: number) => void;
    readonly compiledjsonldcontext_canonicalContextJson: (a: number, b: number) => void;
    readonly compiledjsonldcontext_new: (a: number, b: number, c: number) => void;
    readonly datafactory_blankNode: (a: number, b: number, c: number) => number;
    readonly datafactory_defaultGraph: (a: number) => number;
    readonly datafactory_directionalLiteral: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly datafactory_fromQuad: (a: number, b: number) => number;
    readonly datafactory_fromTerm: (a: number, b: number) => number;
    readonly datafactory_literal: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly datafactory_namedNode: (a: number, b: number, c: number) => number;
    readonly datafactory_new: () => number;
    readonly datafactory_quad: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly datafactory_quotedTriple: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly datafactory_typedLiteral: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly datafactory_variable: (a: number, b: number, c: number) => number;
    readonly dataset_add: (a: number, b: number, c: number) => void;
    readonly dataset_canonicalize: (a: number, b: number) => void;
    readonly dataset_delete: (a: number, b: number, c: number) => void;
    readonly dataset_has: (a: number, b: number, c: number) => void;
    readonly dataset_isomorphic: (a: number, b: number, c: number) => void;
    readonly dataset_match: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly dataset_new: (a: number) => void;
    readonly dataset_parse: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly dataset_project: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly dataset_projectWithAssets: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly dataset_quads: (a: number, b: number) => void;
    readonly dataset_query: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly dataset_serialize: (a: number, b: number, c: number, d: number) => void;
    readonly dataset_serializeConfigured: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly dataset_serializeWithContext: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly dataset_size: (a: number) => number;
    readonly dataset_visualExportJson: (a: number, b: number, c: number, d: number) => void;
    readonly dataset_visualModelJson: (a: number, b: number, c: number, d: number) => void;
    readonly dataset_visualSvgJson: (a: number, b: number, c: number, d: number) => void;
    readonly liftProjection: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly projectionlift_lossLedgerJson: (a: number, b: number) => void;
    readonly projectionlift_takeDataset: (a: number) => number;
    readonly projectionpackage_archive: (a: number, b: number) => void;
    readonly projectionpackage_lossLedgerJson: (a: number, b: number) => void;
    readonly projectionpackage_profile: (a: number, b: number) => void;
    readonly quad_asTerm: (a: number, b: number) => void;
    readonly quad_equals: (a: number, b: number) => number;
    readonly quad_graph: (a: number) => number;
    readonly quad_object: (a: number) => number;
    readonly quad_predicate: (a: number) => number;
    readonly quad_subject: (a: number) => number;
    readonly quad_term_type: (a: number, b: number) => void;
    readonly quad_value: (a: number, b: number) => void;
    readonly queryengine_ask: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly queryengine_construct: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly queryengine_new: () => number;
    readonly queryengine_query: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly queryengine_queryRaw: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly queryengine_queryRawConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly queryengine_queryRawWithContext: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => void;
    readonly queryengine_select: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly queryengine_update: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly queryresult_boolean: (a: number) => number;
    readonly queryresult_kind: (a: number, b: number) => void;
    readonly queryresult_takeDataset: (a: number) => number;
    readonly queryresult_takeSelect: (a: number) => number;
    readonly selectresult_kind: (a: number, b: number) => void;
    readonly selectresult_nextRow: (a: number) => number;
    readonly selectresult_remaining: (a: number) => number;
    readonly selectresult_row_count: (a: number) => number;
    readonly selectresult_takeRow: (a: number, b: number) => number;
    readonly selectresult_variables: (a: number, b: number) => void;
    readonly selectrow_get: (a: number, b: number, c: number) => number;
    readonly selectrow_takeValue: (a: number, b: number) => number;
    readonly selectrow_variables: (a: number, b: number) => void;
    readonly shaclEntail: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly shaclValidateToSarif: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sink_finish: (a: number, b: number) => void;
    readonly sink_new: () => number;
    readonly sink_push: (a: number, b: number, c: number) => void;
    readonly term_datatype: (a: number) => number;
    readonly term_direction: (a: number, b: number) => void;
    readonly term_equals: (a: number, b: number) => number;
    readonly term_graph: (a: number) => number;
    readonly term_language: (a: number, b: number) => void;
    readonly term_object: (a: number) => number;
    readonly term_predicate: (a: number) => number;
    readonly term_subject: (a: number) => number;
    readonly term_term_type: (a: number, b: number) => void;
    readonly term_value: (a: number, b: number) => void;
    readonly version: (a: number) => void;
    readonly queryengine_describe: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
