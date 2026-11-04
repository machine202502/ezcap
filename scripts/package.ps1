# Build the release exe, then write the portable zip and the installer into dist/.
# Requires FFMPEG_DIR (static FFmpeg prefix) and Inno Setup 6 (ISCC.exe).
param(
    [string]$Version
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

if (-not $Version) {
    $cargo = Get-Content -Raw -Path (Join-Path $Root "Cargo.toml")
    if ($cargo -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw "Could not read the package version from Cargo.toml"
    }
    $Version = $Matches[1]
}

if (-not $env:FFMPEG_DIR) {
    throw "Set FFMPEG_DIR to the static FFmpeg prefix before packaging."
}

Write-Host "Building ezcap $Version"
cargo build --release --bin ezcap
if ($LASTEXITCODE -ne 0) {
    throw "cargo build --release failed"
}

$exe = Join-Path $Root "target\release\ezcap.exe"
if (-not (Test-Path $exe)) {
    throw "Release executable was not produced: $exe"
}

$dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$portable = Join-Path $dist "ezcap-$Version-windows-x64.zip"
if (Test-Path $portable) {
    Remove-Item $portable
}
$stage = Join-Path $dist "portable"
if (Test-Path $stage) {
    Remove-Item $stage -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe (Join-Path $stage "ezcap.exe")
Compress-Archive -Path (Join-Path $stage "ezcap.exe") -DestinationPath $portable
Remove-Item $stage -Recurse -Force
Write-Host "Portable: $portable"

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) {
    throw "Inno Setup 6 was not found. Install it so ISCC.exe can build the installer."
}

$iss = Join-Path $Root "installer\ezcap.iss"
& $iscc "/DAppVersion=$Version" "/DSourceExe=$exe" $iss
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed"
}

$setup = Join-Path $dist "ezcap-$Version-windows-x64-setup.exe"
if (-not (Test-Path $setup)) {
    throw "Installer was not produced: $setup"
}
Write-Host "Installer: $setup"
