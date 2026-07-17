// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Practical drive of the `scrybe-tools` registry: build the default registry,
//! render real Markdown (heading + emphasis + a highlighted code block) through
//! the `render` tool under the headless transport, and print the typed `data`.
//!
//! Run: `cargo run -p scrybe-tools --example render_drive`

use scrybe_tools::{Ctx, Registry};
use serde_json::json;

fn main() {
    let reg = Registry::default();
    println!("registered tools: {:?}", reg.names());

    let md = "# Scrybe\n\nRust **inside**, Python *outside*.\n\n```rust\nfn main() {}\n```\n";
    let outcome = reg
        .call(
            "render",
            &Ctx::headless(),
            &json!({ "source": md, "theme": "dark" }),
        )
        .expect("render should dispatch");

    let data = &outcome.data;
    println!(
        "ok={}  kind={}  v={}  theme={}  bytes={}",
        outcome.is_ok(),
        data["kind"],
        data["v"],
        data["theme"],
        data["bytes"]
    );

    let body = data["body_html"].as_str().unwrap();
    println!("--- body_html (first 500 chars) ---");
    println!("{}", &body[..body.len().min(500)]);

    // A missing required argument is an engine fault, not a silent success.
    match reg.call("render", &Ctx::headless(), &json!({})) {
        Err(e) => println!("\nmissing-arg engine fault (expected): {e}"),
        Ok(_) => panic!("expected an engine fault for missing `source`"),
    }
}
