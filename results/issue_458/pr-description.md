# Add `--advertise-addr` to `wyrd d-server`

## Summary
**User impact:** A `wyrd d-server` deployed in a container (or behind NAT) tells
other nodes to reach it at the internal address it listens on — e.g.
`0.0.0.0:50051` — which nothing outside the container can actually connect to. Any
deployment that relies on nodes *discovering* d-servers this way cannot route to
them; today only static, hand-configured endpoints work.

This adds an `--advertise-addr ADDR` flag to `wyrd d-server` so a server can
publish a routable address (a DNS service name or a NAT-mapped address) for
discovery, separate from the address it binds. Omitting the flag keeps today's
behaviour exactly.

Reported in #458.

## What to look at
The new flag on `wyrd d-server`. With `--advertise-addr dserver1:50051`, a server
bound to `0.0.0.0:50051` publishes `http://dserver1:50051` for discovery instead
of its un-reachable bind address. To try it: start a d-server with
`--bind 0.0.0.0:0 --advertise-addr <name:port>`, register it, and look up the
record — the discovered endpoint is the advertised one. Without the flag, the
discovered endpoint is the bound address, exactly as before. The
`deploy/small-multi-node` compose file now sets this per service, so its
d-servers publish reachable names.

## Root cause
The endpoint a d-server publishes for discovery was always derived from its bound
socket address, with no seam to override it. A containerized `--bind 0.0.0.0:PORT`
therefore registered the wildcard address, and since `--bind` only accepts a
numeric socket address, a routable DNS name could not be substituted as a
workaround.

## Fix
Add a `with_advertise_addr` builder on `DServer` that overrides the endpoint used
for the registration record, mirroring the existing `with_identity` builder.
Thread a new `--advertise-addr` CLI flag through to it: when set, the server
registers `http://<advertise>`; when unset, the registration record keeps the
bound-address value. Set `--advertise-addr dserverN:50051` per d-server in the
`deploy/small-multi-node` compose file. The listen socket, the `--bind` type, and
the `http://` scheme derivation are all unchanged.

## Verification
- **Claim:** A d-server bound to a wildcard/loopback address but given a distinct
  advertise address registers *that* advertised endpoint for discovery; with no
  advertise flag set, the registered endpoint stays the bound-address value exactly
  as today.
- **Checked:** `crates/server/src/dserver.rs:217` — `with_advertise_addr` overrides
  the endpoint the registration record carries, which `registration`
  (`dserver.rs:250`) and `register` (`dserver.rs:261`) then publish; the flag is
  parsed at `crates/server/src/cli.rs:473` and applied before the server registers
  at `cli.rs:930`.
- **Test:** `crates/server/tests/advertise_addr_registration.rs` — two cases over an
  in-process coordination store: one asserts the discovered endpoint is the
  advertised `http://dserver-x:50051` (not the bound loopback address), the other
  asserts an unset flag preserves the bound-address registration. Fails pre-fix (the
  builder does not exist, so the test does not compile against the current surface),
  passes post-fix; the full `cargo xtask ci` suite is green. The live
  cross-container dial on the `deploy/small-multi-node` stack is confirmed separately
  by the maintainer.

Fixes #458
