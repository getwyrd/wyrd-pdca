// Regression tests for issue #154 items 2 (panic-safe Tier-2 teardown) and 4
// (`WYRD_DSERVER_COUNT` warns on a rejected value). These ship INLINE in
// `xtask/src/main.rs` (the brief named no test path; this is the natural Rust
// home — they exercise private fns `finalize_panic_safe` / `resolve_dserver_count`
// and compose with #150's `finish_integration`). `cargo xtask ci` runs them via
// `cargo test --workspace`. This file is a verbatim copy of the #154 additions to
// that inline module, preserved in the bundle for the reviewer.
//
// COMPOSITION WITH #150 (per the brief's Iteration-1 carry-forward): #150 lands
// first and introduces `finish_integration` (capture-logs-before-teardown) plus
// the `#[cfg(test)] mod tests` these tests extend. #154's `finalize_panic_safe`
// wraps the test body so finalization — including #150's capture-before-teardown —
// runs on the PANIC-unwind path too, then resumes the panic. The panic test below
// drives `finalize_panic_safe` THROUGH `finish_integration`, asserting both
// invariants hold together (capture before teardown, AND on a panic).
//
// Red→green proof: on the #150 base neither `finalize_panic_safe` nor
// `resolve_dserver_count` exists (the body is called inline so a panic unwinds
// past finalization, and the count is clamped with a silent `.unwrap_or`), so the
// module fails to compile pre-fix (E0425 on both fns) and passes once #154 lands.

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // #154 item 4 — `WYRD_DSERVER_COUNT` must not clamp a rejected value silently.

    #[test]
    fn dserver_count_unset_uses_default_silently() {
        assert_eq!(resolve_dserver_count(None), (DSERVER_COUNT, None));
    }

    #[test]
    fn dserver_count_accepts_a_valid_value_silently() {
        assert_eq!(resolve_dserver_count(Some("4".to_string())), (4, None));
        // The boundary (the smallest meaningful spread) is accepted.
        assert_eq!(resolve_dserver_count(Some("2".to_string())), (2, None));
    }

    #[test]
    fn dserver_count_rejects_unusable_values_with_a_warning() {
        // `0`/`1` (too few to spread), unparsable garbage, and empty all fall
        // back to the default — but each must surface a warning, not clamp silently.
        for bad in ["0", "1", "garbage", ""] {
            let (count, warning) = resolve_dserver_count(Some(bad.to_string()));
            assert_eq!(count, DSERVER_COUNT, "{bad:?} should fall back to default");
            let warning = warning.expect("a rejected value must warn, not clamp silently");
            assert!(
                warning.contains("WYRD_DSERVER_COUNT"),
                "warning should name the variable: {warning:?}"
            );
        }
    }

    // #154 item 2 — finalization must run even when the test body PANICS, and the
    // panic must still resume. Composed with #150's `finish_integration`, this
    // proves the two invariants hold together: a panicking run still captures
    // container diagnostics BEFORE teardown (#150) and never leaks the cluster
    // (#154). A `panic!` unwinding past finalization is exactly what would leak it.

    #[test]
    fn panic_finalizes_capture_then_teardown_then_resumes() {
        let order: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            finalize_panic_safe(
                || panic!("integration test panicked"),
                |result| {
                    finish_integration(
                        result,
                        || order.borrow_mut().push("capture_logs"),
                        || order.borrow_mut().push("teardown"),
                    )
                },
            )
        }));
        assert!(outcome.is_err(), "the panic must resume, not be swallowed");
        assert_eq!(
            *order.borrow(),
            vec!["capture_logs", "teardown"],
            "a panicking run must still capture diagnostics before teardown (#150 + #154)",
        );
    }

    #[test]
    fn clean_run_finalizes_without_capturing_or_panicking() {
        let order: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let result = finalize_panic_safe(
            || Ok(()),
            |result| {
                finish_integration(
                    result,
                    || order.borrow_mut().push("capture_logs"),
                    || order.borrow_mut().push("teardown"),
                )
            },
        );
        assert!(result.is_ok(), "a passing run stays Ok");
        assert_eq!(
            *order.borrow(),
            vec!["teardown"],
            "a passing run captures no logs; only teardown runs",
        );
    }

    #[test]
    fn err_return_finalizes_capture_then_teardown_and_propagates() {
        // A non-panic failure (`Err`) is finalized exactly like #150: capture
        // before teardown, error propagated unchanged.
        let order: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let result = finalize_panic_safe(
            || Err("boom".to_string()),
            |result| {
                finish_integration(
                    result,
                    || order.borrow_mut().push("capture_logs"),
                    || order.borrow_mut().push("teardown"),
                )
            },
        );
        assert_eq!(result, Err("boom".to_string()));
        assert_eq!(
            *order.borrow(),
            vec!["capture_logs", "teardown"],
            "an Err return captures before teardown, like #150",
        );
    }
}
