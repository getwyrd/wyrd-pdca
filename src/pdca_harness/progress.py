"""Run a subprocess while ticking an elapsed-time heartbeat (docs 03 §automation).

A headless ``claude -p`` leaf and a Docker-backed gate both produce no output for
minutes; without a heartbeat the flow looks hung and the human kills a job that is
working. This is the single place that pattern lives — shared by the model leaves
(:mod:`pdca_harness.leaves`) and the deterministic gates (:mod:`pdca_harness.gates`).

A ``status`` probe lets the heartbeat show *what* is happening (which artifacts exist
yet, how long since the last write), not just that time passed.
"""

from __future__ import annotations

import json
import os
import re
import signal
import subprocess
import sys
import threading
import time
from collections import deque
from collections.abc import Callable, Iterable
from pathlib import Path


def _kill_tree(proc: subprocess.Popen) -> None:
    """Kill a timed-out child and everything it spawned — SIGTERM, then SIGKILL.

    The child was started with ``start_new_session=True``, so its pid IS its process
    group id and one ``killpg`` reaches the whole tree. That matters most under
    ``shell=True``, where ``proc`` is only the shell: signalling it alone leaves the
    real work (a ``cargo`` build tree, a container) running and holding the CPU.
    Falls back to the direct child if the group is already gone.

    SIGKILL is sent UNCONDITIONALLY after the SIGTERM grace, never skipped just because
    the leader has exited: under ``shell=True`` the leader is only the shell, and a shell
    exits on SIGTERM while the ``cargo`` tree it spawned — which may ignore SIGTERM — keeps
    running and keeps the capture pipe open. Waiting on the leader answers "did the shell
    die", not "is the work over", so the second signal has to go out regardless.
    """
    # Resolve the group ONCE, while the leader is still alive. Re-reading it per signal
    # would break exactly the case this exists for: SIGTERM kills the shell, `wait` reaps
    # it, and `getpgid` then raises ProcessLookupError — so the SIGKILL would be aimed at
    # a dead pid instead of the group, and the survivors that ignored SIGTERM would live.
    # The child was started with `start_new_session`, so its pid IS the group id.
    try:
        pgid = os.getpgid(proc.pid)
    except (ProcessLookupError, PermissionError):
        pgid = proc.pid
    for sig, grace in ((signal.SIGTERM, 10), (signal.SIGKILL, 5)):
        try:
            os.killpg(pgid, sig)
        except (ProcessLookupError, PermissionError):
            pass  # group already gone — nothing left to signal
        try:
            proc.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            continue


def run_with_heartbeat(
    cmd,
    *,
    cwd=None,
    shell: bool = False,
    env=None,
    input_text: str | None = None,
    capture: bool = False,
    stream_json: bool = False,
    tee_stderr: bool = False,
    stream_format: str = "claude-stream-json",
    interval: int = 15,
    timeout: int | None = None,
    label: str = "",
    status: Callable[[], str] | None = None,
) -> tuple[int, str, bool]:
    """Run ``cmd``, printing ``… still working (NmSSs elapsed)`` every ``interval`` s.

    Returns ``(returncode, output, produced)``. ``output`` is the combined
    stdout+stderr when ``capture`` is True (so a gate can keep its evidence line);
    the bounded **stderr tail** when ``stream_json`` or ``tee_stderr`` is set (so a
    failed leaf's real error — usage/rate limit, 5xx, auth — survives in the bundle
    instead of scrolling past on a console nobody is watching); ``""`` otherwise.
    ``produced`` is whether the child emitted a **substantive** stream event — an
    ``assistant`` / ``user`` / ``result`` event, i.e. a session that did real work.
    Claude emits a ``system``/``init`` event (and ``system``/``api_retry`` on a
    retryable API error) *before* doing anything, so those do NOT count: a non-zero
    exit with ``produced is False`` is the transient-infra signal (the child died
    at/near invocation — usage/rate limit, 5xx, auth — before any real output).
    ``input_text``, if given, is written to stdin.

    ``timeout``, if given, is a HARD wall-clock bound in seconds. On expiry the child
    is killed and :class:`subprocess.TimeoutExpired` is raised, so a hung command can
    never hold a beat open indefinitely (issue #187 / eduralph/pdca-harness#368 — a
    hung advisory gate once held one Check for 19 hours while this very heartbeat
    printed ``… still working``). The child is started in its **own process session**
    when a bound is set, and the whole group is signalled: under ``shell=True`` the
    direct child is only a shell, so killing it alone would orphan the real work
    (a ``cargo`` tree that keeps running and keeps the CPU). SIGTERM first so the
    command can clean up, SIGKILL after a short grace.

    ``status``, if given, is called on every tick to append a live snapshot of the
    child's work (e.g. which artifacts exist yet, time since the last write) — so the
    heartbeat shows *what* is happening, not just that time passed (Tier 1+2). It is
    best-effort: any exception it raises is swallowed so a probe can never break the run.

    ``stream_json`` (Tier 3) parses the child's stdout as Claude's
    ``--output-format stream-json`` event stream and surfaces the **tool it is using
    right now** (``▸ Editing patch.diff`` / ``▸ Running run-tests``) on each tick.
    stdout is consumed for parsing (not echoed); stderr is **teed** — still echoed
    live so real errors show, *and* its tail retained for the caller. Mutually
    exclusive with ``capture`` (capture wins if both set).

    ``tee_stderr`` asks for that same stderr tee **without** the stream parse, for a
    family that has no event stream (``generic``, ``gemini``): stdout keeps inheriting
    the terminal (its output stays live, exactly as before), but stderr is piped, echoed,
    and its tail returned — so a stream-less leaf's failure is diagnosable too, and not
    only claude's. Implied by ``stream_json``; ignored under ``capture`` (which already
    keeps everything).
    """
    tee_err = (stream_json or tee_stderr) and not capture
    capture_out = capture or stream_json
    stdin = subprocess.PIPE if input_text is not None else None
    if capture:
        stdout, stderr = subprocess.PIPE, subprocess.STDOUT
    elif stream_json:
        stdout, stderr = subprocess.PIPE, subprocess.PIPE  # parse stdout; tee stderr
    elif tee_err:
        stdout, stderr = None, subprocess.PIPE  # stdout stays live; tee stderr only
    else:
        stdout, stderr = None, None
    proc = subprocess.Popen(
        cmd, cwd=cwd, shell=shell, env=env, text=True,
        stdin=stdin, stdout=stdout, stderr=stderr,
        # Own session ONLY when a bound is set, so the kill can reach the whole tree.
        # Unconditionally detaching would break the interactive leaves: a process in a
        # different session has no controlling terminal, so a leaf that reads or writes
        # the tty would take SIGTTIN/SIGTTOU instead of talking to the human.
        start_new_session=timeout is not None,
    )

    chunks: list[str] = []
    err_tail: deque[str] = deque(maxlen=200)  # bounded stderr tail for a failed leaf
    produced = {"session": False}  # did a substantive stream event arrive (real work)?
    latest_tool = {"label": ""}  # most recent tool-use, updated by the drain thread
    readers: list[threading.Thread] = []
    if capture_out:
        def _drain() -> None:
            assert proc.stdout is not None
            for line in proc.stdout:  # drain so the pipe can't fill and stall the child
                if capture:
                    chunks.append(line)
                if stream_json:
                    if _is_session_event(line, stream_format):
                        produced["session"] = True  # a startup/init line does NOT count
                    lbl = _stream_tool_label(line, stream_format)
                    if lbl:
                        latest_tool["label"] = lbl
        t = threading.Thread(target=_drain, daemon=True)
        t.start()
        readers.append(t)
    if tee_err:
        def _drain_err() -> None:
            assert proc.stderr is not None
            for line in proc.stderr:  # echo live (errors still show) AND keep the tail
                sys.stderr.write(line)
                sys.stderr.flush()
                err_tail.append(line)
        t = threading.Thread(target=_drain_err, daemon=True)
        t.start()
        readers.append(t)

    if input_text is not None:
        try:
            assert proc.stdin is not None
            proc.stdin.write(input_text)
            proc.stdin.close()
        except BrokenPipeError:
            pass

    suffix = f" — {label}" if label else ""
    start = time.monotonic()
    try:
        while True:
            try:
                # Wake at whichever comes first, the next tick or the deadline — waiting a
                # full `interval` first would let the advertised HARD bound overrun by up
                # to `interval` seconds (15 by default, and a caller may set it higher).
                wait_for = interval
                if timeout is not None:
                    wait_for = max(0.1, min(interval, timeout - (time.monotonic() - start)))
                proc.wait(timeout=wait_for)
                break
            except subprocess.TimeoutExpired:
                elapsed = time.monotonic() - start
                if timeout is not None and elapsed >= timeout:
                    _kill_tree(proc)
                    _close_streams(proc, readers)
                    raise subprocess.TimeoutExpired(cmd, timeout)
                mins, secs = divmod(int(elapsed), 60)
                bits: list[str] = []
                if stream_json and latest_tool["label"]:
                    bits.append(f"▸ {latest_tool['label']}")
                if status is not None:
                    try:
                        snap = status()
                        if snap:
                            bits.append(snap)
                    except Exception:  # a status probe must never break the run
                        pass
                extra = (" · " + " · ".join(bits)) if bits else ""
                print(f"   … still working ({mins}m{secs:02d}s elapsed){suffix}{extra}",
                      file=sys.stderr, flush=True)
    except BaseException:
        # Ctrl-C, or any caller-side abort. A BOUNDED child is in its OWN session, so the
        # terminal's SIGINT never reached it: without this it outlives the driver that
        # started it — the very orphaned-gate-tree failure the bound exists to prevent.
        # An unbounded child shares the driver's group and already took the signal.
        if timeout is not None:
            _kill_tree(proc)
            _close_streams(proc, readers)
        raise
    _close_streams(proc, readers)
    output = "".join(chunks) if capture else ("".join(err_tail) if tee_err else "")
    return proc.returncode, output, produced["session"]


def _close_streams(proc: subprocess.Popen, readers: list[threading.Thread]) -> None:
    """Join the drain threads and close the child's pipes — the one teardown both the
    normal exit and the timeout/interrupt paths use, so no path leaks an open pipe."""
    for reader in readers:
        reader.join(timeout=5)
    for stream in (proc.stdout, proc.stderr):
        if stream is not None:
            stream.close()


# ----------------------------------------------------------------------------
# Tier 3 — parse a leaf's JSONL event stream for the live tool-use surfaced on each
# heartbeat tick. Vendor event shapes differ, so the parsers dispatch on the family's
# ``stream_format``; a family whose format isn't in ``STREAM_FORMATS`` runs stream-less
# (Tiers 1+2 only). claude: ``--output-format stream-json``; codex: ``exec --json``.
# ----------------------------------------------------------------------------
STREAM_FORMATS = frozenset({"claude-stream-json", "codex-stream-json"})

_SESSION_EVENT_TYPES = frozenset({"assistant", "user", "result"})
# codex `exec --json`: real work is an item/turn event; thread.started / turn.started
# are startup-only (like claude's ``system`` init), so they don't count as "produced".
_CODEX_SESSION_TYPES = frozenset({"item.started", "item.completed", "turn.completed"})


def _is_session_event(line: str, fmt: str = "claude-stream-json") -> bool:
    """True iff a stream line is **substantive work** — not a startup/init event the CLI
    emits before doing anything. A non-zero exit having produced no such event is the
    transient-infra signal a retry should target (#138). Best-effort: non-JSON → False."""
    try:
        ev = json.loads(line)
    except (ValueError, TypeError):
        return False
    if not isinstance(ev, dict):
        return False
    if fmt == "codex-stream-json":
        return ev.get("type") in _CODEX_SESSION_TYPES
    return ev.get("type") in _SESSION_EVENT_TYPES


def _stream_tool_label(line: str, fmt: str = "claude-stream-json") -> str:
    """A human label for the tool-use in one stream line, or "" if none.
    Best-effort: a non-JSON / non-tool line yields ""."""
    try:
        ev = json.loads(line)
    except (ValueError, TypeError):
        return ""
    if not isinstance(ev, dict):
        return ""
    if fmt == "codex-stream-json":
        return _codex_item_label(ev)
    # claude: an ``assistant`` event's message content can hold ``tool_use`` blocks;
    # surface the LAST one in the line (the tool just invoked).
    if ev.get("type") != "assistant":
        return ""
    content = (ev.get("message") or {}).get("content") or []
    for block in reversed(content):
        if isinstance(block, dict) and block.get("type") == "tool_use":
            return _tool_label(block.get("name", ""), block.get("input") or {})
    return ""


def _codex_item_label(ev: dict) -> str:
    """Label a codex ``exec --json`` item event (command_execution / file_change), or "".

    Codex emits ``item.started`` / ``item.completed`` with an ``item`` carrying its type;
    an ``agent_message`` item is prose, not a tool, so it yields ""."""
    if ev.get("type") not in ("item.started", "item.completed"):
        return ""
    item = ev.get("item") or {}
    kind = item.get("type")
    if kind == "command_execution":
        cmd = str(item.get("command") or "")
        # unwrap a `/bin/bash -lc '<cmd>'` wrapper to the inner command's first line
        m = re.search(r"-lc?\s+'(.*)'", cmd, re.S)
        inner = (m.group(1) if m else cmd).strip().splitlines()
        first = inner[0] if inner else ""
        return f"Running {first[:48]}" if first else "Running a command"
    if kind == "file_change":
        changes = item.get("changes") or []
        if changes and isinstance(changes[0], dict):
            path = Path(str(changes[0].get("path") or "")).name
            verb = {"add": "Adding", "delete": "Removing"}.get(changes[0].get("kind"), "Editing")
            return f"{verb} {path}" if path else "Editing files"
        return "Editing files"
    return ""


def _tool_label(name: str, inp: dict) -> str:
    """Compact description of a tool call — what the leaf is doing right now."""
    base = Path(str(inp.get("file_path") or inp.get("path") or "")).name
    if name in ("Edit", "MultiEdit", "Write", "NotebookEdit"):
        return f"Editing {base}" if base else name
    if name == "Read":
        return f"Reading {base}" if base else "Reading"
    if name == "Bash":
        first = (inp.get("command") or "").strip().splitlines()
        cmd = first[0] if first else ""
        return f"Running {cmd[:48]}" if cmd else "Running a command"
    if name in ("Grep", "Glob"):
        pat = str(inp.get("pattern") or inp.get("query") or "")
        return f"Searching {pat[:32]}" if pat else "Searching"
    if name in ("Task", "Agent"):
        desc = str(inp.get("description") or "").strip()
        return f"Subagent: {desc[:32]}" if desc else "Subagent"
    return name or "working"


# ----------------------------------------------------------------------------
# Status probe — what a leaf/gate is doing right now: which artifacts exist in the
# watched dir, and how long since the newest write (a stalled job stops writing).
# Project-agnostic; a project whose leaves run a long containerized job can extend
# this with a runner probe (e.g. `docker ps --filter name=<your-prefix>`).
# ----------------------------------------------------------------------------
def bundle_activity(watch_dir, expected: Iterable[str] = ()) -> str:
    """A one-line snapshot of the work in ``watch_dir`` for a heartbeat tick.

    Reports each ``expected`` artifact (``name ✓ <size>`` once written, else
    ``name —``), then how long since the newest write in the dir (``last write 12s
    ago`` / soft ``⚠ no writes 6m`` once a leaf has gone quiet for ≥5 min) — so the
    human can see a leaf is still producing, or has stalled. Best-effort — returns
    ``""`` on any error.
    """
    try:
        watch = Path(watch_dir)
        parts: list[str] = []

        arts = [
            f"{name} ✓ {_fmt_size((watch / name).stat().st_size)}"
            if (watch / name).exists() else f"{name} —"
            for name in expected
        ]
        if arts:
            parts.append(" · ".join(arts))

        newest = _newest_mtime(watch)
        if newest:
            age = int(time.time() - newest)
            if age >= 300:
                parts.append(f"⚠ no writes {age // 60}m")
            elif age >= 120:
                parts.append(f"last write {age // 60}m ago")
            else:
                parts.append(f"last write {age}s ago")
        return " · ".join(p for p in parts if p)
    except Exception:
        return ""


def _fmt_size(n: int) -> str:
    if n < 1024:
        return f"{n}B"
    if n < 1024 * 1024:
        return f"{n / 1024:.1f}KB"
    return f"{n / 1024 / 1024:.1f}MB"


def _newest_mtime(watch: Path) -> float:
    newest = 0.0
    try:
        for f in watch.iterdir():
            if f.is_file():
                newest = max(newest, f.stat().st_mtime)
    except OSError:
        return 0.0
    return newest
