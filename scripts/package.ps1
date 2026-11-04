# Build the release exe, then write the portable zip and the installer into dist/.
# Requires FFMPEG_DIR (static FFmpeg prefix) and Inno Setup 6 (ISCC.exe).
param(
    [string]$Version
)

function Test-ShippedOnWindowsX64 {
    param([string]$Target)
    if (-not $Target) { return $true }
    $t = $Target.ToLowerInvariant()
    if ($t -eq 'cfg(windows)' -or $t -eq 'cfg(target_os = "windows")') { return $true }
    if ($t -eq 'cfg(not(windows))' -or $t -eq 'cfg(unix)' -or $t -eq 'cfg(any())') { return $false }
    foreach ($skip in @(
            'apple', 'android', 'unix', 'linux', 'ios', 'wasm', 'hermit', 'redox',
            'dragonfly', 'freebsd', 'gnullvm', 'target_env = "gnu"',
            'target_arch = "x86"', 'target_arch = "aarch64"'
        )) {
        if ($t.Contains($skip)) { return $false }
    }
    if ($t.Contains('x86_64') -and $t.Contains('msvc')) { return $true }
    return $false
}

function Test-ProcMacroOnly {
    param($Package)
    $kinds = @()
    foreach ($target in @($Package.targets)) {
        foreach ($kind in @($target.kind)) { $kinds += $kind }
    }
    $other = @($kinds | Where-Object { $_ -ne "proc-macro" })
    return ($kinds.Count -gt 0 -and $other.Count -eq 0)
}

function Get-VcpkgVersions {
    param([string]$FfmpegDir)
    $versions = @{}
    $statusPath = Join-Path (Split-Path $FfmpegDir -Parent) "vcpkg\status"
    if (-not (Test-Path $statusPath)) { return $versions }
    $name = $null
    foreach ($line in Get-Content -Path $statusPath) {
        if ($line -match '^Package:\s+(\S+)') {
            $name = $Matches[1]
            continue
        }
        if ($name -and $line -match '^Version:\s+(\S+)') {
            $versions[$name] = $Matches[1]
            $name = $null
        }
    }
    return $versions
}

function New-ThirdPartyNotice {
    param(
        [string]$Version,
        [string]$FfmpegDir
    )
    $root = Resolve-Path (Join-Path $PSScriptRoot "..")
    $commit = "not recorded"
    foreach ($vcpkg in @($env:VCPKG_ROOT, "C:\vcpkg")) {
        if ($vcpkg -and (Test-Path (Join-Path $vcpkg ".git"))) {
            $commit = (& git -C $vcpkg rev-parse HEAD).Trim()
            break
        }
    }
    $text = New-Object System.Text.StringBuilder
    [void]$text.AppendLine("ezcap $Version")
    [void]$text.AppendLine("")
    [void]$text.AppendLine("This executable includes third-party components. COPYING is the GNU GPL v2 or later for FFmpeg and x264. NOTICE summarizes the obligations. The texts below are the copyright and license notices those components require.")
    [void]$text.AppendLine("")
    [void]$text.AppendLine("ezcap source for this version:")
    [void]$text.AppendLine("https://github.com/machine202502/ezcap/tree/v$Version")
    [void]$text.AppendLine("")
    [void]$text.AppendLine("vcpkg commit: $commit")
    [void]$text.AppendLine("")
    [void]$text.AppendLine("==== Native libraries ====")
    [void]$text.AppendLine("")
    $versions = Get-VcpkgVersions -FfmpegDir $FfmpegDir
    $share = Join-Path $FfmpegDir "share"
    $foundNative = $false
    if (Test-Path $share) {
        foreach ($dir in (Get-ChildItem -Directory $share | Sort-Object Name)) {
            $copy = Join-Path $dir.FullName "copyright"
            if (-not (Test-Path $copy)) { continue }
            $foundNative = $true
            $version = $versions[$dir.Name]
            if (-not $version) { $version = "version not recorded" }
            [void]$text.AppendLine("---- $($dir.Name) $version ----")
            [void]$text.AppendLine("")
            [void]$text.AppendLine([System.IO.File]::ReadAllText($copy).TrimEnd())
            [void]$text.AppendLine("")
        }
    }
    if (-not $foundNative) {
        [void]$text.AppendLine("No copyright files were found under $share.")
        [void]$text.AppendLine("")
    }
    [void]$text.AppendLine("==== Phosphor Icons ====")
    [void]$text.AppendLine("")
    [void]$text.AppendLine("assets/Phosphor.ttf - https://phosphoricons.com")
    [void]$text.AppendLine("")
    $phosphor = Join-Path $root "assets\Phosphor.LICENSE"
    [void]$text.AppendLine([System.IO.File]::ReadAllText($phosphor).TrimEnd())
    [void]$text.AppendLine("")
    [void]$text.AppendLine("==== Rust crates ====")
    [void]$text.AppendLine("")
    [void]$text.AppendLine("Crates compiled into this Windows x64 executable. A crate that declares the Do What The Fuck You Want To Public License but ships no license file gets that text below.")
    [void]$text.AppendLine("")
    Push-Location $root
    try {
        $metaJson = (& cargo metadata --format-version 1 | Out-String)
    }
    finally {
        Pop-Location
    }
    if ($LASTEXITCODE -ne 0 -or -not $metaJson) {
        throw "cargo metadata failed"
    }
    if ($PSVersionTable.PSVersion.Major -ge 6) {
        $meta = $metaJson | ConvertFrom-Json -Depth 100
    }
    else {
        $meta = $metaJson | ConvertFrom-Json
    }
    $nodes = @{}
    foreach ($node in $meta.resolve.nodes) { $nodes[$node.id] = $node }
    $pkgs = @{}
    foreach ($pkg in $meta.packages) { $pkgs[$pkg.id] = $pkg }
    $queue = New-Object System.Collections.Generic.Queue[string]
    $queue.Enqueue([string]$meta.resolve.root)
    $seen = @{}
    $crates = @()
    while ($queue.Count -gt 0) {
        $id = $queue.Dequeue()
        if ($seen.ContainsKey($id)) { continue }
        $seen[$id] = $true
        $node = $nodes[$id]
        if ($id -ne $meta.resolve.root -and (Test-ProcMacroOnly $pkgs[$id])) { continue }
        if ($id -ne $meta.resolve.root) { $crates += $pkgs[$id] }
        foreach ($dep in @($node.deps)) {
            $take = $false
            foreach ($kind in @($dep.dep_kinds)) {
                if ($kind.kind -and $kind.kind -ne "normal") { continue }
                if (Test-ShippedOnWindowsX64 $kind.target) { $take = $true }
            }
            if ($take) { $queue.Enqueue([string]$dep.pkg) }
        }
    }
    $wtfpl = @"
            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
                    Version 2, December 2004

 Everyone is permitted to copy and distribute verbatim or modified
 copies of this license document, and changing it is allowed as long
 as the name is changed.

            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION

  0. You just DO WHAT THE FUCK YOU WANT TO.
"@
    foreach ($pkg in ($crates | Sort-Object name, version)) {
        $authors = @($pkg.authors) -join ", "
        if (-not $authors) { $authors = "not recorded" }
        $repository = $pkg.repository
        if (-not $repository) { $repository = $pkg.homepage }
        if (-not $repository) { $repository = "not recorded" }
        [void]$text.AppendLine("---- $($pkg.name) $($pkg.version) ----")
        [void]$text.AppendLine("License: $($pkg.license)")
        [void]$text.AppendLine("Authors: $authors")
        [void]$text.AppendLine("Repository: $repository")
        [void]$text.AppendLine("")
        $dir = Split-Path $pkg.manifest_path
        $files = @(Get-ChildItem -File $dir | Where-Object {
                $_.Name -match '^(?i)(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)'
            } | Sort-Object Name)
        if ($files.Count -eq 0 -and $pkg.license -eq "WTFPL") {
            [void]$text.AppendLine($wtfpl.TrimEnd())
            [void]$text.AppendLine("")
            continue
        }
        if ($files.Count -eq 0 -and $pkg.license -match "MIT") {
            [void]$text.AppendLine("This crate shipped no license file. It is included under the MIT License option of its declared license ($($pkg.license)).")
            [void]$text.AppendLine("")
            [void]$text.AppendLine("MIT License")
            [void]$text.AppendLine("")
            [void]$text.AppendLine("Copyright (c) $authors")
            [void]$text.AppendLine("")
            [void]$text.AppendLine("Permission is hereby granted, free of charge, to any person obtaining a copy")
            [void]$text.AppendLine("of this software and associated documentation files (the `"Software`"), to deal")
            [void]$text.AppendLine("in the Software without restriction, including without limitation the rights")
            [void]$text.AppendLine("to use, copy, modify, merge, publish, distribute, sublicense, and/or sell")
            [void]$text.AppendLine("copies of the Software, and to permit persons to whom the Software is")
            [void]$text.AppendLine("furnished to do so, subject to the following conditions:")
            [void]$text.AppendLine("")
            [void]$text.AppendLine("The above copyright notice and this permission notice shall be included in all")
            [void]$text.AppendLine("copies or substantial portions of the Software.")
            [void]$text.AppendLine("")
            [void]$text.AppendLine("THE SOFTWARE IS PROVIDED `"AS IS`", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR")
            [void]$text.AppendLine("IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,")
            [void]$text.AppendLine("FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE")
            [void]$text.AppendLine("AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER")
            [void]$text.AppendLine("LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,")
            [void]$text.AppendLine("OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE")
            [void]$text.AppendLine("SOFTWARE.")
            [void]$text.AppendLine("")
            continue
        }
        if ($files.Count -eq 0) {
            [void]$text.AppendLine("This crate did not ship a license file. The declared license is $($pkg.license).")
            [void]$text.AppendLine("")
            continue
        }
        foreach ($file in $files) {
            [void]$text.AppendLine("[$($file.Name)]")
            [void]$text.AppendLine([System.IO.File]::ReadAllText($file.FullName).TrimEnd())
            [void]$text.AppendLine("")
        }
    }
    return $text.ToString().TrimEnd()
}

if ($MyInvocation.InvocationName -ne '.') {

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
}
