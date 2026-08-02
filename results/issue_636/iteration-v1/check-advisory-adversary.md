# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-adversary.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
