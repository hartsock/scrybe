// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Built-in tool groups. Each module contributes one or more [`crate::ToolSpec`]s
//! via a `spec()` (or `specs()`) constructor; [`register_defaults`] wires them
//! into the [`crate::Registry`]. Feature-gated groups (`vcs`, `swarm`) will be
//! added here behind `#[cfg(feature = ...)]` in later phases.

use crate::Registry;

pub mod editor;
pub mod lint;
pub mod mermaid;
pub mod render;
pub mod tabs;

/// Register every built-in tool into `reg`.
pub(crate) fn register_defaults(reg: &mut Registry) {
    reg.register(render::spec());
    reg.register(lint::spec());
    reg.register(mermaid::spec());
    reg.register(crate::figures::spec());
    reg.register(tabs::spec());
    for spec in editor::specs() {
        reg.register(spec);
    }
}
