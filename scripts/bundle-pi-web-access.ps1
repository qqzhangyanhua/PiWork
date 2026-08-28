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
$Entry = Join-Path $Target "node_modules\pi-web-access\index.ts"
$Manifest = Join-Path $Target "node_modules\pi-web-access\package.json"
$TypeBoxManifest = Join-Path $Target "node_modules\typebox\package.json"
$Modules = Join-Path $Target "node_modules"
$ResolvedTargetParent = [System.IO.Path]::GetFullPath((Split-Path $Target -Parent))

if (-not $ResolvedTargetParent.StartsWith($SidecarRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to write outside the Pi sidecar directory."
}

function Test-AlreadyBundled {
  if (-not ((Test-Path -LiteralPath $Entry) -and (Test-Path -LiteralPath $Manifest) -and (Test-Path -LiteralPath $TypeBoxManifest))) {
    return $false
  }
  $Installed = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
  $InstalledTypeBox = Get-Content -LiteralPath $TypeBoxManifest -Raw | ConvertFrom-Json
  if ($Installed.version -ne $PackageVersion -or $InstalledTypeBox.version -ne $TypeBoxVersion) {
    return $false
  }
  if ((Test-Path -LiteralPath (Join-Path $Modules "pi-web-access\pi-web-fetch-demo.mp4")) -or
      (Test-Path -LiteralPath (Join-Path $Modules "pi-web-access\banner.png")) -or
      (Test-Path -LiteralPath (Join-Path $Modules "@mixmark-io\domino\test"))) {
    return $false
  }
  return $true
}

if (Test-AlreadyBundled) {
  Write-Host "Bundled $PackageName@$PackageVersion already present at $Target"
  exit 0
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

  if (-not (Test-Path -LiteralPath $Entry)) {
    throw "Bundled extension entry point is missing."
  }
  $Installed = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
  if ($Installed.version -ne $PackageVersion) {
    throw "Bundled extension version does not match $PackageVersion."
  }

  $InstalledTypeBox = Get-Content -LiteralPath $TypeBoxManifest -Raw | ConvertFrom-Json
  if ($InstalledTypeBox.version -ne $TypeBoxVersion) {
    throw "Bundled typebox version does not match Pi's extension API version."
  }

  Get-ChildItem -LiteralPath $Modules -Recurse -Force -Directory -ErrorAction SilentlyContinue |
    Where-Object { @('test', 'tests', 'docs', '.yarn') -contains $_.Name } |
    Sort-Object { $_.FullName.Length } -Descending |
    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force }
  Get-ChildItem -LiteralPath $Modules -Recurse -Force -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -eq 'banner.png' -or $_.Extension -eq '.mp4' } |
    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force }

  $DemoVideo = Join-Path $Modules "pi-web-access\pi-web-fetch-demo.mp4"
  $Banner = Join-Path $Modules "pi-web-access\banner.png"
  if ((Test-Path -LiteralPath $DemoVideo) -or (Test-Path -LiteralPath $Banner)) {
    throw "Packaging weight was not stripped from the bundled extension."
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
