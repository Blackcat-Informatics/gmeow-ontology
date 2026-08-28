// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The tool/resource extension seam and the TOTAL dispatch surface it assembles.
//!
//! [`McpServer`] serves the consumer surface out of the bundled
//! `gmeow.gts` snapshot alone. A host crate that has more than the bundle — today
//! `gmeow-mcp-dev`, which has a checkout — adds its tools from OUTSIDE by handing an
//! [`Extension`] to [`McpServer::from_snapshot_with`](crate::McpServer::from_snapshot_with).
//! This crate therefore never names a dev tool, never carries a `root` path, and
//! never depends on the build executor.
//!
//! # Why a registration is a PAIR
//!
//! A registration is a `(descriptor, handler)` pair, never one or the other. The
//! descriptor is the JSON the client sees in `tools/list` / `resources/list`; the
//! handler is what `tools/call` / `resources/read` runs. Binding them at the
//! registration site makes "advertised" and "dispatchable" the same fact, so the
//! classic MCP wiring bug — a tool advertised in the list that no dispatch arm
//! handles, or a dispatch arm for a tool that is never advertised — cannot be
//! expressed.
//!
//! The consumer builtins keep their descriptor list and their handler list separate
//! (the descriptors are a ~400-line literal; interleaving 31 closures into it would
//! make it unreadable), so they are joined by [`zip_tools`] / [`zip_resources`],
//! which REFUSE to build unless the two lists are in bijection *at the same index*.
//! The invariant is the same; only its proof differs.
//!
//! # Totality
//!
//! [`Surface`] is the assembled result: an ordered registration list plus a
//! name → index map. Dispatch is a map lookup with exactly one failure mode —
//! [`UnknownTool`] /
//! [`UnknownResource`], naming the key. There is no
//! fallthrough arm, no `if mode.is_dev()` guard, and no silent no-op: a name the
//! surface does not carry is refused, and a name it carries always runs its handler.
//!
//! Assembly is likewise total: a key claimed twice (by two extension entries, or by
//! an extension entry shadowing a builtin) is
//! [`DuplicateRegistration`] and the server
//! refuses to construct, because last-writer-wins would let the advertised
//! descriptor and the dispatched handler silently disagree.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::McpServer;
use crate::error::{DuplicateRegistration, InvalidRegistration, UnknownResource, UnknownTool};

/// What a `tools/call` runs: the server, the raw argument object, the JSON text the
/// tool returns. Every consumer tool and every host-registered tool has this shape.
pub type ToolHandler =
    Box<dyn Fn(&McpServer, &Value) -> gmeow_errors::Result<String> + Send + Sync>;

/// What a `resources/read` runs: the server and the resolved language-tag preference
/// list (the `?lang=` query, or the server's startup default), returning the resource
/// body. The MIME type is NOT returned — it is read from the descriptor, so the
/// advertised media type and the served media type are one fact.
pub type ResourceHandler =
    Box<dyn Fn(&McpServer, &[String]) -> gmeow_errors::Result<String> + Send + Sync>;

/// One tool: the `tools/list` descriptor and the `tools/call` handler, inseparable.
pub struct ToolRegistration {
    descriptor: Value,
    handler: ToolHandler,
}

impl ToolRegistration {
    /// Bind a `tools/list` descriptor to its `tools/call` handler.
    pub fn new<F>(descriptor: Value, handler: F) -> Self
    where
        F: Fn(&McpServer, &Value) -> gmeow_errors::Result<String> + Send + Sync + 'static,
    {
        Self {
            descriptor,
            handler: Box::new(handler),
        }
    }

    /// The advertised tool name — the dispatch key. A descriptor without a string
    /// `name` is an [`InvalidRegistration`]: it could be advertised but never called.
    fn name(&self) -> gmeow_errors::Result<&str> {
        self.descriptor
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(InvalidRegistration {
                    message: format!(
                        "tool descriptor carries no string `name`: {}",
                        self.descriptor
                    ),
                })
            })
    }
}

/// One resource: the `resources/list` descriptor and the `resources/read` handler.
pub struct ResourceRegistration {
    descriptor: Value,
    handler: ResourceHandler,
}

impl ResourceRegistration {
    /// Bind a `resources/list` descriptor to its `resources/read` handler.
    pub fn new<F>(descriptor: Value, handler: F) -> Self
    where
        F: Fn(&McpServer, &[String]) -> gmeow_errors::Result<String> + Send + Sync + 'static,
    {
        Self {
            descriptor,
            handler: Box::new(handler),
        }
    }

    /// The advertised resource URI — the dispatch key.
    fn uri(&self) -> gmeow_errors::Result<&str> {
        self.descriptor
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(InvalidRegistration {
                    message: format!(
                        "resource descriptor carries no string `uri`: {}",
                        self.descriptor
                    ),
                })
            })
    }

    /// The advertised media type, which is ALSO the served media type (one fact).
    fn mime(&self) -> gmeow_errors::Result<&str> {
        self.descriptor
            .get("mimeType")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(InvalidRegistration {
                    message: format!(
                        "resource descriptor carries no string `mimeType`: {}",
                        self.descriptor
                    ),
                })
            })
    }
}

/// A set of tool and resource registrations contributed by ONE source.
///
/// The consumer builtins are one `Extension`; a host crate's additions are another.
/// [`Surface::assemble`] merges them in order, so the advertised list is
/// deterministic (builtins first, host second, each in declaration order).
#[derive(Default)]
pub struct Extension {
    tools: Vec<ToolRegistration>,
    resources: Vec<ResourceRegistration>,
}

impl Extension {
    /// An empty extension — the identity of the merge.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build directly from already-paired registration lists.
    #[must_use]
    pub fn from_parts(tools: Vec<ToolRegistration>, resources: Vec<ResourceRegistration>) -> Self {
        Self { tools, resources }
    }

    /// Register one tool (descriptor + handler), in advertised order.
    #[must_use]
    pub fn with_tool<F>(mut self, descriptor: Value, handler: F) -> Self
    where
        F: Fn(&McpServer, &Value) -> gmeow_errors::Result<String> + Send + Sync + 'static,
    {
        self.tools.push(ToolRegistration::new(descriptor, handler));
        self
    }

    /// Register one resource (descriptor + handler), in advertised order.
    #[must_use]
    pub fn with_resource<F>(mut self, descriptor: Value, handler: F) -> Self
    where
        F: Fn(&McpServer, &[String]) -> gmeow_errors::Result<String> + Send + Sync + 'static,
    {
        self.resources
            .push(ResourceRegistration::new(descriptor, handler));
        self
    }
}

/// Join a descriptor list to a same-order handler list, refusing anything that is
/// not a bijection at the same index.
///
/// # Errors
///
/// [`InvalidRegistration`] if the two lists differ in length, if a descriptor has no
/// string `name`, or if the descriptor and handler at some index name different
/// tools — each of which would leave a tool advertised-but-undispatchable (or the
/// converse).
pub fn zip_tools(
    descriptors: Vec<Value>,
    handlers: Vec<(&'static str, ToolHandler)>,
) -> gmeow_errors::Result<Vec<ToolRegistration>> {
    if descriptors.len() != handlers.len() {
        return Err(gmeow_errors::Diag::of_kind(InvalidRegistration {
            message: format!(
                "{} tool descriptors but {} tool handlers — every advertised tool must be \
                 dispatchable and every handler must be advertised",
                descriptors.len(),
                handlers.len()
            ),
        }));
    }
    descriptors
        .into_iter()
        .zip(handlers)
        .map(|(descriptor, (name, handler))| {
            let registration = ToolRegistration {
                descriptor,
                handler,
            };
            let advertised = registration.name()?;
            if advertised != name {
                return Err(gmeow_errors::Diag::of_kind(InvalidRegistration {
                    message: format!(
                        "tool descriptor `{advertised}` is paired with the handler for `{name}` \
                         — the descriptor list and the handler list must agree index by index"
                    ),
                }));
            }
            Ok(registration)
        })
        .collect()
}

/// The [`zip_tools`] twin for resources, keyed on the descriptor `uri`.
///
/// # Errors
///
/// [`InvalidRegistration`] on a length mismatch, a descriptor without a string `uri`
/// or `mimeType`, or a descriptor/handler pair that names different URIs.
pub fn zip_resources(
    descriptors: Vec<Value>,
    handlers: Vec<(&'static str, ResourceHandler)>,
) -> gmeow_errors::Result<Vec<ResourceRegistration>> {
    if descriptors.len() != handlers.len() {
        return Err(gmeow_errors::Diag::of_kind(InvalidRegistration {
            message: format!(
                "{} resource descriptors but {} resource handlers — every advertised resource \
                 must be readable and every handler must be advertised",
                descriptors.len(),
                handlers.len()
            ),
        }));
    }
    descriptors
        .into_iter()
        .zip(handlers)
        .map(|(descriptor, (uri, handler))| {
            let registration = ResourceRegistration {
                descriptor,
                handler,
            };
            let advertised = registration.uri()?;
            if advertised != uri {
                return Err(gmeow_errors::Diag::of_kind(InvalidRegistration {
                    message: format!(
                        "resource descriptor `{advertised}` is paired with the handler for \
                         `{uri}` — the descriptor list and the handler list must agree index by \
                         index"
                    ),
                }));
            }
            // Reject a descriptor with no media type here, at assembly, rather than at
            // the first read.
            registration.mime()?;
            Ok(registration)
        })
        .collect()
}

/// The assembled, duplicate-free, totally-dispatchable tool and resource surface of
/// one [`McpServer`].
pub struct Surface {
    tools: Vec<ToolRegistration>,
    tool_index: BTreeMap<String, usize>,
    resources: Vec<ResourceRegistration>,
    resource_index: BTreeMap<String, usize>,
}

impl Surface {
    /// Merge the consumer builtins with a host extension into one surface.
    ///
    /// # Errors
    ///
    /// [`InvalidRegistration`] if a descriptor carries no dispatch key, or
    /// [`DuplicateRegistration`] if a tool name or resource URI is claimed twice —
    /// including a host entry that would shadow a builtin.
    pub fn assemble(builtin: Extension, host: Extension) -> gmeow_errors::Result<Self> {
        let mut tools = builtin.tools;
        tools.extend(host.tools);
        let mut tool_index = BTreeMap::new();
        for (position, registration) in tools.iter().enumerate() {
            let name = registration.name()?.to_owned();
            if tool_index.insert(name.clone(), position).is_some() {
                return Err(gmeow_errors::Diag::of_kind(DuplicateRegistration {
                    key: format!("tool `{name}`"),
                }));
            }
        }

        let mut resources = builtin.resources;
        resources.extend(host.resources);
        let mut resource_index = BTreeMap::new();
        for (position, registration) in resources.iter().enumerate() {
            let uri = registration.uri()?.to_owned();
            registration.mime()?;
            if resource_index.insert(uri.clone(), position).is_some() {
                return Err(gmeow_errors::Diag::of_kind(DuplicateRegistration {
                    key: format!("resource `{uri}`"),
                }));
            }
        }

        Ok(Self {
            tools,
            tool_index,
            resources,
            resource_index,
        })
    }

    /// The `tools/list` descriptors, in advertised order.
    pub fn tool_descriptors(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.descriptor.clone()).collect()
    }

    /// The `resources/list` descriptors, in advertised order.
    pub fn resource_descriptors(&self) -> Vec<Value> {
        self.resources
            .iter()
            .map(|r| r.descriptor.clone())
            .collect()
    }

    /// Every advertised tool name, in advertised order. Because a registration is a
    /// pair, this is EXACTLY the set of dispatchable names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools
            .iter()
            .map(|t| {
                t.descriptor
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("assemble() proved every tool descriptor has a string name")
            })
            .collect()
    }

    /// Run the handler registered for `name`.
    ///
    /// # Errors
    ///
    /// [`UnknownTool`], naming `name`, if nothing registered it — the ONLY dispatch
    /// failure mode. Otherwise whatever the handler raises.
    pub fn dispatch_tool(
        &self,
        server: &McpServer,
        name: &str,
        args: &Value,
    ) -> gmeow_errors::Result<String> {
        let position = self.tool_index.get(name).copied().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(UnknownTool {
                name: name.to_owned(),
            })
        })?;
        (self.tools[position].handler)(server, args)
    }

    /// Read the resource registered for `uri`, returning `(mimeType, body)` where the
    /// media type is the advertised one.
    ///
    /// # Errors
    ///
    /// [`UnknownResource`], naming `uri`, if nothing registered it. Otherwise
    /// whatever the handler raises.
    pub fn read_resource(
        &self,
        server: &McpServer,
        uri: &str,
        requested: &[String],
    ) -> gmeow_errors::Result<(String, String)> {
        let position = self.resource_index.get(uri).copied().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(UnknownResource {
                uri: uri.to_owned(),
            })
        })?;
        let registration = &self.resources[position];
        let mime = registration.mime()?.to_owned();
        let body = (registration.handler)(server, requested)?;
        Ok((mime, body))
    }
}
