# Build the release exe, then write the portable zip and the installer into dist/.
# Requires FFMPEG_DIR (static FFmpeg prefix) and Inno Setup 6 (ISCC.exe).
param(
    [string]$Version
)

function New-ThirdPartyNotice {
    param(
        [string]$Version,
        [string]$FfmpegDir
    )
    $statusPath = Join-Path (Split-Path $FfmpegDir -Parent) "vcpkg\status"
    $status = ""
    if (Test-Path $statusPath) {
        $status = Get-Content -Raw -Path $statusPath
    }
    $ports = @(foreach ($name in @("ffmpeg", "x264", "libwebp", "mp3lame", "zlib")) {
        $found = $null
        foreach ($block in ($status -split "(?m)(?=^Package:)")) {
            if ($block -match "(?m)^Package:\s+$([regex]::Escape($name))(?::\S+)?\s*$" -and
                $block -match "(?m)^Version:\s+(.+)\s*$") {
                $found = $Matches[1].Trim()
                break
            }
        }
        if ($found) { "- ${name} ${found}" } else { "- ${name} (version not recorded)" }
    })
    $commit = "not recorded"
    foreach ($root in @($env:VCPKG_ROOT, "C:\vcpkg")) {
        if ($root -and (Test-Path (Join-Path $root ".git"))) {
            $commit = (& git -C $root rev-parse HEAD).Trim()
            break
        }
    }
    @"
ezcap $Version

This executable includes:
$($ports -join "`n")

FFmpeg: https://ffmpeg.org/legal.html
x264: https://www.videolan.org/developers/x264.html
Both are used under the GNU General Public License, version 2 or later. See COPYING.

ezcap source for this version:
https://github.com/machine202502/ezcap/tree/v$Version

vcpkg commit: $commit
"@
}

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

$thirdParty = Join-Path $dist "THIRD-PARTY.txt"
Set-Content -Path $thirdParty -Encoding utf8 -Value (New-ThirdPartyNotice -Version $Version -FfmpegDir $env:FFMPEG_DIR)

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
Copy-Item (Join-Path $Root "COPYING") (Join-Path $stage "COPYING")
Copy-Item (Join-Path $Root "NOTICE") (Join-Path $stage "NOTICE")
Copy-Item $thirdParty (Join-Path $stage "THIRD-PARTY.txt")
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $portable
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
& $iscc "/DAppVersion=$Version" "/DSourceExe=$exe" "/DThirdParty=$thirdParty" $iss
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed"
}

$setup = Join-Path $dist "ezcap-$Version-windows-x64-setup.exe"
if (-not (Test-Path $setup)) {
    throw "Installer was not produced: $setup"
}
Write-Host "Installer: $setup"
