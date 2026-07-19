// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Practical drive: render a Mermaid flowchart to SVG with Scrybe provenance and
//! show that the original source is recoverable from the `<metadata>`.
//!
//! Run: `cargo run -p scrybe-mermaid-render --example provenance_drive`

fn main() {
    let source = "graph TD; A[Start]-->B{OK?}; B-->|yes|C[Ship]; B-->|no|A;";
    let svg = scrybe_mermaid_render::render_svg_with_source(source).expect("render");

    println!("source        : {source}");
    println!(
        "sha256        : {}",
        scrybe_mermaid_render::source_sha256(source)
    );
    println!("svg bytes     : {}", svg.len());
    println!("has <svg> root: {}", svg.contains("<svg"));
    println!("has <metadata>: {}", svg.contains("<metadata"));

    // Recover the embedded source from the metadata (proves round-trippability).
    let start = svg
        .find("<scrybe:source>")
        .map(|i| i + "<scrybe:source>".len());
    let end = svg.find("</scrybe:source>");
    if let (Some(s), Some(e)) = (start, end) {
        println!("recovered src : {}", &svg[s..e]);
    } else {
        println!("recovered src : <none found>");
    }

    // Rasterize to PNG and write it out so it can be opened / inspected.
    let png = scrybe_mermaid_render::render_png(source).expect("render_png");
    let mut out = std::env::temp_dir();
    out.push("scrybe-provenance-drive.png");
    std::fs::write(&out, &png).expect("write png");
    println!("png bytes     : {}", png.len());
    println!("png magic ok  : {}", png.starts_with(b"\x89PNG\r\n\x1a\n"));
    println!("png written   : {}", out.display());
}
