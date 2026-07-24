# Adversarial review — issue 626 (iteration 7), advisory

Lens: refute the settlement (leg B) and the reviewer's verdict; find the execution that
strands/absorbs/publishes-over-reclaimed/over-envelope and is **absent from the register**.
Target read-only at `$PDCA_TARGET`
(`docs/design/proposals/draft/0016-multipart-commit-protocol.md`, 1919 lines).

## The evidence (red→green)

- Docs-only deliverable, verification posture (a). There is **no code red→green to re-run**:
  `C4-verify` and `C5-mutants` are vacuous on a no-Rust diff (the brief says so, and the gate
  rows record it — I did **not** read either as green). Mechanical leg A1 (`C4-ci`) is green,
  and I confirmed the two things it actually checks: the README index row mirrors 0014's
  draft-row shape with a resolving relative link (`docs/design/proposals/README.md:32`), and
  every `[iNNN]` link reference is defined (`[i625]` at `:1919`) — no dangling link, no missing
  row. So leg A1's green is real but shallow, exactly as the brief bounds it.
- **The gating judgment leg is already RED, not green.** `check-gates.json` records
  `T4-batch-review = fail` (7 blocking, 0 recorded-rejected) and `overall = fail`. There is no
  confirmatory "green" verdict for me to overturn here — the deterministic gate already blocks
  publish. My job below is only to add grounded, concrete findings on top of that.

## Findings

- **NEEDS-HUMAN [impl] — `0016:522`, `:1816`, `:396`, `:1492`: restore-fencing a *Completing*
  session that has already written segments is an undefined transition, and the resulting stray
  `seg:<id>:E:*` records have no reclamation path — outcome (a), and absent from the register.**
  D-B requires the restore pass to "fence every restored **`Open`/`Completing`** session to
  `Aborting`" (`:522`, `:1816`). But the state machine gives a `Completing` session only two
  exits — `→ Completed` (flip) and `→ Open` (reaper rollback) — and the batch inventory defines
  **no** `Completing → Aborting` batch: the abort/reap-fence row is `require(mpu == Open@E)`,
  with `Completing@E` named only "for the **rollback**" to `Open` (`:396`). So "fence a
  Completing session to Aborting" is a transition the rest of the document never defines. The
  concrete trace: snapshot a session in `Completing@E` **after** its segment-write phase
  (`seg:<id>:E:*` durable) but **before** the root flip → restore → D-B fences it to `Aborting`.
  The `seg:` deleters are only `retire:records:{seg}` drain or supersede/delete of the owning
  *committed* inode (`:222` deleter column); neither fires here (no flip ⇒ no committed inode; and
  the register/backward-compat text never says the restore-fence installs
  `retire:records:{seg:<id>:E}`). The `Aborting` exit reclaims `retire:bytes:` + `sidx:` only
  (`:340`), not `seg:`. Result: the segment **records** dangle unreclaimed (their fragments are
  orphan-marked via `retire:bytes:{session}`, so after grace the records point at deleted
  fragments and just accumulate in `seg:`). The register's restore rows (X17 `:1492`, X17b `:1493`)
  both stipulate an **`Open`** session — the `Completing`-with-segments case is not enumerated.
  Builder fix is document-local: either route restore's `Completing` sessions through the
  rollback-to-`Open` path first (which *does* install `retire:records:{seg:<id>:E}`, §7c/`:1306`)
  then fence `Open → Aborting`, or define a real `Completing → Aborting` restore transition whose
  batch installs the epoch-scoped seg cleanup — and add the matching X-row + §2 edge.

- **NEEDS-HUMAN — `0016:1120-1127`, `:1710`: the derived `MAX_SESSIONS` collapses to ≈ 1 for the
  large-part deployments the launch scope targets — a fitness-to-purpose call the human should
  confirm, not a defect the builder can close.** The arithmetic is honest and computed (leg B(iv)
  is satisfied), so this is **not** a refutation: at `MAX_PART_CHUNKS = 381`,
  `U_ref ≈ 3.82 M` ⇒ `MAX_SESSIONS = ⌊W_ref/U_ref⌋ ≈ 1` at `W_ref = 4 M`, and reaching even 32
  concurrent large-part uploads needs `W_ref ≈ 122 M` chunk-refs ("tens of GB" of reconcile-host
  RAM). The document is upfront about this. But "the release that ships multipart MUST support
  objects over 10 GiB" (§9 scope change) plus "a single fleet may run ~1 concurrent large-object
  upload unless the operator provisions tens of GB of reconcile RAM" is a launch-capacity trade
  the maintainer should explicitly bless — flagging it here so it reaches §6 rather than being
  read as settled by silence. No `[impl]` tag: the builder cannot fix it by iterating; only a
  human can decide the trade is acceptable (or send it back to re-scope `W_ref`/`MAX_PART_CHUNKS`).

## Refutations attempted that I could NOT land (the load-bearing core held)

I attacked each of these with a concrete interleaving and the register/mechanism answered:

- **Late-landing fragment after teardown (X49, `:861-887`).** Tried to strand a fragment
  authorized-before-fence that lands after the reaper deleted its `sidx:` entry. Defeated by
  full-`staged`-placement pre-marking + the **strict** `G_orphan > W_write + δ_clock` coupling
  (I verified GC's grace test is the inclusive `≥` the doc claims, `gc.rs:171-176`), and I could
  not construct a landing time past the position's grace given `t_auth < t_fence ≤ t_mark`.
- **Rollback → re-Complete → publish while the prior `retire:records:{seg}` still pends (X40,
  `:1312-1328`).** Per-attempt (epoch-scoped) `seg:<id>:E:*` vs `seg:<id>:E':*` are disjoint
  ranges; draining epoch-E deletes nothing epoch-E' published. Held.
- **Repoint vs supersede/delete of a committed segmented `seg:` fragment (X47, `:1523`).**
  Destination pre-mark + dual `require(seg==prior) && require(inode==prior)` + drain re-reading
  current placement closes both CAS branches. Held on both interleavings.
- **Exactly-once `mpuctl:count` decrement under gateway-inline vs reaper teardown (X42, `:403`,
  `:1518`).** The `require(mpu:<id> == prior)` session precondition makes the terminal batch
  single-winner; I could not force a double-decrement or a low-drift.
- **Unbounded owned `sidx:` residue from crash-looped parts (F11a, X41, `:940-974`).** `sinf:`
  counts crashed slots (never released while `Open`), so `503` fires at the cap before the
  per-session teardown scan can approach `SCAN_CAP`. Held.
- **Terminal delete racing a re-created owned entry (X43, `:1519`).** Every `sidx:` intent
  carries `require(mpu == Open@E)`; nothing refills the walked-empty range post-fence. Held.
- **Segmented-GET tear when `seg:` records are deleted mid-resolution (X51, `:1400-1422`).**
  Root-flip-before-`seg:`-delete ordering + resolve-retry (absent segment ⇒ re-read root;
  unchanged-root-with-absent-segment ⇒ fail-closed) closes it clock-free. Held.

I also probed the two namespaces the design admits are *not* cardinality-bounded — `retire:`
(paginated `scan_page` + oldest-obligation-age alarm) and the committed reference build (X48,
`W_ref_committed` telemetry). Both are disclosed as capacity/operational costs with a bounded
reclamation path (a backlogged drain is not a *stranding*), so neither is a Refutation-standard
outcome; I note them only to show they were attacked, not as findings.

The ⚑ per-session part-boundary serialization cost (`:1855`) is explicitly kept as a flagged
NEEDS-HUMAN sign-off question per the iteration-5 direction — settled as the human's to rule on,
so I did not spend a refutation on it.

## Net

One concrete, builder-fixable enumeration gap (restore × `Completing`-with-segments) and one
human fitness call (`MAX_SESSIONS ≈ 1` at launch part sizes). The fence/epoch machine,
records-as-proof, restore fence, epoch-scoped segments, exactly-once decrement, and the
byte-budgeted batch inventory survived every interleaving I could build — consistent with the
brief's instruction to preserve them. The gating `T4-batch-review` is already red independently.
