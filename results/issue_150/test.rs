// The test the brief requires for issue #150, as shipped in
// `xtask/src/main.rs` under `#[cfg(test)] mod tests` (a binary crate's private
// `finish_integration` is only reachable from an in-crate test module, not from
// `xtask/tests/`). Reproduced here as the bundle's explicit test artifact.
//
// Red before the fix: pre-fix `finish_integration` does not exist, so the test
//   fails to compile —
//     error[E0425]: cannot find function `finish_integration` in this scope
// Green after the fix: `cargo test -p xtask` → 2 passed.
//
// Import-light by design: it drives the extracted ordering function with
// order-recording closures, so it needs no Docker / container runtime and runs
// inside the headless `cargo xtask ci` (`cargo test --workspace`).

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // The operability invariant (#150): on a FAILED integration run, container
    // diagnostics must be captured BEFORE the cluster is torn down — teardown
    // never precedes log capture, or a failed nightly run preserves nothing to
    // diagnose it. Drives `finish_integration` with order-recording closures so
    // the ordering is asserted without a container runtime.
    #[test]
    fn failure_captures_logs_before_teardown() {
        let order: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let result = finish_integration(
            Err("Tier-2 integration test failed".to_string()),
            || order.borrow_mut().push("capture_logs"),
            || order.borrow_mut().push("teardown"),
        );
        assert!(result.is_err(), "the failure must be propagated unchanged");
        assert_eq!(
            *order.borrow(),
            vec!["capture_logs", "teardown"],
            "diagnostics must be captured before teardown destroys them",
        );
    }

    // A passing run needs no diagnostics: only teardown runs, and Ok is
    // propagated. (Guards against a fix that always captures, which would noise
    // up every green nightly run.)
    #[test]
    fn success_tears_down_without_capturing_logs() {
        let order: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let result = finish_integration(
            Ok(()),
            || order.borrow_mut().push("capture_logs"),
            || order.borrow_mut().push("teardown"),
        );
        assert!(result.is_ok(), "a passing run must stay Ok");
        assert_eq!(
            *order.borrow(),
            vec!["teardown"],
            "a passing run captures no logs; only teardown runs",
        );
    }
}
