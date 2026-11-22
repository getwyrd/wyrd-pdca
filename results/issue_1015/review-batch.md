# Batched review — 3 passes, union of findings

- [ ] `crates/server/src/cli.rs:2127` **BUG** (seen by 1 pass): The one-shot uses a create-capable redb open after an existence check, so it can initialize a zero-byte `meta.redb` or recreate a file removed between the check and the open, then report that nothing needs reaping.
- [ ] `crates/server/src/cli.rs:1722` **BUG** (seen by 1 pass): The TiKV and FoundationDB outcome lines identify only the backend type, so a successful run against the wrong configured cluster is indistinguishable from a run against the intended cluster.
- [ ] `crates/server/src/cli.rs:2127` **BUG** (seen by 1 pass): The existence check still calls redb’s create-or-open API, so a zero-length file or a file removed between the check and open can be initialized as an empty database and yield a successful “nothing to reap” result.
- [ ] `crates/server/src/cli.rs:1722` **BUG** (seen by 1 pass): The one-shot prints only `tikv` or `fdb` for a networked store, so an operator who accidentally targets another cluster using the same backend gets an indistinguishable success report.
- [ ] `crates/custodian/src/reconciliation.rs:199` **BUG** (seen by 1 pass): The one-shot starts retirement after an empty store check, so a first Create and Abort under a mismatched profile can land before retirement lists sessions and let retirement mutate records under an unchecked profile.
- [ ] `crates/server/src/cli.rs:1722` **BUG** (seen by 1 pass): The outcome line identifies network stores only as `tikv` or `fdb`, so an operator who selects the wrong PD endpoints or FoundationDB cluster file gets the same store identifier on a successful pass.
- [ ] `crates/server/src/cli.rs:2127` **BUG** (seen by 1 pass): The existence check still calls redb’s create-capable opener, which can initialize a zero-byte file or recreate a file removed after the check and then report that the new store has nothing to reap.
- [ ] `crates/custodian/src/reconciliation.rs:239` **CONVENTION** (seen by 1 pass): Returning the reconstruction error unconditionally masks a later `CommitUnknownResult` from retirement, such as an indeterminate cursor commit, and loses the error classification the transaction rubric requires callers to preserve.

## Recorded rejections (triaged — not blocking)

- `crates/custodian/src/reconciliation.rs:187` BUG: The combined reconciliation path checks the admission profile on `reaper.meta` but drains `ctx.meta`, so callers supplying different stores can authorize destructive retirement on a store whose profile was never checked.
- `crates/custodian/src/reaper.rs:1199` BUG: The fence does not pin the admission profile it checked, so a pass that read a matching zero-count ledger can judge and fence sessions admitted after a profile swap before the final check detects the mismatch.
- `crates/custodian/src/reconciliation.rs:187` BUG: The combined step lets the reaper authorize retirement using one metadata store and then drain another through the reconstruction context, bypassing the profile check for the store the drain mutates.
- `crates/custodian/src/reaper.rs:1199` BUG: If a matching ledger reaches zero and an operator switches profiles before the session scan, the reaper can fence uploads admitted under the replacement profile, and its final ledger check cannot undo those fences.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
