# C4-ci gate adjudication — iteration 12's non-reproducing exit-101

**Verdict: the recorded gating red does not reproduce and is adjudicated environmental/transient.
The iteration-12 rejection stands on the adversary's substantive findings, not on this gate row.**

## Recorded failure (check-gates.json, iteration 12)

- `C4-ci` = fail, gating: `cargo test --workspace --exclude wyrd-dst` exit status 101.
- The driver retained only this one-line summary — no test output, no environment capture.

## Reproduction attempts, all green

| Run | By | Conditions | Result |
|---|---|---|---|
| 1–2 | adversary (iteration 12 review) | `$PDCA_TARGET` = `wyrd.pdca-wt`, full gate steps (fmt, clippy, test), twice | exit 0, all green |
| 3 | this adjudication, 2026-07-06T02:03:50+02:00 | same worktree, fingerprinted env (below), **with a live PD/TiKV v8.5.5 cluster occupying 127.0.0.1:2379/2380/20160** (`client-rust-test-cluster-*`, an unrelated project's cluster that was also up around the gate window) | exit 0, all green (`evidence/c4-gate-rerun.log`) |

Run 3 deliberately tested the "port-squatting cluster" hypothesis — the workspace test suite
(default features, tikv OFF) is indifferent to those ports being occupied. That hypothesis is
**eliminated** for the default-feature test step.

## Environment fingerprint (run 3)

- rustc/cargo 1.96.0 (2026-05-25); libprotoc 3.21.12 present
- `WYRD_TIKV_TOOLCHAIN` unset; **no** `WYRD_*` variables set
- Worktree: `/home/eddie/wyrd/wyrd.pdca-wt` @ `feat/m4.5-deploy-tikv-pd-etcd` + iteration-12 patch applied

## Remaining plausible causes (unrecoverable without the original output)

1. **Leaked host firewall state** from a prior live-leg run — the silent-lossy-heal defect the
   earlier iterations documented (`iptables -D` failure swallowed, rules leaked on loopback IPs).
   A leaked DROP on 127.0.0.x could fail any test using loopback networking at gate time.
2. A differently-configured box (`WYRD_TIKV_TOOLCHAIN` set → compiles the pre-1.0
   `tikv-client`/grpcio tree → a *different* failure than the recorded test-step 101).
3. Transient host pressure (OOM-killed test process reports as test failure → exit 101).

## Process consequence (carried to brief + Act candidates)

- Gate runs MUST capture full step output and an environment fingerprint (env `WYRD_*`, rustc,
  protoc, `docker ps`, `iptables -S` on the loopback IPs) into the bundle, so a red is
  adjudicable after the fact.
- Before any gate run on this box: verify no leaked partition rules
  (`iptables -S | grep 127.0.0.` must be empty) — cheap, and eliminates cause 1 going forward.
