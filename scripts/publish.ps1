# Package the portable zip and the installer, then attach both to a GitHub release tag.
# Requires the GitHub CLI (gh) authenticated for this repository.
param(
    [string]$Tag,
    [switch]$Draft
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$cargo = Get-Content -Raw -Path (Join-Path $Root "Cargo.toml")
if ($cargo -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw "Could not read the package version from Cargo.toml"
}
$Version = $Matches[1]
if (-not $Tag) {
    $Tag = "v$Version"
}

& (Join-Path $PSScriptRoot "package.ps1") -Version $Version
if ($LASTEXITCODE -ne 0) {
    throw "package.ps1 failed"
}

$portable = Join-Path $Root "dist\ezcap-$Version-windows-x64.zip"
$setup = Join-Path $Root "dist\ezcap-$Version-windows-x64-setup.exe"
foreach ($path in @($portable, $setup)) {
    if (-not (Test-Path $path)) {
        throw "Missing release file: $path"
    }
}

$notes = @"
Windows x64.

- ``ezcap-$Version-windows-x64.zip`` is portable. Unzip it and run ``ezcap.exe``.
- ``ezcap-$Version-windows-x64-setup.exe`` installs ezcap for all users.

The executable includes FFmpeg and x264 under the GNU GPL v2 or later, Phosphor Icons under the MIT License, and the other libraries and Rust crates named in the notices. ``COPYING``, ``NOTICE``, and ``THIRD-PARTY.txt`` in the zip and the install folder carry those terms and where to get the source.
ezcap source for this tag: https://github.com/machine202502/ezcap/tree/$Tag
"@

$ghArgs = @(
    "release", "create", $Tag,
    "--title", "ezcap $Version",
    "--notes", $notes,
    $portable,
    $setup
)
if ($Draft) {
    $ghArgs += "--draft"
}

gh @ghArgs
if ($LASTEXITCODE -ne 0) {
    throw "gh release create failed. If $Tag already exists, delete that release before publishing again."
}
