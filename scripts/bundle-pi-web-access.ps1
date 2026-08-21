param()

$ErrorActionPreference = "Stop"

$PackageName = "pi-web-access"
$PackageVersion = "0.24.0"
$TypeBoxVersion = "1.1.38"
$ExpectedIntegrity = "sha512-BVosva1tGDhHveaGpFnc++YS5+pzmWVzJ/5B+1xBavkRjAgyDvMpA1EfVL+GYIviAxXKck9JyRGVzo4ASV7snA=="
$Registry = "https://registry.npmjs.org"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$SidecarRoot = (Resolve-Path (Join-Path $RepoRoot "src-tauri\binaries\pi-sidecar")).Path
$Target = Join-Path $SidecarRoot "builtin-extensions\pi-web-access"
$ResolvedTargetParent = [System.IO.Path]::GetFullPath((Split-Path $Target -Parent))

if (-not $ResolvedTargetParent.StartsWith($SidecarRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to write outside the Pi sidecar directory."
}

$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("piwork-web-access-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TempRoot | Out-Null

try {
  Push-Location $TempRoot
  try {
    $Pack = npm pack "$PackageName@$PackageVersion" --json --ignore-scripts --registry=$Registry | ConvertFrom-Json
    if ($Pack.Count -ne 1 -or $Pack[0].integrity -ne $ExpectedIntegrity) {
      throw "The downloaded $PackageName package did not match the pinned integrity."
    }
    $Tarball = Join-Path $TempRoot $Pack[0].filename
  } finally {
    Pop-Location
  }

  if (Test-Path -LiteralPath $Target) {
    $ResolvedTarget = (Resolve-Path -LiteralPath $Target).Path
    if (-not $ResolvedTarget.StartsWith($SidecarRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
      throw "Refusing to replace a target outside the Pi sidecar directory."
    }
    Remove-Item -LiteralPath $ResolvedTarget -Recurse -Force
  }
  New-Item -ItemType Directory -Path $Target | Out-Null

  # pi-web-access declares typebox with a caret range, but its current API is
  # compatible with the 1.1.x version bundled by Pi, not typebox 1.3.x.
  npm install --prefix $Target --ignore-scripts --omit=dev --omit=optional --legacy-peer-deps --no-audit --no-fund --no-save --package-lock=false $Tarball "typebox@$TypeBoxVersion"

  $Entry = Join-Path $Target "node_modules\pi-web-access\index.ts"
  $Manifest = Join-Path $Target "node_modules\pi-web-access\package.json"
  if (-not (Test-Path -LiteralPath $Entry)) {
    throw "Bundled extension entry point is missing."
  }
  $Installed = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
  if ($Installed.version -ne $PackageVersion) {
    throw "Bundled extension version does not match $PackageVersion."
  }

  $InstalledTypeBox = Get-Content -LiteralPath (Join-Path $Target "node_modules\typebox\package.json") -Raw | ConvertFrom-Json
  if ($InstalledTypeBox.version -ne $TypeBoxVersion) {
    throw "Bundled typebox version does not match Pi's extension API version."
  }

  $PiEntrypoint = Join-Path $SidecarRoot "dist\piwork-pi.js"
  $PreviousAgentDir = $env:PI_CODING_AGENT_DIR
  try {
    $env:PI_CODING_AGENT_DIR = Join-Path $TempRoot "agent"
    $SmokeOutput = '{"id":"piwork-extension-smoke","type":"get_state"}' |
      node $PiEntrypoint --mode rpc --offline --no-extensions --extension $Entry --no-session 2>&1 |
      Out-String
  } finally {
    $env:PI_CODING_AGENT_DIR = $PreviousAgentDir
  }
  if ($LASTEXITCODE -ne 0 -or $SmokeOutput -match "Failed to load extension") {
    throw "Bundled extension failed the Pi RPC load check: $SmokeOutput"
  }

  Write-Host "Bundled $PackageName@$PackageVersion at $Target"
} finally {
  if (Test-Path -LiteralPath $TempRoot) {
    Remove-Item -LiteralPath $TempRoot -Recurse -Force
  }
}
