#!/usr/bin/env bash
# Instance gate-toolchain hook (issue #207 tier 3; added for #105) — the Protocol Buffers
# compiler, an OPTIONAL prerequisite.
#
# Who actually needs it: only the OFF-by-default `etcd` feature. The real `etcd-client` 0.14
# REGENERATES its etcd protobufs at build time and so needs a system `protoc` — i.e.
# `cargo xtask etcd-conformance`, and any bundle that compiles `--features etcd`. Its absence
# cost wyrd#365 two full iterations (the builder could not compile the etcd store, so the
# single-leader/no-split-brain correctness was verified by code-reading only).
#
# Who does NOT need it: the default gate. `crates/proto/build.rs` compiles the .proto with
# **protox**, a pure-Rust frontend, "so the build needs no system protoc" (ADR-0016) — and
# under `--cfg madsim` it uses `skip_protoc_run`, still protoc-free. So `cargo xtask ci`
# (the gating C4-ci check) never needs protoc.
#
# Therefore this hook ALWAYS EXITS 0, unlike rust.sh. bootstrap-tools.sh treats a non-zero
# hook exit as a REQUIRED miss and fails `--check` on it; protoc is not required, and an
# operator who never touches the etcd feature must not be blocked. It best-effort provisions
# and reports; it never fails the bootstrap. Matches the WARN doctor row in pdca.toml.
set -uo pipefail
CHECK_ONLY="${CHECK_ONLY:-0}"
have() { command -v "$1" >/dev/null 2>&1; }

if have protoc; then
  echo "  protoc: OK ($(protoc --version 2>&1))"
  exit 0
fi

_absent_note() {
  echo "  protoc: MISSING (optional) — only the OFF-by-default \`etcd\` feature needs it"
  echo "          (etcd-client regenerates its protobufs at build time): \`cargo xtask"
  echo "          etcd-conformance\`, or a bundle compiling \`--features etcd\` (#105, wyrd#365)."
  echo "          The gating \`cargo xtask ci\` does NOT need it (crates/proto uses protox)."
  echo "          Install: apt-get install -y protobuf-compiler"
}

if [ "$CHECK_ONLY" = 1 ]; then
  _absent_note
  exit 0   # optional prerequisite — never a required miss
fi

# Provision best-effort. Never prompt: a bootstrap runs non-interactively, so only elevate
# when that can happen without a password.
if have apt-get; then
  if [ "$(id -u)" = 0 ]; then
    echo "  protoc: installing (apt-get protobuf-compiler)"
    apt-get install -y protobuf-compiler >/dev/null 2>&1 || true
  elif have sudo && sudo -n true 2>/dev/null; then
    echo "  protoc: installing (sudo apt-get protobuf-compiler)"
    sudo apt-get install -y protobuf-compiler >/dev/null 2>&1 || true
  fi
fi

if have protoc; then
  echo "  protoc: installed ($(protoc --version 2>&1))"
else
  _absent_note
fi
exit 0
