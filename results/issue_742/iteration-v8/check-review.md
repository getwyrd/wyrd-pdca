# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `shellcheck` confirmed absent (detect cmd `shellcheck --version` exited 127). Install hint: apt-get install shellcheck — release.yml lints deploy/dist/install.sh with it; without it a builder cannot check an installer edit before the release does. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
