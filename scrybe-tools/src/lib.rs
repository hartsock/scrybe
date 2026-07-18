// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe tools — one [`ToolSpec`] registry shared by the CLI and the MCP server.
//!
//! Foundation crate for the MCP rebuild (issue #122; design
//! `docs/design/mcp-rebuild.md`). A single [`Registry`] is consumed by *both*
//! front ends, so CLI↔MCP parity holds by construction: same handler, different
//! envelope. This first slice is deliberately additive — the core types, the
//! [`Registry`], the headless [`Transport`], and the pure `render` tool.
//! Dispatch-through-`scrybe-rpc`, the remaining tools, and the protocol fixes
//! land in later phases (design §8).

use serde_json::Value;

pub mod lint;
pub mod tools;

/// Tool group — drives progressive disclosure and feature gating (design §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    Core,
    Editor,
    Mermaid,
    Vcs,
    UiParity,
    Swarm,
}

/// Versioned schema for a tool's stable `data` payload. Agents read `data`; they
/// never parse `description` prose or the human `text` (design §4A).
#[derive(Clone, Copy)]
pub struct DataSchema {
    /// Payload version, bumped when the shape changes so agents can pin.
    pub version: u32,
    /// JSON Schema for the `data` object.
    pub schema: fn() -> Value,
}

/// A business-level failure: the tool ran and said "no" (e.g. "heading not
/// found"). This is DATA carried inside the outcome, not an engine fault —
/// the MCP `isError` flag stays `false`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolError {
    /// Stable machine code, e.g. `"heading_not_found"`.
    pub code: String,
    /// Human/agent-readable explanation.
    pub message: String,
}

impl ToolError {
    /// Convenience constructor.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// The result of a *successful* tool invocation. A `Some(tool_error)` still
/// means the call succeeded — the tool told the agent "no". Engine faults are
/// the separate [`EngineFault`] type returned by the dispatcher.
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    /// Typed, versioned payload, serialized under `data` on every surface.
    pub data: Value,
    /// Business failure, if any. `None` == the tool did its job.
    pub tool_error: Option<ToolError>,
}

impl ToolOutcome {
    /// A successful outcome with no business error.
    pub fn ok(data: Value) -> Self {
        Self {
            data,
            tool_error: None,
        }
    }

    /// A successful call that carries a business failure.
    pub fn fail(data: Value, error: ToolError) -> Self {
        Self {
            data,
            tool_error: Some(error),
        }
    }

    /// True when there is no business error.
    pub fn is_ok(&self) -> bool {
        self.tool_error.is_none()
    }
}

/// One tool, shared verbatim by the CLI and the MCP server (design §2.2).
pub struct ToolSpec {
    /// Wire name, e.g. `"render"`. Also the CLI subcommand stem.
    pub name: &'static str,
    /// Human/agent-facing description. This is ALSO the embedded agent prompt —
    /// it carries behavioral guidance, not just a label — and is rendered into
    /// MCP `tools/list` and `scrybe <cmd> --help` verbatim.
    pub description: &'static str,
    /// JSON Schema for arguments (MCP `inputSchema`; also drives CLI arg parse).
    pub input_schema: fn() -> Value,
    /// Versioned, typed schema for the tool's stable `data` payload.
    pub data_schema: DataSchema,
    /// Does this tool change editor/disk/app state? Gates read-only agents and
    /// dry-run mode.
    pub mutates: bool,
    /// Tool group for progressive disclosure + feature gating.
    pub facet: Facet,
    /// The one implementation, shared by both front ends.
    pub handler: fn(&Ctx, &Value) -> ToolOutcome,
}

/// Engine fault: the dispatcher could not even run the tool (unknown tool, bad
/// arguments, transport down). Surfaces as MCP `isError: true` / a non-zero CLI
/// exit — distinct from a business [`ToolError`] carried inside `data`
/// (design §2.2, §5).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EngineFault {
    /// No tool with this name is registered.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    /// Arguments failed validation before the handler ran.
    #[error("invalid arguments: {0}")]
    BadArgs(String),
    /// The transport to the live app failed.
    #[error("transport error: {0}")]
    Transport(String),
}

/// Failure talking to the live app over the socket.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// No Scrybe app is running to service the request.
    #[error("no Scrybe app is running")]
    NoApp,
    /// An I/O or protocol error on the socket.
    #[error("{0}")]
    Io(String),
}

/// Round-trips scrybe-rpc requests to the live app (design §2.3). The only place
/// a tool touches the outside world — the future `LiveApp` transport and the
/// modulex extraction seam both live behind this trait.
pub trait Transport {
    /// Round-trip a scrybe-rpc `method` with `params` over `~/.scrybe/sock`.
    fn call(&self, method: &str, params: Value) -> Result<Value, TransportError>;
    /// Is a live app currently reachable?
    fn is_live(&self) -> bool;
}

/// No socket: only the pure, GUI-free subset of tools runs. GUI/stateful tools
/// get a clean `NoApp`.
pub struct Headless;

impl Transport for Headless {
    fn call(&self, _method: &str, _params: Value) -> Result<Value, TransportError> {
        Err(TransportError::NoApp)
    }

    fn is_live(&self) -> bool {
        false
    }
}

/// Dials the **live app** over `~/.scrybe/sock` via `scrybe-rpc`'s client — the
/// same wire the CLI uses, so a tool routed through this transport drives the
/// running editor exactly like `scrybe <cmd>` does. When no app is running,
/// `call` returns `NoApp` and pure tools should fall back to `Headless`.
pub struct LiveApp;

impl Transport for LiveApp {
    fn call(&self, method: &str, params: Value) -> Result<Value, TransportError> {
        match scrybe_rpc::client::send(method, params) {
            Ok(resp) => match resp.error {
                Some(err) => Err(TransportError::Io(format!("{}: {}", err.code, err.message))),
                None => Ok(resp.result.unwrap_or(Value::Null)),
            },
            // The client says "no Scrybe running" when the socket isn't there.
            Err(e) if e.contains("no Scrybe running") => Err(TransportError::NoApp),
            Err(e) => Err(TransportError::Io(e)),
        }
    }

    fn is_live(&self) -> bool {
        scrybe_rpc::client::is_live()
    }
}

/// Handler execution context — carries the transport a stateful tool would use.
pub struct Ctx {
    /// How stateful tools reach the live app.
    pub transport: Box<dyn Transport>,
}

impl Ctx {
    /// A context with no live app: pure tools only.
    pub fn headless() -> Self {
        Self {
            transport: Box::new(Headless),
        }
    }

    /// A context that dials the live app over `~/.scrybe/sock` (`LiveApp`).
    pub fn live() -> Self {
        Self {
            transport: Box::new(LiveApp),
        }
    }

    /// A context with an explicit transport.
    pub fn with_transport(transport: Box<dyn Transport>) -> Self {
        Self { transport }
    }
}

/// The shared tool registry consumed by both front ends.
pub struct Registry {
    tools: Vec<ToolSpec>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Register a tool. Panics on a duplicate name — a programming error, caught
    /// in tests, never reachable from user input.
    pub fn register(&mut self, spec: ToolSpec) {
        assert!(
            self.get(spec.name).is_none(),
            "duplicate tool registered: {}",
            spec.name
        );
        self.tools.push(spec);
    }

    /// Look up a tool by wire name.
    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// All registered tool names, in registration order.
    pub fn names(&self) -> Vec<&'static str> {
        self.tools.iter().map(|t| t.name).collect()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// True when no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Dispatch a call. An unknown tool or a missing required argument is an
    /// [`EngineFault`] (`Err`); a business failure is
    /// `Ok(ToolOutcome { tool_error: Some(..) })`.
    pub fn call(&self, name: &str, ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
        let spec = self
            .get(name)
            .ok_or_else(|| EngineFault::UnknownTool(name.to_string()))?;
        require_args(&(spec.input_schema)(), args)?;
        Ok((spec.handler)(ctx, args))
    }
}

impl Default for Registry {
    /// The default registry with every built-in tool registered.
    fn default() -> Self {
        let mut reg = Self::new();
        tools::register_defaults(&mut reg);
        reg
    }
}

/// Minimal JSON-Schema `required` check — the dispatcher's argument gate until a
/// full validator lands. A missing required key is an [`EngineFault::BadArgs`].
fn require_args(schema: &Value, args: &Value) -> Result<(), EngineFault> {
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) {
            if args.get(key).is_none() {
                return Err(EngineFault::BadArgs(format!(
                    "missing required argument: {key}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_registry_registers_render() {
        let reg = Registry::default();
        assert!(reg.get("render").is_some());
        assert!(reg.names().contains(&"render"));
        assert!(!reg.is_empty());
    }

    #[test]
    fn unknown_tool_is_engine_fault() {
        let reg = Registry::default();
        let err = reg
            .call("does_not_exist", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert_eq!(err, EngineFault::UnknownTool("does_not_exist".into()));
    }

    #[test]
    fn missing_required_arg_is_bad_args() {
        let reg = Registry::default();
        // `render` requires `source`; omitting it is an engine fault, not a
        // business error — the dispatcher rejects it before the handler runs.
        let err = reg
            .call("render", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert!(
            matches!(err, EngineFault::BadArgs(ref m) if m.contains("source")),
            "expected BadArgs mentioning source, got {err:?}"
        );
    }

    #[test]
    fn headless_transport_has_no_live_app() {
        let ctx = Ctx::headless();
        assert!(!ctx.transport.is_live());
        assert!(ctx.transport.call("open", json!({})).is_err());
    }

    #[test]
    fn live_app_transport_is_wired_and_fails_cleanly_without_an_app() {
        // Robust regardless of whether a dev app happens to be running: the call
        // must never panic, and when no app is reachable it must error (NoApp),
        // never silently succeed.
        let ctx = Ctx::live();
        if !ctx.transport.is_live() {
            assert!(
                ctx.transport.call("state", json!({})).is_err(),
                "no live app → call must error"
            );
        }
    }

    #[test]
    #[should_panic(expected = "duplicate tool")]
    fn duplicate_registration_panics() {
        let mut reg = Registry::new();
        reg.register(tools::render::spec());
        reg.register(tools::render::spec());
    }
}
