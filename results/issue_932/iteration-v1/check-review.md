# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '--property', 'MemoryMax=16G', '--property', 'MemorySwapMax=0', '--property', 'ManagedOOMMemoryPressure=kill', '--', 'codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check', '-m', 'gpt-6-astra', '-c', 'model_reasoning_effort=xhigh', '-c', 'sandbox_workspace_write.network_access=true', '--json']' returned non-zero exit status 1.).

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
