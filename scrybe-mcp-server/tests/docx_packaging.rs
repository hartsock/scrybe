use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn read_repo_file(path: &str) -> String {
    fs::read_to_string(repo_root().join(path)).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn metapackage_depends_on_docx_exporter() {
    let pyproject = read_repo_file("scrybe-meta/pyproject.toml");

    // Version-agnostic: a release bump must not break "the metapackage
    // installs the Word exporter". Asserts the pin exists at any lock-step
    // version (the version lives in [workspace.package], per #128).
    assert!(
        pyproject.contains("\"scrybe-plugin-docx =="),
        "scrybe.ai should install the Word exporter"
    );
}

#[test]
fn release_workflow_builds_and_publishes_docx_exporter() {
    let workflow = read_repo_file(".github/workflows/release.yml");

    assert!(
        workflow.contains("build-docx:"),
        "release workflow should build the pure-Python docx package"
    );
    assert!(
        workflow.contains("working-directory: scrybe-plugin-docx"),
        "release workflow should build from the docx package directory"
    );
    assert!(
        workflow.contains("{ name: scrybe-plugin-docx, glob: scrybe_plugin_docx-* }"),
        "release workflow should publish scrybe-plugin-docx to PyPI"
    );
    assert!(
        workflow.contains("needs: [build-wheels, build-sdists, build-meta, build-docx]"),
        "PyPI publication should wait for the docx package artifacts"
    );
}

#[test]
fn local_install_wires_docx_exporter() {
    let justfile = read_repo_file("justfile");

    assert!(
        justfile.contains("install: install-app"),
        "the default install should route through the app install recipe"
    );
    assert!(
        justfile.contains("install-app: app install-python-toolkit"),
        "app install should include the Python runtime tools"
    );
    assert!(
        justfile.contains(
            "cd scrybe-mermaid && VIRTUAL_ENV=\"$HOME/venv\" ~/venv/bin/maturin develop --release"
        ),
        "local install should install the local Mermaid Python binding before docx"
    );
    assert!(
        justfile.contains("cd scrybe-plugin-docx && ~/venv/bin/python -m pip install -e ."),
        "local install should install the docx exporter entry point"
    );
    assert!(
        justfile.contains("cd scrybe-plugin-docx && python -m pip install -e ."),
        "editable install should install the docx exporter entry point"
    );
}
