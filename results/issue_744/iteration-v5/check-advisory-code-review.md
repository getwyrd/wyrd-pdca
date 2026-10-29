- NEEDS-HUMAN [impl] — **Low: reject a zero root budget even with an explicit threshold.** `crates/validate/src/sizes.rs:244` accepts `TuningFlags { max_root_value_bytes: Some(0), segment_threshold_bytes: Some(1048576), ..Default::default() }`: the explicit threshold bypasses `derived_segment_threshold`, and `Tuning::check` validates only the chunk size and threshold. Both `resolve` and `size_classes` therefore succeed with a zero budget, contrary to the public contract at `crates/validate/src/sizes.rs:95` that hand-built zero values are refused. The CLI rejects this input, so the gap affects direct library callers. Validate a nonzero root budget independently of threshold derivation and add this combination to the public-API tests.

No other correctness or reuse/simplification/efficiency findings. Reviewed the target source and frozen gate logs; did not rerun builds or modify the target.

<!-- pdca:leaf-complete -->
