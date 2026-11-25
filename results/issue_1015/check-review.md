# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `fdb cluster healthy` confirmed absent (detect cmd `fdbcli -C "${WYRD_FDB_CLUSTER_FILE:-/etc/foundationdb/fdb.cluster}" --exec "status minimal"` exited 1). Install hint: bring up deploy/fdb-single-node (docker compose up -d) and run `configure new single memory` once; fdbcli ships in the foundationdb-clients package. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
