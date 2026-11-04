# ezcap

Windows program for screen, region, and audio capture. It records MKV, GIF, and WebP, takes screenshots, and can keep running from the tray.

The source in this repository is under the MIT license. A release build statically links FFmpeg and x264 under the GNU GPL v2 or later, so the distributed executable is covered by the GPL. `COPYING` is that license. The overlay icons are Phosphor Icons under the MIT license (`assets/Phosphor.LICENSE`). `NOTICE` names these obligations. The generated `THIRD-PARTY.txt` adds the copyright text for the linked codecs and for the Rust crates, and it travels with the zip and the installer. Source and downloads: https://github.com/machine202502/ezcap

## Build

Windows x64, a stable Rust toolchain, and a static FFmpeg install from `vcpkg.json` (codecs: H.264, WebP, MP3). Point `FFMPEG_DIR` at that prefix. It must contain `lib` and `lib/pkgconfig`.

```powershell
$env:FFMPEG_DIR = "C:\path\to\vcpkg_installed\x64-windows-static"
cargo build --release --bin ezcap
```

The executable is `target\release\ezcap.exe`. Icons, the no-signal card, and the completion sound are compiled into it.

`scripts\package.ps1` builds that executable, then writes a portable zip and an Inno Setup installer into `dist\`. The installer needs Inno Setup 6. Pushing a new `version` in `Cargo.toml` to `master` runs the same packaging on GitHub and publishes the files when that version has no release yet.

## Test

Use the same `FFMPEG_DIR`.

```powershell
cargo test --bin ezcap
```

That runs the unit tests. Snapshot tests are ignored. They compare the window and the area overlay at scale 1 and need a desktop:

```powershell
cargo test --bin ezcap -- --ignored
```

Set `EZCAP_UPDATE_SNAPSHOTS` to any value to replace the images in `tests\snapshots` instead of comparing them.
