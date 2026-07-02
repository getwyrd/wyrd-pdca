# shellcheck shell=bash
# ensure_cargo — make `cargo` resolvable regardless of how a gate was launched.
#
# Why this exists (the "toolchain everywhere" problem): the PDCA driver runs every gate
# command through a Python subprocess that inherits ONLY the launching shell's PATH
# (`gates.py` → `{**os.environ, ...}`). Interactive shells have ~/.cargo/bin on PATH
# because rustup drops `. "$HOME/.cargo/env"` into the login profile — but non-interactive
# launchers (CI runners, cron, systemd, a bare `sh -c`, the agent sandbox) frequently do
# NOT. There, `cargo xtask ci` / `cargo test` die with a bare "cargo: command not found"
# even though rustup is installed and the toolchain is pinned (../wyrd/rust-toolchain.toml).
#
# This shim is the single place that closes that gap: if `cargo` isn't already on PATH,
# source rustup's own env file (`${CARGO_HOME:-$HOME/.cargo}/env`), which prepends
# ${CARGO_HOME:-$HOME/.cargo}/bin. The pinned toolchain then resolves as usual (rustup's
# proxies read rust-toolchain.toml). If cargo is STILL unresolved, fail with an actionable
# message instead of an opaque shell error.
#
# Sourced (not exec'd) by engine/xtask.sh and engine/scripts/run-verify.sh right before
# their first cargo use. Defining the function is side-effect-free; nothing happens until
# a caller runs `ensure_cargo`, so pure test hooks that never touch cargo stay pure.
ensure_cargo() {
  command -v cargo >/dev/null 2>&1 && return 0
  local env_file="${CARGO_HOME:-$HOME/.cargo}/env"
  if [ -f "$env_file" ]; then
    # shellcheck source=/dev/null
    . "$env_file"
  fi
  command -v cargo >/dev/null 2>&1 && return 0
  echo "${0##*/}: cargo not found on PATH, and no rustup env at '$env_file'." >&2
  echo "          Install Rust (https://rustup.rs) or put cargo on PATH so the Wyrd gate can run." >&2
  return 127
}
