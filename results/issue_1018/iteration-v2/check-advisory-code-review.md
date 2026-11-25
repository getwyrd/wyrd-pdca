- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3190`: The printed teardown command loses the network store selection. For example, `WYRD_FDB_CLUSTER_FILE=/clusters/a.cluster wyrd mpu abort --upload-id ID --metadata-backend fdb` prints a custodian command containing only `--metadata-backend fdb`. The invocation-scoped override is gone when that command is pasted, so the connection helper (`crates/server/src/cli.rs:341`) can fall back to `/etc/foundationdb/fdb.cluster` and reap a different cluster while leaving the aborted upload undrained. TiKV likewise loses an invocation-scoped `WYRD_TIKV_PD_ENDPOINTS`. Include the effective, shell-quoted connection settings in the suggested command and cover invocation-scoped overrides in its tests.

No additional correctness or reuse/simplification/efficiency findings. Reviewed the target source and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->
