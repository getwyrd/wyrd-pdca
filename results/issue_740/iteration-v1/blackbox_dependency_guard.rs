//! #740 blackbox dependency-closure guard: the flippable regression for
//! `xtask::repo_guard::scan_blackbox_dependency_closure` — the SAME pure function
//! `run_blackbox_dependency_guard` runs inside `cargo xtask ci` — driven here over
//! planted `cargo metadata` documents to demonstrate RED (a real violation caught, not
//! resting red on non-existence), over the real workspace to demonstrate GREEN, and
//! over malformed documents to demonstrate fail-closed. Mirrors the #616 idiom
//! (`xtask/tests/repo_hygiene_guards.rs:34-49`, `:384-403`).
//!
//! **Criterion 1 lives here too**: [`the_blackbox_guard_is_registered_in_hygiene_guards`]
//! asserts the guard's presence in `xtask::repo_guard::HYGIENE_GUARDS` — the exact list
//! `run_ci` iterates (`xtask/src/main.rs`, beside `run_unsafe_forbid_guard`). Without
//! that assertion, a guard that passes every planted case below and is never wired into
//! `run_ci` would still pass this whole file.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use xtask::repo_guard::{scan_blackbox_dependency_closure, HygieneGuard, HYGIENE_GUARDS};

/// The workspace root (`<root>/xtask` is this crate's manifest dir) — mirrors
/// `xtask/tests/repo_hygiene_guards.rs`'s own helper.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate is nested under the workspace root")
        .to_path_buf()
}

/// A minimal, self-contained `cargo metadata --all-features` document: `wyrd-validate`
/// depends on `target_kind` (kind of the resolve-graph edge AND the declared
/// dependency's `kind`) on `edge_name`, `edge_optional` in the declared list. When
/// `edge_in_graph` is false the resolve node carries no deps at all (simulating an
/// edge some future feature-resolution subtlety keeps out of the graph even under
/// `--all-features` — the belt-and-braces case criterion 2's third planted case
/// covers).
fn planted_metadata(
    edge_name: &str,
    edge_kind_json: &str, // `null` or `"dev"`, verbatim JSON
    edge_optional: bool,
    edge_in_graph: bool,
) -> String {
    let validate_id = "path+file:///fake/wyrd-validate#wyrd-validate@0.0.0";
    let dep_id = format!("path+file:///fake/{edge_name}#{edge_name}@0.0.0");
    let graph_deps = if edge_in_graph {
        format!(
            r#"[{{"name": "{}", "pkg": "{dep_id}", "dep_kinds": [{{"kind": {edge_kind_json}, "target": null}}]}}]"#,
            edge_name.replace('-', "_"),
        )
    } else {
        "[]".to_string()
    };
    let dep_node = if edge_in_graph {
        format!(r#",{{"id": "{dep_id}", "deps": []}}"#)
    } else {
        String::new()
    };
    format!(
        r#"{{
            "packages": [
                {{
                    "id": "{validate_id}",
                    "name": "wyrd-validate",
                    "dependencies": [
                        {{"name": "{edge_name}", "kind": {edge_kind_json}, "optional": {edge_optional}}}
                    ]
                }},
                {{"id": "{dep_id}", "name": "{edge_name}", "dependencies": []}}
            ],
            "resolve": {{
                "nodes": [
                    {{"id": "{validate_id}", "deps": {graph_deps}}}{dep_node}
                ]
            }}
        }}"#
    )
}

// ─── criterion 2: the three planted cases ──────────────────────────────────────────

#[test]
fn a_normal_wyrd_dependency_is_exactly_one_violation_naming_it() {
    let doc = planted_metadata("wyrd-core", "null", false, true);
    let violations = scan_blackbox_dependency_closure(&doc, "wyrd-validate", "wyrd-")
        .expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("wyrd-core"),
        "the violation names the offending crate: {violations:?}"
    );
}

#[test]
fn the_same_edge_marked_dev_is_no_violation() {
    let doc = planted_metadata("wyrd-core", "\"dev\"", false, true);
    let violations = scan_blackbox_dependency_closure(&doc, "wyrd-validate", "wyrd-")
        .expect("planted document is well-formed");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn an_optional_off_by_default_normal_edge_is_still_the_violation() {
    // The edge is declared (kind: null, optional: true) but absent from the resolve
    // graph — exactly the case a narrower (non `--all-features`, declared-list-blind)
    // scan would miss, and exactly what the Design's "belt to the graph's braces"
    // second scan exists to catch.
    let doc = planted_metadata("wyrd-core", "null", true, false);
    let violations = scan_blackbox_dependency_closure(&doc, "wyrd-validate", "wyrd-")
        .expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("wyrd-core") && violations[0].contains("optional"),
        "the violation names the crate and that it is optional: {violations:?}"
    );
}

#[test]
fn scan_is_green_over_the_real_workspace_metadata() {
    let meta = std::process::Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("failed to spawn cargo metadata");
    assert!(meta.status.success(), "cargo metadata must succeed");
    let violations = scan_blackbox_dependency_closure(
        &String::from_utf8_lossy(&meta.stdout),
        "wyrd-validate",
        "wyrd-",
    )
    .expect("the real workspace metadata is scannable");
    assert!(
        violations.is_empty(),
        "wyrd-validate must not depend on any wyrd-* crate: {violations:?}"
    );
}

// ─── fail-closed (Design: mirrors scan_roots' "refusing to pass a workspace it cannot
// see", repo_guard.rs:505-510) ───────────────────────────────────────────────────────

#[test]
fn an_unparsable_document_is_err() {
    scan_blackbox_dependency_closure("not json", "wyrd-validate", "wyrd-")
        .expect_err("unparsable JSON must fail closed");
}

#[test]
fn a_document_with_no_resolve_section_is_err() {
    let doc = r#"{"packages": [{"id": "p", "name": "wyrd-validate", "dependencies": []}]}"#;
    scan_blackbox_dependency_closure(doc, "wyrd-validate", "wyrd-")
        .expect_err("a missing resolve section must fail closed");
}

#[test]
fn a_package_absent_from_packages_is_err() {
    let doc = r#"{"packages": [], "resolve": {"nodes": []}}"#;
    scan_blackbox_dependency_closure(doc, "wyrd-validate", "wyrd-")
        .expect_err("a package cargo metadata never saw must fail closed");
}

#[test]
fn a_package_present_but_absent_from_the_resolve_graph_is_err() {
    let doc = r#"{
        "packages": [{"id": "p", "name": "wyrd-validate", "dependencies": []}],
        "resolve": {"nodes": []}
    }"#;
    scan_blackbox_dependency_closure(doc, "wyrd-validate", "wyrd-")
        .expect_err("a package missing from the resolve graph must fail closed");
}

// ─── criterion 1: the guard really runs inside `run_ci` ────────────────────────────

#[test]
fn the_blackbox_guard_is_registered_in_hygiene_guards() {
    // `xtask::repo_guard::HYGIENE_GUARDS` is the exact list `run_ci` iterates
    // (`xtask/src/main.rs`, beside `run_unsafe_forbid_guard`, main.rs:1558). A guard
    // that is defined and passes every case above but is missing from this list never
    // runs under `cargo xtask ci` — the wiring hazard `run_ci_steps`' injected `exec`
    // already guards against (`main.rs:1486-1498`).
    let guard: &HygieneGuard = HYGIENE_GUARDS
        .iter()
        .find(|g| g.name == "blackbox-dependency-guard")
        .expect("the blackbox guard must be registered in HYGIENE_GUARDS, which run_ci executes");
    // And the registered callable is not a stub: invoking it must actually run the
    // real check over the real workspace and come back green.
    (guard.run)().expect("the registered guard must pass over the real workspace");
}
