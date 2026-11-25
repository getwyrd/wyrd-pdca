# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `root + device-mapper` confirmed absent (detect cmd `sudo -n dmsetup version` exited 1). Install hint: run on a host with passwordless sudo and device-mapper (dmsetup) for `cargo xtask disk-faults`. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
