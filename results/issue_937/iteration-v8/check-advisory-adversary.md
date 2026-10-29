# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-adversary.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '--property', 'MemoryMax=8G', '--property', 'MemorySwapMax=0', '--property', 'ManagedOOMMemoryPressure=kill', '--', 'claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
