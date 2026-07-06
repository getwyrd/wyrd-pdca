## Summary
**User impact:** When you run the storage custodian against a cluster's shared
metadata store, it reports that there is nothing to repair and never rebuilds data
that a failed storage node lost — even when objects written through the S3 gateway
are genuinely under-replicated. Durability is silently not maintained: the repair
backlog sits at zero while data is actually at risk, so a second node failure could
lose it.

This PR makes the custodian actually inspect what has been stored on each pass, so it
detects and rebuilds under-replicated objects a gateway wrote.

Reported in #455.

## What to look at
The change is in the deployable custodian role: each repair pass now checks the live
storage fleet against the recorded object placement *before* it repairs, instead of
only working through a queue that some other component had to fill first. To exercise
the whole loop end to end, run the new integration test — it writes an object through
a real gateway S3 PUT to a shared store, deletes one erasure-coded fragment from a
live storage node, runs the custodian over that same store, and watches the
under-replication backlog rise to 1, drain to 0 after repair, and the object read
back byte-for-byte:

```
cargo test -p wyrd-server --test closed_write_path
```

## Root cause
The deployable custodian's reconstruction loop only drained the shared repair queue
each pass and never scanned committed placement, so nothing derived new repair work
from what the gateway had written. The one shipping loop that *does* derive
obligations from placement — scrub — was never wired into that role, so a custodian
opened over a store a gateway wrote drained an empty queue and computed zero repair
work: the "empty store sees no repair" symptom, even though a node had lost data.

## Fix
Wire scrub into the deployable reconstruction loop so each pass first scrubs the live
(reachable) fleet — walking every referenced fragment and enqueuing any it finds
missing or corrupt — then reconstructs those obligations in the same pass. This is a
composition change in the server role: it reuses the existing scrub and reconstruction
loops and adds no new logic to the custodian library or the gateway write path. A
transient scrub fault degrades only that pass (logged and continued), never the
process, matching the loop's existing survival policy.

## Verification
- **Claim:** an object written through a gateway S3 PUT to a shared cluster metadata
  store becomes a custodian-derived repair obligation after a storage node loses its
  fragment — the under-replication backlog reads 1, returns to 0 after repair, and the
  object reads back byte-identical.
- **Checked:** `crates/server/src/custodian.rs:333-372` on
  `feat/m4-production-metadata-backend` — the reconstruction loop builds a scrub
  context over the live fleet and runs it before reconstruction;
  `crates/custodian/src/reconciliation.rs:65-92` runs scrub before reconstruction
  within a single pass, so the obligation scrub enqueues is visible to reconstruction
  in that same pass. The write and repair paths share one placement contract —
  `crates/server/src/lib.rs:147` (the gateway PUT) records the placement that
  `crates/server/src/cli.rs:758` (the deployable custodian's backend-open path) reads.
- **Test:** `crates/server/tests/closed_write_path.rs` — fails pre-fix (with the scrub
  wiring reverted the backlog reads 0: the custodian drains an empty queue and sees no
  repair work), passes post-fix. It drives a real gateway PUT/GET over loopback gRPC
  D-servers and a shared redb store; the fault modeled is a *reachable* storage node's
  durable fragment loss — the case a placement scan can observe. A wholly-unreachable /
  process-dead node needs the desired-state detector and is left to a follow-up.

Fixes #455
