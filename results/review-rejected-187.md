# Recorded review rejections — issue #187 (gate timeout + Check-leaf resume)

Format: `<file:line> | <CLASS> | <MATCH> | <reason>`. Every other finding from the
batched review pass was **fixed**; only the row below is rejected, with the citation
that settles it. Recorded so a later review round does not re-raise it.

src/pdca_harness/progress.py:36 | SCOPE | **"assumes POSIX-only `SIGKILL`, `getpgid`, and
`killpg`, so on supported Windows installations a timeout leaves the process running"** |
Windows is not a supported installation. The harness runs every gate through
`shell=True` against POSIX shell commands, its own tooling is bash
(`scripts/bootstrap-tools.sh`, `scripts/flow`, `engine/xtask.sh`, `engine/tests/*.sh`),
and `start_new_session` — the flag this teardown pairs with — is itself POSIX-only, as is
the process-group model the whole design rests on. A Windows port is a much larger piece
of work than one kill path, and pretending to support it here would leave a bound that
silently does not bind. If Windows support is ever taken on, this teardown is one of many
places that need a platform branch, and it should be done as that work rather than
guessed at now.
