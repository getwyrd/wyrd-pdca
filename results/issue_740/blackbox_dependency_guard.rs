//! #740 blackbox dependency-closure guard: the flippable regression for
//! `xtask::repo_guard::scan_blackbox_dependency_closure` — the SAME pure function
//! `run_blackbox_dependency_guard` runs inside `cargo xtask ci` — driven here over
//! planted `cargo metadata` documents to demonstrate RED (a real violation caught, not
//! resting red on non-existence), over the real workspace to demonstrate GREEN, and
//! over malformed documents to demonstrate fail-closed. Mirrors the #616 idiom
//! (`xtask/tests/repo_hygiene_guards.rs:34-49`, `:384-403`).
//!
//! Three properties are proved separately, because each can hold while another fails:
//!
//! 1. **the closure is transitive** — a `wyrd-*` crate two and three hops out is found,
//!    with the path that reached it, while a clean chain of the same depth stays green
//!    and a `dev` edge prunes everything behind it (proposal 0017 §9's "normal, not
//!    total" is a property of the whole walk, not of the first hop);
//! 2. **the guard fails closed** — every part of the document it cannot read is an
//!    `Err`, never a vacuously clean pass: an unparsable document, a missing resolve
//!    section, an unreachable node one hop out, and — the case that makes malformed
//!    metadata dangerous rather than merely odd — a dependency `kind` it cannot
//!    classify. "I cannot tell what kind this edge is" must not silently become "this
//!    edge is dev-only", because that hides the edge and everything behind it;
//! 3. **the guard actually runs** — [`the_blackbox_guard_is_registered_in_hygiene_guards`]
//!    asserts its presence in `xtask::repo_guard::HYGIENE_GUARDS` and invokes the
//!    registered callable over the real workspace, and
//!    [`run_hygiene_guards_reaches_the_blackbox_guard`] /
//!    [`run_hygiene_guards_stops_at_the_first_failing_guard`] drive the phase `run_ci`
//!    executes (`xtask/src/main.rs`, beside `run_unsafe_forbid_guard`) with a recording
//!    runner, so "registered but never reached" and "reached but its `Err` dropped" are
//!    both covered. Without those, a guard that passes every planted case below and
//!    never runs under `cargo xtask ci` would still pass this whole file.

#![forbid(unsafe_code)]

use xtask::repo_guard::{
    run_hygiene_guards, scan_blackbox_dependency_closure, HygieneGuard, BLACKBOX_GUARD_TARGETS,
    BLACKBOX_METADATA_ARGS, HYGIENE_GUARDS,
};
use xtask::workspace_root;

// ─── planted `cargo metadata` documents ────────────────────────────────────────────
//
// Shaped exactly like the real thing (verified against `cargo metadata --format-version
// 1 --locked --all-features` on this workspace): package ids are opaque strings, a
// declared dependency carries `name`/`kind`/`optional`, and a resolve edge carries
// `pkg` plus a non-empty `dep_kinds` array whose entries hold `null`, `"dev"` or
// `"build"`.

/// A declared dependency entry: `(dependency name, verbatim JSON for `kind`, optional)`.
type Declared<'a> = (&'a str, &'a str, bool);
/// A resolve-graph edge: `(dependency name, verbatim JSON for the `dep_kinds` array)`.
type Edge<'a> = (&'a str, &'a str);

/// A normal (`[dependencies]`) resolve edge — the only kind that ships in the binary.
const NORMAL: &str = r#"[{"kind": null, "target": null}]"#;
/// A `[dev-dependencies]` resolve edge: deliberately unconstrained (proposal 0017 §9).
const DEV: &str = r#"[{"kind": "dev", "target": null}]"#;
/// A `[build-dependencies]` resolve edge: runs at build time, never linked.
const BUILD: &str = r#"[{"kind": "build", "target": null}]"#;

fn pkg_id(name: &str) -> String {
    format!("path+file:///fake/{name}#{name}@0.0.0")
}

fn metadata_doc(packages: &[(&str, &[Declared])], nodes: &[(&str, &[Edge])]) -> String {
    let packages_json = packages
        .iter()
        .map(|(name, declared)| {
            let deps = declared
                .iter()
                .map(|(dep, kind, optional)| {
                    format!(r#"{{"name": "{dep}", "kind": {kind}, "optional": {optional}}}"#)
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"id": "{}", "name": "{name}", "dependencies": [{deps}]}}"#,
                pkg_id(name)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let nodes_json = nodes
        .iter()
        .map(|(name, edges)| {
            let deps = edges
                .iter()
                .map(|(dep, dep_kinds)| {
                    format!(
                        r#"{{"name": "{}", "pkg": "{}", "dep_kinds": {dep_kinds}}}"#,
                        dep.replace('-', "_"),
                        pkg_id(dep)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(r#"{{"id": "{}", "deps": [{deps}]}}"#, pkg_id(name))
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"packages": [{packages_json}], "resolve": {{"nodes": [{nodes_json}]}}}}"#)
}

/// `wyrd-validate --<edge_kind>--> <dep>`, declared and resolved, for the one-hop cases.
fn one_hop(dep: &str, declared_kind: &str, optional: bool, edge_kind: Option<&str>) -> String {
    let declared: Vec<Declared> = vec![(dep, declared_kind, optional)];
    let edges: Vec<Edge> = edge_kind.map(|k| vec![(dep, k)]).unwrap_or_default();
    metadata_doc(
        &[("wyrd-validate", &declared), (dep, &[])],
        &[("wyrd-validate", &edges), (dep, &[])],
    )
}

fn scan(doc: &str) -> Result<Vec<String>, String> {
    scan_blackbox_dependency_closure(doc, "wyrd-validate", "wyrd-")
}

// ─── criterion 2: the three planted cases ──────────────────────────────────────────

#[test]
fn a_normal_wyrd_dependency_is_exactly_one_violation_naming_it() {
    let violations = scan(&one_hop("wyrd-core", "null", false, Some(NORMAL)))
        .expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("wyrd-core"),
        "the violation names the offending crate: {violations:?}"
    );
}

#[test]
fn the_same_edge_marked_dev_is_no_violation() {
    let violations = scan(&one_hop("wyrd-core", "\"dev\"", false, Some(DEV)))
        .expect("planted document is well-formed");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn an_optional_off_by_default_normal_edge_is_still_the_violation() {
    // The edge is declared (kind: null, optional: true) but absent from the resolve
    // graph — exactly the case a narrower (non `--all-features`, declared-list-blind)
    // scan would miss, and exactly what the Design's "belt to the graph's braces"
    // second scan exists to catch.
    let violations =
        scan(&one_hop("wyrd-core", "null", true, None)).expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("wyrd-core") && violations[0].contains("optional"),
        "the violation names the crate and that it is optional: {violations:?}"
    );
}

// ─── the closure is TRANSITIVE, and pruned by edge kind at every depth ─────────────

/// The whole reason the guard walks a graph rather than reading one manifest: a
/// validator that pulls in a helper crate which itself links `wyrd-core` has exactly the
/// self-referential verdict proposal 0017 §9 forbids, and no direct-dependency check
/// would see it. The reported PATH is asserted, not just the crate name — "reaches
/// wyrd-core" is not actionable; "via wyrd-validate -> helper -> wyrd-core" is.
#[test]
fn a_multi_hop_normal_path_reaches_the_violation_and_reports_the_path() {
    let doc = metadata_doc(
        &[
            ("wyrd-validate", &[("helper", "null", false)]),
            ("helper", &[("deep", "null", false)]),
            ("deep", &[("wyrd-core", "null", false)]),
            ("wyrd-core", &[]),
        ],
        &[
            ("wyrd-validate", &[("helper", NORMAL)]),
            ("helper", &[("deep", NORMAL)]),
            ("deep", &[("wyrd-core", NORMAL)]),
            ("wyrd-core", &[]),
        ],
    );
    let violations = scan(&doc).expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("wyrd-validate -> helper -> deep -> wyrd-core"),
        "the violation reports the path that reached it: {violations:?}"
    );
}

/// The complementary half — without it, a guard that reported a violation for every
/// reachable package would pass the case above. A three-hop closure of ordinary
/// third-party crates is walked to the end and stays green.
#[test]
fn a_clean_multi_hop_closure_is_green() {
    let doc = metadata_doc(
        &[
            ("wyrd-validate", &[("aws-sdk-s3", "null", false)]),
            ("aws-sdk-s3", &[("hyper", "null", false)]),
            ("hyper", &[("bytes", "null", false)]),
            ("bytes", &[]),
        ],
        &[
            ("wyrd-validate", &[("aws-sdk-s3", NORMAL)]),
            ("aws-sdk-s3", &[("hyper", NORMAL)]),
            ("hyper", &[("bytes", NORMAL)]),
            ("bytes", &[]),
        ],
    );
    assert!(
        scan(&doc)
            .expect("planted document is well-formed")
            .is_empty(),
        "a closure of ordinary crates must stay green however deep it is"
    );
}

/// "Normal, not total" applies to the WHOLE walk, not just the first hop: the §14
/// fixtures are dev-dependencies, and what they link is unconstrained. A guard that
/// pruned dev edges only at the root would report every crate those fixtures reach.
#[test]
fn a_dev_edge_prunes_everything_behind_it() {
    for pruned in [DEV, BUILD] {
        let doc = metadata_doc(
            &[
                ("wyrd-validate", &[("fixture", "\"dev\"", false)]),
                ("fixture", &[("wyrd-core", "null", false)]),
                ("wyrd-core", &[]),
            ],
            &[
                ("wyrd-validate", &[("fixture", pruned)]),
                ("fixture", &[("wyrd-core", NORMAL)]),
                ("wyrd-core", &[]),
            ],
        );
        assert!(
            scan(&doc)
                .expect("planted document is well-formed")
                .is_empty(),
            "a {pruned} edge must prune the subtree behind it"
        );
    }
}

/// The two halves together, in one graph: the same `wyrd-core` is reachable through a
/// dev edge (legal) and through a normal one (forbidden). Reporting it once, via the
/// normal path, is the property — a walk that stopped at the first *reachable* mention
/// would report the dev route, and one that pruned by target rather than by edge would
/// report nothing at all.
#[test]
fn a_dev_route_does_not_mask_a_normal_route_to_the_same_crate() {
    let doc = metadata_doc(
        &[
            (
                "wyrd-validate",
                &[("fixture", "\"dev\"", false), ("shim", "null", false)],
            ),
            ("fixture", &[("wyrd-core", "null", false)]),
            ("shim", &[("wyrd-core", "null", false)]),
            ("wyrd-core", &[]),
        ],
        &[
            ("wyrd-validate", &[("fixture", DEV), ("shim", NORMAL)]),
            ("fixture", &[("wyrd-core", NORMAL)]),
            ("shim", &[("wyrd-core", NORMAL)]),
            ("wyrd-core", &[]),
        ],
    );
    let violations = scan(&doc).expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("wyrd-validate -> shim -> wyrd-core"),
        "the normal route is the one reported: {violations:?}"
    );
}

/// A real resolve graph contains cycles (two crates that are each other's
/// dev-dependency, a crate that depends on an older copy of itself). The walk must
/// terminate and still report what it found, rather than looping until the test harness
/// times out.
#[test]
fn a_cyclic_graph_terminates_and_still_reports() {
    let doc = metadata_doc(
        &[
            ("wyrd-validate", &[("a", "null", false)]),
            ("a", &[("b", "null", false)]),
            ("b", &[("a", "null", false), ("wyrd-core", "null", false)]),
            ("wyrd-core", &[]),
        ],
        &[
            ("wyrd-validate", &[("a", NORMAL)]),
            ("a", &[("b", NORMAL)]),
            ("b", &[("a", NORMAL), ("wyrd-core", NORMAL)]),
            ("wyrd-core", &[]),
        ],
    );
    let violations = scan(&doc).expect("planted document is well-formed");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("wyrd-core"), "{violations:?}");
}

// ─── fail-closed (Design: mirrors scan_roots' "refusing to pass a workspace it cannot
// see", repo_guard.rs:496-510) ───────────────────────────────────────────────────────

#[test]
fn an_unparsable_document_is_err() {
    scan("not json").expect_err("unparsable JSON must fail closed");
}

#[test]
fn a_document_with_no_resolve_section_is_err() {
    let doc = r#"{"packages": [{"id": "p", "name": "wyrd-validate", "dependencies": []}]}"#;
    scan(doc).expect_err("a missing resolve section must fail closed");
}

#[test]
fn a_package_absent_from_packages_is_err() {
    scan(r#"{"packages": [], "resolve": {"nodes": []}}"#)
        .expect_err("a package cargo metadata never saw must fail closed");
}

#[test]
fn a_package_present_but_absent_from_the_resolve_graph_is_err() {
    let doc = r#"{
        "packages": [{"id": "p", "name": "wyrd-validate", "dependencies": []}],
        "resolve": {"nodes": []}
    }"#;
    scan(doc).expect_err("a package missing from the resolve graph must fail closed");
}

/// Fail-closed one hop OUT, which is where a silent skip does real damage: an
/// intermediate package reached by a normal edge but carrying no resolve node hides
/// everything behind it, so `wyrd-validate -> middle -> wyrd-core` would report clean.
/// Skipping the unwalkable node is indistinguishable, to the gate, from a clean closure.
#[test]
fn an_edge_reaching_a_package_with_no_resolve_node_is_err() {
    let doc = r#"{
        "packages": [
            {"id": "v", "name": "wyrd-validate", "dependencies": []},
            {"id": "m", "name": "middle", "dependencies": []}
        ],
        "resolve": {"nodes": [
            {"id": "v", "deps": [{"name": "middle", "pkg": "m",
                                  "dep_kinds": [{"kind": null, "target": null}]}]}
        ]}
    }"#;
    let err = scan(doc).expect_err("a reached package with no node must fail closed");
    assert!(
        err.contains("middle"),
        "the error names what it cannot see: {err}"
    );
}

/// "I cannot tell what kind this edge is" is not "this edge is dev-only". Each of these
/// documents carries a real `wyrd-core` edge whose kind cannot be classified — a
/// missing `dep_kinds` array, an empty one, an entry without a `kind` key, and one whose
/// `kind` is neither `null` nor a kind cargo emits. Classifying any of them as
/// non-normal would drop the edge (and everything behind it) out of the closure the
/// guard claims to have checked, which is exactly a green gate over an unchecked tree.
#[test]
fn an_edge_whose_kind_cannot_be_classified_is_err_not_silently_non_normal() {
    for (case, dep_kinds) in [
        ("no dep_kinds array", None),
        ("an empty dep_kinds array", Some("[]")),
        ("an entry with no kind key", Some(r#"[{"target": null}]"#)),
        ("a non-string, non-null kind", Some(r#"[{"kind": 42}]"#)),
        (
            "a kind string cargo never emits",
            Some(r#"[{"kind": "runtime"}]"#),
        ),
    ] {
        let edge = match dep_kinds {
            Some(kinds) => {
                format!(r#"{{"name": "wyrd_core", "pkg": "c", "dep_kinds": {kinds}}}"#)
            }
            None => r#"{"name": "wyrd_core", "pkg": "c"}"#.to_string(),
        };
        let doc = format!(
            r#"{{
                "packages": [
                    {{"id": "v", "name": "wyrd-validate", "dependencies": []}},
                    {{"id": "c", "name": "wyrd-core", "dependencies": []}}
                ],
                "resolve": {{"nodes": [
                    {{"id": "v", "deps": [{edge}]}},
                    {{"id": "c", "deps": []}}
                ]}}
            }}"#
        );
        let err = scan(&doc).expect_err(&format!("{case} must fail closed"));
        assert!(
            err.contains("wyrd-core") || err.contains("wyrd_core"),
            "the error names the edge it cannot read ({case}): {err}"
        );
    }
}

/// The same rule on the declared list, which is the half that catches an optional
/// off-by-default edge: a `kind` this guard cannot classify must not be quietly treated
/// as "not a normal dependency", because that is precisely how a hand-edited or
/// future-cargo document turns the belt-and-braces scan into a vacuous pass.
#[test]
fn a_declared_dependency_whose_kind_cannot_be_classified_is_err() {
    for (case, entry) in [
        ("no kind key", r#"{"name": "wyrd-core", "optional": true}"#),
        (
            "a non-string, non-null kind",
            r#"{"name": "wyrd-core", "kind": 42, "optional": true}"#,
        ),
        (
            "a kind string cargo never emits",
            r#"{"name": "wyrd-core", "kind": "runtime", "optional": true}"#,
        ),
    ] {
        let doc = format!(
            r#"{{
                "packages": [{{"id": "v", "name": "wyrd-validate", "dependencies": [{entry}]}}],
                "resolve": {{"nodes": [{{"id": "v", "deps": []}}]}}
            }}"#
        );
        let err = scan(&doc).expect_err(&format!(
            "a declared dependency with {case} must fail closed"
        ));
        assert!(
            err.contains("wyrd-core") && err.contains("kind"),
            "the error names the declaration it cannot read ({case}): {err}"
        );
    }
}

/// Two packages under one name (two versions in the resolve, or a registry crate that
/// shares a workspace member's name) is an ambiguity, not a coin flip: auditing whichever
/// entry came first would clear the other one silently.
#[test]
fn more_than_one_package_under_the_scanned_name_is_err() {
    let doc = r#"{
        "packages": [
            {"id": "v1", "name": "wyrd-validate", "dependencies": []},
            {"id": "v2", "name": "wyrd-validate",
             "dependencies": [{"name": "wyrd-core", "kind": null, "optional": false}]}
        ],
        "resolve": {"nodes": [{"id": "v1", "deps": []}, {"id": "v2", "deps": []}]}
    }"#;
    let err = scan(doc).expect_err("an ambiguous package name must fail closed");
    assert!(
        err.contains("more than one package named"),
        "the error says what is ambiguous: {err}"
    );
}

/// The declared-list scan is the belt to the graph's braces; a belt nobody can read is
/// not a belt, so a package entry without a `dependencies` array — or with a nameless
/// entry in it — is an error rather than a skipped half of the scan.
#[test]
fn an_unreadable_declared_dependency_list_is_err() {
    let no_list = r#"{
        "packages": [{"id": "v", "name": "wyrd-validate"}],
        "resolve": {"nodes": [{"id": "v", "deps": []}]}
    }"#;
    scan(no_list).expect_err("a missing declared-dependency list must fail closed");

    let nameless = r#"{
        "packages": [{"id": "v", "name": "wyrd-validate",
                      "dependencies": [{"kind": null, "optional": false}]}],
        "resolve": {"nodes": [{"id": "v", "deps": []}]}
    }"#;
    scan(nameless).expect_err("a nameless declared dependency must fail closed");
}

// ─── the real workspace, read exactly as production reads it ───────────────────────

/// The invariant itself, over the document the guard actually consumes: the same argv
/// (`BLACKBOX_METADATA_ARGS`) production shells, so this case cannot pass over a
/// friendlier document than the gate reads. The flags are asserted individually because
/// each one silently narrows what the scan can see if it is dropped — `--all-features`
/// most of all, since without it an optional off-by-default `wyrd-*` edge never reaches
/// the resolve graph and every planted case above still passes.
#[test]
fn the_guard_reads_an_all_features_locked_resolve_of_the_real_workspace() {
    assert!(
        BLACKBOX_METADATA_ARGS.contains(&"--all-features")
            && BLACKBOX_METADATA_ARGS.contains(&"--locked")
            && !BLACKBOX_METADATA_ARGS.contains(&"--no-deps"),
        "the closure must be resolved under every feature, from the committed lockfile, \
         WITH the resolve graph: {BLACKBOX_METADATA_ARGS:?}"
    );
    let meta = std::process::Command::new("cargo")
        .args(BLACKBOX_METADATA_ARGS)
        .current_dir(workspace_root())
        .output()
        .expect("failed to spawn cargo metadata");
    assert!(
        meta.status.success(),
        "cargo metadata must succeed: {}",
        String::from_utf8_lossy(&meta.stderr)
    );
    let metadata = String::from_utf8_lossy(&meta.stdout);
    for (package, prefix) in BLACKBOX_GUARD_TARGETS {
        let violations = scan_blackbox_dependency_closure(&metadata, package, prefix)
            .expect("the real workspace metadata is scannable");
        assert!(
            violations.is_empty(),
            "{package} must not reach any {prefix}* crate: {violations:?}"
        );
    }
}

// ─── criterion 1: the guard really runs inside `run_ci` ────────────────────────────

#[test]
fn the_blackbox_guard_is_registered_in_hygiene_guards() {
    // `xtask::repo_guard::HYGIENE_GUARDS` is the exact list `run_ci`'s hygiene phase
    // runs (`xtask/src/main.rs:1563`, beside `run_unsafe_forbid_guard` at `:1558`). A
    // guard that is defined and passes every case above but is missing from this list
    // never runs under `cargo xtask ci` — the wiring hazard `run_ci_steps`' injected
    // `exec` already guards against (`main.rs:1486-1498`).
    let guard: &HygieneGuard = HYGIENE_GUARDS
        .iter()
        .find(|g| g.name == "blackbox-dependency-guard")
        .expect("the blackbox guard must be registered in HYGIENE_GUARDS, which run_ci executes");
    // And the registered callable is not a stub: invoking it must actually run the
    // real check over the real workspace and come back green.
    (guard.run)().expect("the registered guard must pass over the real workspace");
}

/// Registration is only half of "the guard runs": the phase that walks the list must
/// actually reach it. `run_hygiene_guards` is the code `run_ci` executes
/// (`xtask/src/main.rs:1563`), driven here with a RECORDING runner instead of the real
/// one — the `run_ci_steps` idiom (`main.rs:1486-1498`), which exercises the wiring
/// without spawning anything.
#[test]
fn run_hygiene_guards_reaches_the_blackbox_guard() {
    let mut visited = Vec::new();
    run_hygiene_guards(&mut |guard| {
        visited.push(guard.name);
        Ok(())
    })
    .expect("a recording runner never fails");
    assert!(
        visited.contains(&"blackbox-dependency-guard"),
        "the phase run_ci executes must reach the blackbox guard: {visited:?}"
    );
    assert_eq!(
        visited.len(),
        HYGIENE_GUARDS.len(),
        "every registered guard is reached, not just the first: {visited:?}"
    );
}

/// The other way a wired-up guard reports to nobody: its `Err` is dropped instead of
/// stopping the gate. A violation that does not fail the build is a green gate over a
/// broken invariant, so the phase must propagate the first failure and stop there.
#[test]
fn run_hygiene_guards_stops_at_the_first_failing_guard() {
    let mut attempted = 0usize;
    let err = run_hygiene_guards(&mut |_| {
        attempted += 1;
        Err("planted guard failure".to_string())
    })
    .expect_err("a failing guard must fail the phase, not be discarded");
    assert!(err.contains("planted guard failure"), "{err}");
    assert_eq!(attempted, 1, "the phase stops at the first failure");
}
