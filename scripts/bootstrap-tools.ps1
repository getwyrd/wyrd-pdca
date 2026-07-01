# Windows parity for scripts/bootstrap-tools.sh (issue eduralph/pdca-harness#207):
# bootstrap EVERY tool `wyrd-pdca flow` needs, idempotently, on a host of unknown
# state. Each tool is probed with Get-Command (the `command -v` analogue) and only
# installed if absent, so re-running is a no-op. install.ps1 delegates to this.
#
#   pwsh -File scripts/bootstrap-tools.ps1           install what's missing, then venv + console script
#   pwsh -File scripts/bootstrap-tools.ps1 -Check    report each tool's status and exit (installs nothing)
#
# Tool classes mirror the bash script:
#   system   git, gh, python  -> `winget install` when available, else print the exact
#            install command and exit non-zero (never silently skip a REQUIRED tool).
#   user     rustup->cargo/rustc, claude, codex  -> official installers, no admin.
# REQUIRED: python+venv, git, gh, claude, cargo/rustc (gating C4-ci is `cargo xtask ci`).
# OPTIONAL: codex (advisory leaf is non-gating — a miss degrades to a §6 note).
param([switch]$Check)

$ErrorActionPreference = "Stop"
$Python = if ($env:PYTHON) { $env:PYTHON } else { "python" }
$script:MissingRequired = 0
$BinDir = Join-Path $HOME ".local\bin"

function Log  ($m) { Write-Host "[bootstrap] $m"        -ForegroundColor Blue }
function Ok   ($m) { Write-Host "[bootstrap] ok: $m"    -ForegroundColor Green }
function Warn ($m) { Write-Host "[bootstrap] warn: $m"  -ForegroundColor Yellow }
function Err  ($m) { Write-Host "[bootstrap] error: $m" -ForegroundColor Red }
function Have ($c) { [bool](Get-Command $c -ErrorAction SilentlyContinue) }
function Ver  ($c) { try { (& $c --version 2>&1 | Select-Object -First 1) } catch { "" } }
function Fail-Required { $script:MissingRequired = 1 }

# --- winget plumbing (system packages) --------------------------------------
function Winget-Install ($id) {
  if (-not (Have winget)) { return $false }
  # Pipe to Out-Host so winget's native stdout is shown but does NOT leak into the
  # function's return stream (PowerShell would otherwise fold it into the bool below).
  winget install -e --id $id --accept-source-agreements --accept-package-agreements --silent | Out-Host
  return ($LASTEXITCODE -eq 0)
}

# ensure_system <cmd> <winget-id> <manual hint> — REQUIRED system tool, else instruct+flag
function Ensure-System ($cmd, $wingetId, $hint) {
  if (Have $cmd) { Ok "$cmd present ($(Ver $cmd))"; return }
  if ($Check) { Err "$cmd MISSING (install: $hint)"; Fail-Required; return }
  Log "installing $cmd via winget ($wingetId)"
  if ((Winget-Install $wingetId) -and (Have $cmd)) { Ok "$cmd installed" }
  else { Err "could not install $cmd automatically. Install it manually: $hint"; Fail-Required }
}

# --- REQUIRED: git, gh ------------------------------------------------------
Ensure-System git "Git.Git"    "winget install -e --id Git.Git"
Ensure-System gh  "GitHub.cli" "https://github.com/cli/cli#installation — then: gh auth login"
if ((Have gh) -and -not (& { gh auth status *> $null; $LASTEXITCODE -eq 0 })) {
  Warn "gh is installed but NOT authenticated — run 'gh auth login' before a real flow (tracker scrape + PRs need it)."
}

# --- REQUIRED: python (Windows python.org/winget builds bundle venv+pip) -----
if (Have $Python) { Ok "$Python present ($(Ver $Python))" }
else {
  if ($Check) { Err "$Python MISSING (install: winget install -e --id Python.Python.3.12)"; Fail-Required }
  else {
    Log "installing Python via winget"
    if ((Winget-Install "Python.Python.3.12") -and (Have $Python)) { Ok "python installed ($(Ver $Python))" }
    else { Err "could not install python. Install >=3.11 from https://python.org or winget"; Fail-Required }
  }
}

# --- REQUIRED: Rust toolchain (gating C4-ci is `cargo xtask ci`) -------------
function Ensure-Rust {
  if ((Have cargo) -and (Have rustc)) { Ok "rust present ($(Ver rustc))"; return }
  if ($Check) { Err "cargo/rustc MISSING (install: rustup — https://rustup.rs)"; Fail-Required; return }
  Log "installing Rust via rustup (user-space: %USERPROFILE%\.rustup, .cargo)"
  $arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
  $init = Join-Path $env:TEMP "rustup-init.exe"
  try {
    Invoke-WebRequest "https://win.rustup.rs/$arch" -OutFile $init -UseBasicParsing
    & $init -y --default-toolchain stable | Out-Host
    $env:Path = "$HOME\.cargo\bin;$env:Path"      # current session; installer persists it for new shells
    if (Have cargo) { Ok "rust installed ($(Ver rustc)) — new shells pick it up via %USERPROFILE%\.cargo\bin" }
    else { Err "rustup ran but cargo is not on PATH; add %USERPROFILE%\.cargo\bin"; Fail-Required }
    Warn "cargo builds that link native code need the MSVC C++ Build Tools (winget install -e --id Microsoft.VisualStudio.2022.BuildTools) if not already present."
  } catch { Err "rustup install failed ($($_.Exception.Message)). Install manually from https://rustup.rs"; Fail-Required }
  finally { Remove-Item $init -ErrorAction SilentlyContinue }
}
Ensure-Rust

function Ensure-UserBinOnPath {
  New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
  if ($env:Path -notlike "*$BinDir*") { $env:Path = "$BinDir;$env:Path" }
}

# --- REQUIRED: Claude Code CLI (all six leaves are family="claude") ----------
function Ensure-Claude {
  if (Have claude) { Ok "claude present ($(Ver claude))"; return }
  if ($Check) { Err "claude MISSING (install: irm https://claude.ai/install.ps1 | iex)"; Fail-Required; return }
  Log "installing Claude Code CLI (official installer)"
  try {
    Invoke-RestMethod https://claude.ai/install.ps1 | Invoke-Expression
    Ensure-UserBinOnPath
    if (Have claude) { Ok "claude installed ($(Ver claude))" }
    else { Err "claude installed but not on PATH; restart the shell or add its bin dir to PATH"; Fail-Required }
  } catch { Err "claude install failed ($($_.Exception.Message)). See https://docs.claude.com/claude-code"; Fail-Required }
}
Ensure-Claude

# --- OPTIONAL: OpenAI Codex CLI (advisory leaf, non-gating -> best-effort) ----
function Ensure-Codex {
  if (Have codex) { Ok "codex present ($(Ver codex))"; return }
  if ($Check) { Warn "codex MISSING (optional; advisory review degrades to a §6 note)"; return }
  Log "installing Codex CLI from the latest GitHub release (optional)"
  Ensure-UserBinOnPath
  $arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
  $tgt  = "$arch-pc-windows-msvc"
  try {
    $rel = Invoke-RestMethod "https://api.github.com/repos/openai/codex/releases/latest" -Headers @{ "User-Agent" = "pdca-bootstrap" }
    $asset = $rel.assets | Where-Object { $_.name -match [regex]::Escape($tgt) -and $_.name -match '\.zip$' } | Select-Object -First 1
    if (-not $asset) { Warn "no codex $tgt release asset found — install manually (npm i -g @openai/codex) if you want the cross-vendor advisory. Continuing."; return }
    $zip = Join-Path $env:TEMP "codex.zip"; $dir = Join-Path $env:TEMP "codex-extract"
    Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing
    Remove-Item $dir -Recurse -ErrorAction SilentlyContinue; Expand-Archive $zip -DestinationPath $dir -Force
    $exe = Get-ChildItem $dir -Recurse -Filter "codex*.exe" | Select-Object -First 1
    if ($exe) { Copy-Item $exe.FullName (Join-Path $BinDir "codex.exe") -Force; Ok "codex installed ($(Ver codex))" }
    else { Warn "codex archive had no codex.exe; skipping (optional)." }
    Remove-Item $zip, $dir -Recurse -ErrorAction SilentlyContinue
  } catch { Warn "codex download/extract failed ($($_.Exception.Message)); skipping (optional)." }
}
Ensure-Codex

# --- console script: venv + editable install --------------------------------
function Install-ConsoleScript {
  $pip = ".venv\Scripts\pip.exe"
  if ($Check) {
    if (Test-Path ".venv\Scripts\wyrd-pdca.exe") { Ok "console script present (.venv\Scripts\wyrd-pdca.exe)" }
    else { Err "console script MISSING (run without -Check)"; Fail-Required }
    return
  }
  if (-not (Test-Path $pip)) {
    Log "creating .venv + installing the console script (pip install -e .)"
    & $Python -m venv .venv
    if (-not (Test-Path $pip)) {
      Warn "stdlib venv has no pip (ensurepip missing) — bootstrapping pip via get-pip.py"
      Remove-Item .venv -Recurse -ErrorAction SilentlyContinue
      & $Python -m venv --without-pip .venv
      $getpip = Join-Path $env:TEMP "get-pip.py"
      Invoke-WebRequest https://bootstrap.pypa.io/get-pip.py -OutFile $getpip -UseBasicParsing
      & ".venv\Scripts\python.exe" $getpip
      Remove-Item $getpip -ErrorAction SilentlyContinue
    }
  }
  & ".venv\Scripts\pip.exe" install -q -e .
  if (Test-Path ".venv\Scripts\wyrd-pdca.exe") { Ok "console script installed (.venv\Scripts\wyrd-pdca.exe)" }
}
Install-ConsoleScript

# --- summary ----------------------------------------------------------------
Write-Host ""
if ($script:MissingRequired -ne 0) {
  Err "one or more REQUIRED tools are missing — see the lines above. Resolve them, then re-run."
  exit 1
}
if ($Check) { Ok "all REQUIRED tools present." }
else {
  Ok "bootstrap complete — run the cycle via .venv\Scripts\wyrd-pdca (or 'wyrd-pdca' once .venv\Scripts is on PATH)."
  Log "next: install.ps1 also writes the Claude workspace-read overlay so the interactive leaves don't prompt."
}
exit 0
