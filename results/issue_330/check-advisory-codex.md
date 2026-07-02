# Advisory Review: Issue #330 Scrub Missing-Fragment Detection

No blocking correctness issues found. I did not find any concrete patch-introduced correctness bug, nor any reuse/simplification/efficiency cleanup worth routing as an advisory finding.

## Evidence

- `crates/custodian/src/scrub.rs:74` still derives scrub's universe from `referenced_fragments(ctx.meta)`, which `crates/custodian/src/gc.rs:179` defines over committed inode records only; this grounds the intended false-positive guard for in-flight pending writes.
- `crates/custodian/src/scrub.rs:85` groups the committed reference set by placed D server and `crates/custodian/src/scrub.rs:91` only probes stores present in `ctx.fleet`, matching the existing `ScrubContext` contract that the caller supplies the fleet to scrub.
- `crates/custodian/src/scrub.rs:129` now turns `Ok(None)` for a referenced placed fragment into `emit_missing` plus `repair::enqueue_repair`, which directly addresses the absent-fragment gap described in issue #330.
- `crates/custodian/tests/scrub.rs:887` covers the committed-reference-but-absent fragment case, and `crates/custodian/tests/scrub.rs:924` covers the pending-write non-finding guardrail.

## Validation

Applied `patch.diff` cleanly in a temporary copy of `$PDCA_TARGET` at `/tmp/wyrd-issue330-review.Ur4Zwm`.

Ran:

```sh
env ZIG_LOCAL_CACHE_DIR=/tmp/zig-local-cache ZIG_GLOBAL_CACHE_DIR=/tmp/zig-global-cache XDG_CACHE_HOME=/tmp/xdg-cache cargo test -p wyrd-custodian --test scrub
```

Result: `12 passed; 0 failed`.

The initial test attempts failed only because the sandbox made `/home/eddie/.cache/zig/tmp` read-only for the linker cache; redirecting the cache to `/tmp` resolved that environment issue.
