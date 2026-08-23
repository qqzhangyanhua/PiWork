param(
  [ValidateSet("debug", "release")]
  [string]$Profile = "debug"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Manifest = Join-Path $RepoRoot "src-tauri\document-runtime\Cargo.toml"
$TargetRoot = Join-Path $RepoRoot "src-tauri\target\document-runtime"
$CargoArguments = @("build", "--manifest-path", $Manifest, "--target-dir", $TargetRoot)
if ($Profile -eq "release") {
  $CargoArguments += "--release"
}

& cargo @CargoArguments
if ($LASTEXITCODE -ne 0) {
  throw "Document runtime build failed with exit code $LASTEXITCODE."
}

$ExecutableName = "piwork-document-runtime.exe"
$BuiltExecutable = Join-Path $TargetRoot "$Profile\$ExecutableName"
$RuntimeResource = Join-Path $RepoRoot "src-tauri\binaries\document-runtime\$ExecutableName"
$RuntimeBesideApp = Join-Path $RepoRoot "src-tauri\target\$Profile\document-runtime\$ExecutableName"

New-Item -ItemType Directory -Force -Path (Split-Path $RuntimeResource) | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path $RuntimeBesideApp) | Out-Null
Copy-Item -LiteralPath $BuiltExecutable -Destination $RuntimeResource -Force
Copy-Item -LiteralPath $BuiltExecutable -Destination $RuntimeBesideApp -Force
