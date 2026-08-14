/* @ts-self-types="./purrdf_wasm.d.ts" */

/**
 * An immutable JSON-LD 1.1 context compiled once and reusable across datasets.
 */
export class CompiledJsonLdContext {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        CompiledJsonLdContextFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_compiledjsonldcontext_free(ptr, 0);
    }
    /**
     * Return the recursively canonical context document as JSON.
     * @returns {string}
     */
    canonicalContextJson() {
        let deferred2_0;
        let deferred2_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.compiledjsonldcontext_canonicalContextJson(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr1 = r0;
            var len1 = r1;
            if (r3) {
                ptr1 = 0; len1 = 0;
                throw takeObject(r2);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Compile the context branch of a versioned JSON-LD options document.
     * @param {string} options_json
     */
    constructor(options_json) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(options_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            wasm.compiledjsonldcontext_new(retptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            CompiledJsonLdContextFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) CompiledJsonLdContext.prototype[Symbol.dispose] = CompiledJsonLdContext.prototype.free;

/**
 * An RDF/JS `DataFactory`. Stateless except for the auto-generated blank-node
 * counter (`blankNode()` with no argument mints a fresh label).
 */
export class DataFactory {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DataFactoryFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_datafactory_free(ptr, 0);
    }
    /**
     * `blankNode(value?)` → a `BlankNode` term; a fresh label is minted when omitted.
     * @param {string | null} [value]
     * @returns {Term}
     */
    blankNode(value) {
        var ptr0 = isLikeNone(value) ? 0 : passStringToWasm0(value, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        var len0 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_blankNode(this.__wbg_ptr, ptr0, len0);
        return Term.__wrap(ret);
    }
    /**
     * `defaultGraph()` → the `DefaultGraph` term.
     * @returns {Term}
     */
    defaultGraph() {
        const ret = wasm.datafactory_defaultGraph(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * `directionalLiteral(value, language, direction)` → an RDF-1.2 base-direction
     * literal (`direction` is `"ltr"` or `"rtl"`). The deliberate extension to stock
     * RDF/JS — no incumbent library carries base direction.
     * @param {string} value
     * @param {string} language
     * @param {string} direction
     * @returns {Term}
     */
    directionalLiteral(value, language, direction) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(value, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(language, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            const ptr2 = passStringToWasm0(direction, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len2 = WASM_VECTOR_LEN;
            wasm.datafactory_directionalLiteral(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Term.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `fromQuad(original)` → a copy of `original`.
     * @param {Quad} original
     * @returns {Quad}
     */
    fromQuad(original) {
        _assertClass(original, Quad);
        const ret = wasm.datafactory_fromQuad(this.__wbg_ptr, original.__wbg_ptr);
        return Quad.__wrap(ret);
    }
    /**
     * `fromTerm(original)` → a copy of `original` (RDF/JS structural clone).
     * @param {Term} original
     * @returns {Term}
     */
    fromTerm(original) {
        _assertClass(original, Term);
        const ret = wasm.datafactory_fromTerm(this.__wbg_ptr, original.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * `literal(value, language?)` → a plain (`xsd:string`) or language-tagged literal.
     *
     * The RDF/JS spec's unified `literal(value, languageOrDatatype)` — where the second
     * argument may be a string *or* a `NamedNode` — is presented by the TypeScript
     * wrapper, which dispatches the `NamedNode` case to [`DataFactory::typed_literal`].
     * (A `#[wasm_bindgen]`-exported type cannot be recovered from an untyped `JsValue`
     * in Rust, so the polymorphism lives one layer out, in JS.) For base-direction
     * literals (RDF 1.2) use [`DataFactory::directional_literal`].
     * @param {string} value
     * @param {string | null} [language]
     * @returns {Term}
     */
    literal(value, language) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(language) ? 0 : passStringToWasm0(language, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_literal(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return Term.__wrap(ret);
    }
    /**
     * `namedNode(value)` → a `NamedNode` term.
     * @param {string} value
     * @returns {Term}
     */
    namedNode(value) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_namedNode(this.__wbg_ptr, ptr0, len0);
        return Term.__wrap(ret);
    }
    /**
     * `new DataFactory()` — a fresh factory with its blank-node counter at zero.
     */
    constructor() {
        const ret = wasm.datafactory_new();
        this.__wbg_ptr = ret;
        DataFactoryFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * `quad(subject, predicate, object, graph?)` → a `Quad`. The graph defaults to the
     * default graph. A quoted-triple term (from [`DataFactory::quoted_triple`]) may be
     * passed as `subject` or `object` (the RDF-1.2 wedge).
     * @param {Term} subject
     * @param {Term} predicate
     * @param {Term} object
     * @param {Term | null} [graph]
     * @returns {Quad}
     */
    quad(subject, predicate, object, graph) {
        _assertClass(subject, Term);
        _assertClass(predicate, Term);
        _assertClass(object, Term);
        let ptr0 = 0;
        if (!isLikeNone(graph)) {
            _assertClass(graph, Term);
            ptr0 = graph.__destroy_into_raw();
        }
        const ret = wasm.datafactory_quad(this.__wbg_ptr, subject.__wbg_ptr, predicate.__wbg_ptr, object.__wbg_ptr, ptr0);
        return Quad.__wrap(ret);
    }
    /**
     * `quotedTriple(subject, predicate, object)` → a quoted-triple `Term`
     * (`termType: "Quad"`) — the RDF-1.2 wedge. Embed it by passing it as the
     * `subject`/`object` of another quad.
     * @param {Term} subject
     * @param {Term} predicate
     * @param {Term} object
     * @returns {Term}
     */
    quotedTriple(subject, predicate, object) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(subject, Term);
            _assertClass(predicate, Term);
            _assertClass(object, Term);
            wasm.datafactory_quotedTriple(retptr, this.__wbg_ptr, subject.__wbg_ptr, predicate.__wbg_ptr, object.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Term.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `typedLiteral(value, datatype)` → a datatyped literal. `datatype` must be a
     * `NamedNode`. (The RDF/JS `literal(value, datatype)` form, surfaced by the TS
     * wrapper.)
     * @param {string} value
     * @param {Term} datatype
     * @returns {Term}
     */
    typedLiteral(value, datatype) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(value, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            _assertClass(datatype, Term);
            wasm.datafactory_typedLiteral(retptr, this.__wbg_ptr, ptr0, len0, datatype.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Term.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `variable(value)` → a `Variable` term.
     * @param {string} value
     * @returns {Term}
     */
    variable(value) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_variable(this.__wbg_ptr, ptr0, len0);
        return Term.__wrap(ret);
    }
}
if (Symbol.dispose) DataFactory.prototype[Symbol.dispose] = DataFactory.prototype.free;

/**
 * An RDF/JS `DatasetCore` backed by the engine's COW mutable dataset.
 */
export class Dataset {
    static __wrap(ptr) {
        const obj = Object.create(Dataset.prototype);
        obj.__wbg_ptr = ptr;
        DatasetFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DatasetFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_dataset_free(ptr, 0);
    }
    /**
     * `add(quad)` → insert a quad. Returns `true` if the effective set changed.
     * @param {Quad} quad
     * @returns {boolean}
     */
    add(quad) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(quad, Quad);
            wasm.dataset_add(retptr, this.__wbg_ptr, quad.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `canonicalize()` → the dataset as canonical, flat N-Quads under RDFC-1.0
     * (SHA-256).
     *
     * The deterministic identity string for the graph: two datasets denote the same
     * RDF graph (under blank-node relabeling) iff their canonical forms are
     * byte-identical. This is the same RDFC-1.0 output the conformance gate pins.
     * @returns {string}
     */
    canonicalize() {
        let deferred2_0;
        let deferred2_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.dataset_canonicalize(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr1 = r0;
            var len1 = r1;
            if (r3) {
                ptr1 = 0; len1 = 0;
                throw takeObject(r2);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * `delete(quad)` → remove a quad. Returns `true` if the effective set changed.
     * @param {Quad} quad
     * @returns {boolean}
     */
    delete(quad) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(quad, Quad);
            wasm.dataset_delete(retptr, this.__wbg_ptr, quad.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `has(quad)` → whether the quad is in the dataset.
     * @param {Quad} quad
     * @returns {boolean}
     */
    has(quad) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(quad, Quad);
            wasm.dataset_has(retptr, this.__wbg_ptr, quad.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `isomorphic(other)` → whether this dataset and `other` are the same RDF graph
     * under blank-node relabeling.
     *
     * The formal RDF graph-identity check, backed by full RDFC-1.0 canonicalization:
     * an exact oracle with no false positives or false negatives. Equivalent to
     * comparing the two [`canonicalize`](Self::canonicalize) strings, but avoids
     * materializing them for obviously-different inputs.
     * @param {Dataset} other
     * @returns {boolean}
     */
    isomorphic(other) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(other, Dataset);
            wasm.dataset_isomorphic(retptr, this.__wbg_ptr, other.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `match(subject?, predicate?, object?, graph?)` → a new dataset of the matching
     * quads. An omitted (`undefined`) position is a wildcard; `defaultGraph()` matches
     * only the default graph, a named node matches that graph.
     * @param {Term | null} [subject]
     * @param {Term | null} [predicate]
     * @param {Term | null} [object]
     * @param {Term | null} [graph]
     * @returns {Dataset}
     */
    match(subject, predicate, object, graph) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            let ptr0 = 0;
            if (!isLikeNone(subject)) {
                _assertClass(subject, Term);
                ptr0 = subject.__destroy_into_raw();
            }
            let ptr1 = 0;
            if (!isLikeNone(predicate)) {
                _assertClass(predicate, Term);
                ptr1 = predicate.__destroy_into_raw();
            }
            let ptr2 = 0;
            if (!isLikeNone(object)) {
                _assertClass(object, Term);
                ptr2 = object.__destroy_into_raw();
            }
            let ptr3 = 0;
            if (!isLikeNone(graph)) {
                _assertClass(graph, Term);
                ptr3 = graph.__destroy_into_raw();
            }
            wasm.dataset_match(retptr, this.__wbg_ptr, ptr0, ptr1, ptr2, ptr3);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Dataset.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * An empty dataset.
     */
    constructor() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.dataset_new(retptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            DatasetFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `parse(input, format, base?)` → a dataset of the parsed quads.
     *
     * `format` is a media type or short name
     * (turtle/ntriples/nquads/trig/rdfxml/jsonld/yamlld).
     * Ill-typed literals are preserved verbatim (RDFLib parity), not rejected.
     * @param {string} input
     * @param {string} format
     * @param {string | null} [base]
     * @returns {Dataset}
     */
    static parse(input, format, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            var ptr2 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len2 = WASM_VECTOR_LEN;
            wasm.dataset_parse(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Dataset.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Project this dataset into a deterministic graph, tabular, or research-object USTAR package.
     * @param {string} profile
     * @param {string} config_json
     * @returns {ProjectionPackage}
     */
    project(profile, config_json) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(profile, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            wasm.dataset_project(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return ProjectionPackage.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Project this dataset plus a canonical payload-only USTAR into an attached RO-Crate.
     * @param {string} profile
     * @param {string} config_json
     * @param {Uint8Array} assets_archive
     * @returns {ProjectionPackage}
     */
    projectWithAssets(profile, config_json, assets_archive) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(profile, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            const ptr2 = passArray8ToWasm0(assets_archive, wasm.__wbindgen_export2);
            const len2 = WASM_VECTOR_LEN;
            wasm.dataset_projectWithAssets(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return ProjectionPackage.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `quads()` → every effective quad, as a JS array.
     * @returns {Quad[]}
     */
    quads() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.dataset_quads(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayJsValueFromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `query(sparql, base?)` → run a SPARQL query against this dataset, offline.
     *
     * Returns **SPARQL Results JSON** for SELECT / ASK and **Turtle** for
     * CONSTRUCT / DESCRIBE. A parse error, an evaluation error, or a `SERVICE` /
     * `LOAD` clause (unresolvable in-browser) throws a JsError — never a silent
     * empty result.
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {string}
     */
    query(sparql, base) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.dataset_query(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
        }
    }
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
     * @param {string} format
     * @returns {string}
     */
    serialize(format) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            wasm.dataset_serialize(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Serialize JSON-LD/YAML-LD using the shared versioned options decoder.
     * @param {string} format
     * @param {string} options_json
     * @returns {string}
     */
    serializeConfigured(format, options_json) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(options_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            wasm.dataset_serializeConfigured(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * Serialize JSON-LD/YAML-LD using a reusable compiled context.
     * @param {string} format
     * @param {CompiledJsonLdContext} context
     * @param {string | null} [yaml_schema_url]
     * @returns {string}
     */
    serializeWithContext(format, context, yaml_schema_url) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            _assertClass(context, CompiledJsonLdContext);
            var ptr1 = isLikeNone(yaml_schema_url) ? 0 : passStringToWasm0(yaml_schema_url, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.dataset_serializeWithContext(retptr, this.__wbg_ptr, ptr0, len0, context.__wbg_ptr, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * `size` — the number of effective quads.
     * @returns {number}
     */
    get size() {
        const ret = wasm.dataset_size(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * `visualExportJson(optionsJson?)` -> model, scene, geometry, and index as JSON.
     * @param {string | null} [options_json]
     * @returns {string}
     */
    visualExportJson(options_json) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            var ptr0 = isLikeNone(options_json) ? 0 : passStringToWasm0(options_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len0 = WASM_VECTOR_LEN;
            wasm.dataset_visualExportJson(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * `visualModelJson(optionsJson?)` -> the renderer-neutral RDF 1.2 model as JSON.
     * @param {string | null} [options_json]
     * @returns {string}
     */
    visualModelJson(options_json) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            var ptr0 = isLikeNone(options_json) ? 0 : passStringToWasm0(options_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len0 = WASM_VECTOR_LEN;
            wasm.dataset_visualModelJson(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * `visualSvgJson(optionsJson?)` -> deterministic SVG and its complete export.
     * @param {string | null} [options_json]
     * @returns {string}
     */
    visualSvgJson(options_json) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            var ptr0 = isLikeNone(options_json) ? 0 : passStringToWasm0(options_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len0 = WASM_VECTOR_LEN;
            wasm.dataset_visualSvgJson(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
        }
    }
}
if (Symbol.dispose) Dataset.prototype[Symbol.dispose] = Dataset.prototype.free;

/**
 * Result of lifting a strict carrier package into an in-memory RDF dataset.
 */
export class ProjectionLift {
    static __wrap(ptr) {
        const obj = Object.create(ProjectionLift.prototype);
        obj.__wbg_ptr = ptr;
        ProjectionLiftFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ProjectionLiftFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_projectionlift_free(ptr, 0);
    }
    /**
     * Canonical, versioned runtime loss-ledger JSON.
     * @returns {string}
     */
    get lossLedgerJson() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.projectionlift_lossLedgerJson(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Move the lifted dataset out of this result. The dataset can be taken once.
     * @returns {Dataset | undefined}
     */
    takeDataset() {
        const ret = wasm.projectionlift_takeDataset(this.__wbg_ptr);
        return ret === 0 ? undefined : Dataset.__wrap(ret);
    }
}
if (Symbol.dispose) ProjectionLift.prototype[Symbol.dispose] = ProjectionLift.prototype.free;

/**
 * A deterministic USTAR projection package and its canonical runtime ledger.
 */
export class ProjectionPackage {
    static __wrap(ptr) {
        const obj = Object.create(ProjectionPackage.prototype);
        obj.__wbg_ptr = ptr;
        ProjectionPackageFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ProjectionPackageFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_projectionpackage_free(ptr, 0);
    }
    /**
     * Canonical deterministic USTAR bytes.
     * @returns {Uint8Array}
     */
    get archive() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.projectionpackage_archive(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Canonical, versioned runtime loss-ledger JSON.
     * @returns {string}
     */
    get lossLedgerJson() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.projectionpackage_lossLedgerJson(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Stable carrier profile name.
     * @returns {string}
     */
    get profile() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.projectionpackage_profile(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) ProjectionPackage.prototype[Symbol.dispose] = ProjectionPackage.prototype.free;

/**
 * An RDF/JS [Quad](https://rdf.js.org/data-model-spec/#quad-interface) — a statement
 * `(subject, predicate, object, graph)` with `termType: "Quad"`.
 */
export class Quad {
    static __wrap(ptr) {
        const obj = Object.create(Quad.prototype);
        obj.__wbg_ptr = ptr;
        QuadFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        QuadFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_quad_free(ptr, 0);
    }
    /**
     * A quoted-triple [`Term`] (`termType: "Quad"`) viewing this quad's `(s, p, o)` —
     * the RDF-1.2 wedge: pass the result as a subject/object to embed it.
     * @returns {Term}
     */
    asTerm() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.quad_asTerm(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Term.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Structural RDF/JS quad equality.
     * @param {Quad} other
     * @returns {boolean}
     */
    equals(other) {
        _assertClass(other, Quad);
        const ret = wasm.quad_equals(this.__wbg_ptr, other.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * The graph [`Term`] of the quad (`DefaultGraph` when unnamed).
     * @returns {Term}
     */
    get graph() {
        const ret = wasm.quad_graph(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * The object [`Term`] of the quad.
     * @returns {Term}
     */
    get object() {
        const ret = wasm.quad_object(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * The predicate [`Term`] of the quad.
     * @returns {Term}
     */
    get predicate() {
        const ret = wasm.quad_predicate(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * The subject [`Term`] of the quad.
     * @returns {Term}
     */
    get subject() {
        const ret = wasm.quad_subject(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * Always `"Quad"` (a Quad is itself an RDF/JS term).
     * @returns {string}
     */
    get termType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.quad_term_type(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Empty for a Quad (per RDF/JS).
     * @returns {string}
     */
    get value() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.quad_value(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Quad.prototype[Symbol.dispose] = Quad.prototype.free;

/**
 * A reusable SPARQL engine that keeps the native plan cache alive across calls.
 */
export class QueryEngine {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        QueryEngineFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_queryengine_free(ptr, 0);
    }
    /**
     * Run an ASK query and return the boolean result.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {boolean}
     */
    ask(dataset, sparql, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.queryengine_ask(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Run a CONSTRUCT query and return its result dataset.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {Dataset}
     */
    construct(dataset, sparql, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.queryengine_construct(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Dataset.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Run a DESCRIBE query and return its result dataset.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {Dataset}
     */
    describe(dataset, sparql, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.queryengine_describe(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Dataset.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Create a reusable offline SPARQL engine.
     */
    constructor() {
        const ret = wasm.queryengine_new();
        this.__wbg_ptr = ret;
        QueryEngineFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Run any SPARQL query and return a typed raw wasm result wrapper.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {QueryResult}
     */
    query(dataset, sparql, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.queryengine_query(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return QueryResult.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Run any SPARQL query and serialize its raw result.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     * @param {string | null} [format]
     * @returns {string}
     */
    queryRaw(dataset, sparql, base, format) {
        let deferred5_0;
        let deferred5_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            var ptr2 = isLikeNone(format) ? 0 : passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len2 = WASM_VECTOR_LEN;
            wasm.queryengine_queryRaw(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr4 = r0;
            var len4 = r1;
            if (r3) {
                ptr4 = 0; len4 = 0;
                throw takeObject(r2);
            }
            deferred5_0 = ptr4;
            deferred5_1 = len4;
            return getStringFromWasm0(ptr4, len4);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred5_0, deferred5_1, 1);
        }
    }
    /**
     * Serialize a CONSTRUCT/DESCRIBE result with configured JSON-LD/YAML-LD.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null | undefined} base
     * @param {string} format
     * @param {string} options_json
     * @returns {string}
     */
    queryRawConfigured(dataset, sparql, base, format, options_json) {
        let deferred6_0;
        let deferred6_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            const ptr2 = passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len2 = WASM_VECTOR_LEN;
            const ptr3 = passStringToWasm0(options_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len3 = WASM_VECTOR_LEN;
            wasm.queryengine_queryRawConfigured(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr5 = r0;
            var len5 = r1;
            if (r3) {
                ptr5 = 0; len5 = 0;
                throw takeObject(r2);
            }
            deferred6_0 = ptr5;
            deferred6_1 = len5;
            return getStringFromWasm0(ptr5, len5);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred6_0, deferred6_1, 1);
        }
    }
    /**
     * Serialize a CONSTRUCT/DESCRIBE result with a reusable compiled context.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null | undefined} base
     * @param {string} format
     * @param {CompiledJsonLdContext} context
     * @param {string | null} [yaml_schema_url]
     * @returns {string}
     */
    queryRawWithContext(dataset, sparql, base, format, context, yaml_schema_url) {
        let deferred6_0;
        let deferred6_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            const ptr2 = passStringToWasm0(format, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len2 = WASM_VECTOR_LEN;
            _assertClass(context, CompiledJsonLdContext);
            var ptr3 = isLikeNone(yaml_schema_url) ? 0 : passStringToWasm0(yaml_schema_url, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len3 = WASM_VECTOR_LEN;
            wasm.queryengine_queryRawWithContext(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, context.__wbg_ptr, ptr3, len3);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr5 = r0;
            var len5 = r1;
            if (r3) {
                ptr5 = 0; len5 = 0;
                throw takeObject(r2);
            }
            deferred6_0 = ptr5;
            deferred6_1 = len5;
            return getStringFromWasm0(ptr5, len5);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred6_0, deferred6_1, 1);
        }
    }
    /**
     * Run a SELECT query and return typed rows.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {SelectResult}
     */
    select(dataset, sparql, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.queryengine_select(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return SelectResult.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Apply a SPARQL UPDATE atomically to the supplied dataset.
     * @param {Dataset} dataset
     * @param {string} sparql
     * @param {string | null} [base]
     */
    update(dataset, sparql, base) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(dataset, Dataset);
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            var len1 = WASM_VECTOR_LEN;
            wasm.queryengine_update(retptr, this.__wbg_ptr, dataset.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) QueryEngine.prototype[Symbol.dispose] = QueryEngine.prototype.free;

/**
 * A typed SPARQL result returned by the raw wasm binding.
 */
export class QueryResult {
    static __wrap(ptr) {
        const obj = Object.create(QueryResult.prototype);
        obj.__wbg_ptr = ptr;
        QueryResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        QueryResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_queryresult_free(ptr, 0);
    }
    /**
     * The ASK boolean when `kind === "ask"`, otherwise `undefined`.
     * @returns {boolean | undefined}
     */
    get boolean() {
        const ret = wasm.queryresult_boolean(this.__wbg_ptr);
        return ret === 0xFFFFFF ? undefined : ret !== 0;
    }
    /**
     * The result discriminator: `"select"`, `"ask"`, or `"graph"`.
     * @returns {string}
     */
    get kind() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.queryresult_kind(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Move the graph dataset out of this wrapper.
     * @returns {Dataset | undefined}
     */
    takeDataset() {
        const ret = wasm.queryresult_takeDataset(this.__wbg_ptr);
        return ret === 0 ? undefined : Dataset.__wrap(ret);
    }
    /**
     * Move the SELECT result out of this wrapper.
     * @returns {SelectResult | undefined}
     */
    takeSelect() {
        const ret = wasm.queryresult_takeSelect(this.__wbg_ptr);
        return ret === 0 ? undefined : SelectResult.__wrap(ret);
    }
}
if (Symbol.dispose) QueryResult.prototype[Symbol.dispose] = QueryResult.prototype.free;

/**
 * A typed SELECT result returned by the raw wasm binding.
 */
export class SelectResult {
    static __wrap(ptr) {
        const obj = Object.create(SelectResult.prototype);
        obj.__wbg_ptr = ptr;
        SelectResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SelectResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_selectresult_free(ptr, 0);
    }
    /**
     * The result discriminator.
     * @returns {string}
     */
    get kind() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.selectresult_kind(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Move the next unconsumed row out of the result.
     * @returns {SelectRow | undefined}
     */
    nextRow() {
        const ret = wasm.selectresult_nextRow(this.__wbg_ptr);
        return ret === 0 ? undefined : SelectRow.__wrap(ret);
    }
    /**
     * Number of rows that have not yet been consumed.
     * @returns {number}
     */
    get remaining() {
        const ret = wasm.selectresult_remaining(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Total number of SELECT rows, including rows already consumed.
     * @returns {number}
     */
    get rowCount() {
        const ret = wasm.selectresult_row_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Move a row out by result index. Each row can be consumed once.
     * @param {number} index
     * @returns {SelectRow | undefined}
     */
    takeRow(index) {
        const ret = wasm.selectresult_takeRow(this.__wbg_ptr, index);
        return ret === 0 ? undefined : SelectRow.__wrap(ret);
    }
    /**
     * Projected variables, in SELECT projection order.
     * @returns {string[]}
     */
    get variables() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.selectresult_variables(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayJsValueFromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) SelectResult.prototype[Symbol.dispose] = SelectResult.prototype.free;

/**
 * One SELECT binding row.
 */
export class SelectRow {
    static __wrap(ptr) {
        const obj = Object.create(SelectRow.prototype);
        obj.__wbg_ptr = ptr;
        SelectRowFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SelectRowFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_selectrow_free(ptr, 0);
    }
    /**
     * Return the bound term for a variable name, or `undefined` for unbound/absent.
     * @param {string} variable
     * @returns {Term | undefined}
     */
    get(variable) {
        const ptr0 = passStringToWasm0(variable, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.selectrow_get(this.__wbg_ptr, ptr0, len0);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * Move one value out by projection index, or return `undefined` when the
     * cell is unbound, absent, or was already consumed.
     * @param {number} index
     * @returns {Term | undefined}
     */
    takeValue(index) {
        const ret = wasm.selectrow_takeValue(this.__wbg_ptr, index);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * Variables projected by this row, in SELECT projection order.
     * @returns {string[]}
     */
    get variables() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.selectrow_variables(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayJsValueFromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) SelectRow.prototype[Symbol.dispose] = SelectRow.prototype.free;

/**
 * An RDF/JS `Sink` — a streaming consumer that interns pushed quads through the
 * `purrdf-events` protocol and freezes them at `finish()`.
 */
export class Sink {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SinkFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_sink_free(ptr, 0);
    }
    /**
     * `finish()` — run the protocol's forward-reference resolution and return the
     * resulting dataset. The sink is consumed; further `push`/`finish` is an error.
     * @returns {Dataset}
     */
    finish() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.sink_finish(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Dataset.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * `new Sink()` — an empty sink ready to accept quads via `push`.
     */
    constructor() {
        const ret = wasm.sink_new();
        this.__wbg_ptr = ret;
        SinkFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * `push(quad)` — stream one quad into the sink (interned via the event protocol).
     * @param {Quad} quad
     */
    push(quad) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(quad, Quad);
            wasm.sink_push(retptr, this.__wbg_ptr, quad.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) Sink.prototype[Symbol.dispose] = Sink.prototype.free;

/**
 * An RDF/JS [Term](https://rdf.js.org/data-model-spec/#term-interface).
 */
export class Term {
    static __wrap(ptr) {
        const obj = Object.create(Term.prototype);
        obj.__wbg_ptr = ptr;
        TermFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TermFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_term_free(ptr, 0);
    }
    /**
     * `datatype` — the literal's datatype as a `NamedNode`, or `undefined` for a
     * non-literal.
     * @returns {Term | undefined}
     */
    get datatype() {
        const ret = wasm.term_datatype(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `direction` — the RDF-1.2 base direction (`"ltr"`/`"rtl"`), or `""` when absent.
     * The deliberate extension to stock RDF/JS (`.goals`: overcome, don't inherit).
     * @returns {string}
     */
    get direction() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.term_direction(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Structural RDF/JS term equality.
     * @param {Term} other
     * @returns {boolean}
     */
    equals(other) {
        _assertClass(other, Term);
        const ret = wasm.term_equals(this.__wbg_ptr, other.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * `graph` of a quoted-triple term — always the default graph (a quoted triple has
     * no graph slot), else `undefined`.
     * @returns {Term | undefined}
     */
    get graph() {
        const ret = wasm.term_graph(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `language` — the literal's language tag, or `""` for a non-language-tagged term
     * (RDF/JS uses the empty string, not `undefined`).
     * @returns {string}
     */
    get language() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.term_language(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * `object` of a quoted-triple term, else `undefined`.
     * @returns {Term | undefined}
     */
    get object() {
        const ret = wasm.term_object(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `predicate` of a quoted-triple term as a `NamedNode`, else `undefined`.
     * @returns {Term | undefined}
     */
    get predicate() {
        const ret = wasm.term_predicate(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `subject` of a quoted-triple term (`termType: "Quad"`), else `undefined`.
     * @returns {Term | undefined}
     */
    get subject() {
        const ret = wasm.term_subject(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `termType` — the RDF/JS discriminator.
     * @returns {string}
     */
    get termType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.term_term_type(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * `value` — the IRI, blank label, lexical form, or variable name. Empty for a
     * quoted triple and the default graph (per RDF/JS).
     * @returns {string}
     */
    get value() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.term_value(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Term.prototype[Symbol.dispose] = Term.prototype.free;

/**
 * Lift a strict bidirectional USTAR package into an in-memory RDF dataset.
 * @param {Uint8Array} archive
 * @param {string} profile
 * @param {string} config_json
 * @returns {ProjectionLift}
 */
export function liftProjection(archive, profile, config_json) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passArray8ToWasm0(archive, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(profile, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(config_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len2 = WASM_VECTOR_LEN;
        wasm.liftProjection(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return ProjectionLift.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * `shaclEntail(shapesTtl, dataNt)` → the materialized dataset as an N-Triples
 * string (the base graph plus every inferred triple).
 *
 * `shapesTtl` is a Turtle shapes graph; `dataNt` is an N-Triples data graph.
 * Throws (rejects) if either graph fails to parse or if rule application fails.
 * @param {string} shapes_ttl
 * @param {string} data_nt
 * @returns {string}
 */
export function shaclEntail(shapes_ttl, data_nt) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(shapes_ttl, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(data_nt, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len1 = WASM_VECTOR_LEN;
        wasm.shaclEntail(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr3 = r0;
        var len3 = r1;
        if (r3) {
            ptr3 = 0; len3 = 0;
            throw takeObject(r2);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
    }
}

/**
 * `shaclValidateToSarif(shapesTtl, dataNt)` → a SARIF 2.1.0 JSON string.
 *
 * `shapesTtl` is a Turtle shapes graph; `dataNt` is an N-Triples data graph.
 * Throws (rejects) if either graph fails to parse.
 * @param {string} shapes_ttl
 * @param {string} data_nt
 * @returns {string}
 */
export function shaclValidateToSarif(shapes_ttl, data_nt) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(shapes_ttl, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(data_nt, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len1 = WASM_VECTOR_LEN;
        wasm.shaclValidateToSarif(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr3 = r0;
        var len3 = r1;
        if (r3) {
            ptr3 = 0; len3 = 0;
            throw takeObject(r2);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
    }
}

/**
 * The purrdf engine version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build + the npm package: importing `purrdf` and
 * calling `version()` proves the module instantiated and the engine linked.
 * @returns {string}
 */
export function version() {
    let deferred1_0;
    let deferred1_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.version(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_fdd633d4bb5dd76a: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg___wbindgen_throw_ea4887a5f8f9a9db: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_now_d2e0afbad4edbe82: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_quad_new: function(arg0) {
            const ret = Quad.__wrap(arg0);
            return addHeapObject(ret);
        },
        __wbg_random_3182549db57fb083: function() {
            const ret = Math.random();
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
    };
    return {
        __proto__: null,
        "./purrdf_wasm_bg.js": import0,
    };
}

const CompiledJsonLdContextFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_compiledjsonldcontext_free(ptr, 1));
const DataFactoryFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_datafactory_free(ptr, 1));
const DatasetFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_dataset_free(ptr, 1));
const ProjectionLiftFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_projectionlift_free(ptr, 1));
const ProjectionPackageFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_projectionpackage_free(ptr, 1));
const QuadFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_quad_free(ptr, 1));
const QueryEngineFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_queryengine_free(ptr, 1));
const QueryResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_queryresult_free(ptr, 1));
const SelectResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_selectresult_free(ptr, 1));
const SelectRowFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_selectrow_free(ptr, 1));
const SinkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_sink_free(ptr, 1));
const TermFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_term_free(ptr, 1));

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(takeObject(mem.getUint32(i, true)));
    }
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('purrdf_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
