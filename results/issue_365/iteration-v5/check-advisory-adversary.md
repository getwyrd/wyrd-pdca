# Adversarial review — issue 365 / coordination-etcd-l5-backend (iteration 5)

Skeptic's pass. Grounded on the target tree at `/home/eddie/wyrd/wyrd.pdca-wt-l0`.
Advisory only — nothing here gates. The prior four rejections drove the crate to a
genuinely stronger place (store compiled+driven under madsim; split-brain guard gated
demonstrated-red; single-leader now checked in the real-etcd conformance too). Below is
what I could still break and what I tried but could not.

## Refutations (the fix)

- **NEEDS-HUMAN — `election_name` reintroduces exactly the hierarchical-prefix-collision
  bug that iteration 4 forced the builder to fix for *registration* — but only registration
  got the fix.** `crates/coordination-etcd/src/keyspace.rs:58` (`election_name`) formats the
  raw caller key: `format!("{ns}{ELECT}{key}")`, with **no** `encode_segment`, while
  `registration_member`/`registration_prefix` (keyspace.rs:43,48) *do* encode it precisely to
  stop `/` in a logical key forging an etcd key-prefix nesting. etcd's election API (used at
  `store.rs:324-329`) determines leadership by the **min-create-revision under the prefix
  `<name>/`** and its candidate keys live under that prefix — this diff's own test confirms it
  (`crates/dst/tests/coordination.rs:399-404` reads candidate keys with
  `get("lapse/elect/custodian", with_prefix())`). Concrete failing case: an instance holding
  `elect_leader("custodian")` (candidate under `…/elect/custodian/`) and any instance calling
  `elect_leader("custodian/shard-a")` (candidate `…/elect/custodian/shard-a/<hex>`, which
  *starts_with* `…/elect/custodian/`). The child's candidate now falls inside the parent
  election's leadership range; if it holds the lower create-revision, the parent election
  `waitDeletes` behind a key that will never resign for it — the parent's `elect_leader`
  blocks or resolves to the wrong holder. The trait's `elect_leader(&self, key: &str)` places
  no restriction on keys, and the registration path was defended for exactly this arbitrary-
  hierarchical-key case; elections were not. A human must decide whether hierarchical election
  keys are in-contract (then this is a defect: encode the segment as registration does, adding
  the mirror of `discovery_prefix_isolates_hierarchical_keys` for elections) or explicitly
  out-of-contract (then document + assert the restriction). It is caught by **no** test on
  either backend — every election clause uses flat keys (`"custodian"`, `"y"`).

- **The gated green never runs the code path this bug lives on — criterion (b) rests on the
  simulator alone.** `check-gates.json:33-48`: the only gating row is `C4-ci`, and `C4-verify`
  is non-gating with `path_line` admitting *"no pre-patch state to isolate a RED against."*
  The real-etcd conformance (`crates/coordination-etcd/tests/conformance.rs`) is skipped unless
  `WYRD_ETCD_ENDPOINTS` is set (:35-41), and `cargo xtask etcd-conformance` hard-requires
  docker **and** system `protoc` (`xtask/src/main.rs:293-312`) — neither present in a PDCA
  worktree, so it was not run here. Brief criterion (b) is *"the shared suite is green on both
  backends (real etcd via a Tier-2 compose target)."* No artifact in `check-gates.json` shows a
  real-etcd run; the green is the **madsim simulator's** (dst tier). Any reviewer claim that (b)
  is *satisfied* (as opposed to *earnable*) is unwarranted on this evidence, and the election-
  prefix defect above is precisely the class of real-etcd behaviour a simulator can mask.

## Attempted but could not refute

- **Real-etcd single-leader (the iteration-4 reject).** Now genuinely present at
  `crates/coordination-etcd/tests/conformance.rs:102-109`, and it is robust: a `b.elect_leader`
  that returned `Err` would `unwrap()`-panic (line 107 `.map(|r| r.unwrap())`), and a wrong
  concurrent `Ok` fails `is_none()` — the only pass path is B genuinely blocking. Could not
  turn it into a false green.
- **Orphaned cancelled campaign / detached keep-alive leak** (iterations 1–2 reject): the guard
  now lives on the campaign future's stack and `Drop`/`stop_without_revoke` revoke on
  cancellation (`store.rs:95-104,309-342`), gated by `a_cancelled_campaign_leaks_no_orphan`
  (dst/tests/coordination.rs:332-365). Held.
- **Transient-proclaim-error self-churn** (iterations 3–4 reject): loss is read only from the
  keep-alive's authoritative `is_lost()` (`store.rs:76,285-304`), and
  `a_transient_proclaim_error_keeps_the_hold_and_its_lease` (dst:448-501) pins that a proclaim
  error retains the live lease. Held.
- **`config_revision` config-only advancement** (iteration 3): etcd path takes `max(mod_revision)`
  over the config prefix (`store.rs:418-441`), and `contract_config_is_revisioned:219-228`
  asserts an unrelated `register` does *not* move it — non-vacuous and correct for the etcd
  keyspace layout.
- **`unlock` releasing a newer holder / registration prefix bleed** (iterations 1,4): `unlock`
  revokes only its own lease (`store.rs:386-398`) and `registration_member` is `/`-and-`%`
  encoded (keyspace.rs:43-50, tests :94-127). Held. Note `lock_key`/`config_key` are also
  unencoded but are used with **exact-key** ops (no prefix range), so unlike elections they do
  not collide — benign, mentioned only for completeness.
