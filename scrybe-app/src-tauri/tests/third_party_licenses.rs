// Third-party license notices must ship with the desktop bundles (#84).
// The app statically links MPL-2.0 crates (cssparser, selectors, dtoa-short,
// option-ext via the tauri/dirs chains), so tauri.conf.json bundles
// `licenses/*` into the app resources. These tests keep that surface honest.

/// The bundled copies must stay byte-identical to the repo-root canonical
/// files — a drift means the shipped notice no longer matches the documented
/// one. `include_str!` also makes a missing file a compile error, so the
/// bundle can never silently lose its notices.
#[test]
fn bundled_licenses_match_repo_root() {
    assert_eq!(
        include_str!("../licenses/LICENSE-MPL-2.0"),
        include_str!("../../../LICENSE-MPL-2.0"),
        "scrybe-app/src-tauri/licenses/LICENSE-MPL-2.0 drifted from the repo-root copy"
    );
    assert_eq!(
        include_str!("../licenses/THIRD-PARTY-LICENSES.md"),
        include_str!("../../../THIRD-PARTY-LICENSES.md"),
        "scrybe-app/src-tauri/licenses/THIRD-PARTY-LICENSES.md drifted from the repo-root copy"
    );
}

/// The notice names four MPL-2.0 crates; if one leaves the dependency tree
/// (or the notice stops naming it) the two must be reconciled together.
#[test]
fn mpl_crates_in_lockfile_are_documented() {
    let lockfile = include_str!("../../../Cargo.lock");
    let notice = include_str!("../../../THIRD-PARTY-LICENSES.md");
    for krate in ["cssparser", "selectors", "dtoa-short", "option-ext"] {
        assert!(
            lockfile.contains(&format!("name = \"{krate}\"")),
            "{krate} no longer in Cargo.lock — update THIRD-PARTY-LICENSES.md to match the tree"
        );
        assert!(
            notice.contains(&format!("`{krate}`")),
            "{krate} missing from THIRD-PARTY-LICENSES.md"
        );
    }
}

/// tauri.conf.json must keep bundling the licenses directory.
#[test]
fn tauri_bundle_includes_licenses() {
    let conf = include_str!("../tauri.conf.json");
    assert!(
        conf.contains("licenses/*"),
        "tauri.conf.json bundle.resources no longer includes licenses/*"
    );
}
