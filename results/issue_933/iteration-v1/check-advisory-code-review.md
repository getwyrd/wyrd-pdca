- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:2077`: The new `.await?` returns on renewal failure before accounting for fragment writes. Hold one dispatched put, fire the renewal timer, and have all four renewal commits return `Conflict`: staging returns `RenewalRoundsSpent` (classified `Terminal`) while the dropped put may still publish. A renewal refusal can similarly return `NoSuchUpload`, and previously collected unknown-effect write reports are also discarded. Handle the renewal result after collecting fragment outcomes; preserve unknown write effects as `Indeterminate` together with any metadata-commit doubt. Add regression coverage for renewal failure with a pending put and with an already-reported unknown write effect. This confirms the single underlying defect repeated in the frozen T4 log.

No additional actionable reuse, simplification, or efficiency findings. The C5 survivor at `crates/core/src/multipart/stage.rs:315` is equivalent: `3 + 2N` is always odd, while `MAX_BATCH_OPS` is 500, so changing `>` to `>=` cannot change this check's result.

Reviewed the target source and frozen gate evidence; no tests were rerun and no target files were changed.

<!-- pdca:leaf-complete -->
