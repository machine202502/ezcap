use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Put the catalogs next to the built executable, so `cargo run` and a plain
/// `cargo build` see the same `locales/` folder the installer ships. Packaging
/// still copies the executable alone, so the portable build stays English-only.
fn copy_locales() {
    println!("cargo:rerun-if-changed=locales");
    let Ok(out_dir) = std::env::var("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    // OUT_DIR is <target>/<profile>/build/<package>-<hash>/out.
    let Some(profile_dir) = out_dir.ancestors().nth(3) else {
        return;
    };
    let source = Path::new("locales");
    if !source.is_dir() {
        return;
    }
    let dest = profile_dir.join("locales");
    let _ = std::fs::create_dir_all(&dest);
    let Ok(entries) = std::fs::read_dir(source) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let _ = std::fs::copy(&path, dest.join(name));
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-changed=assets/ezcap.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    copy_locales();
    let mut resource = winres::WindowsResource::new();
    resource.set_icon("assets/ezcap.ico");
    resource.set_manifest_file("assets/app.manifest");
    resource.set("ProductName", "ezcap");
    resource.set("FileDescription", "Screen, region and audio capture");
    resource.set(
        "Comments",
        "https://github.com/machine202502/ezcap — includes FFmpeg and x264 under the GNU GPL v2 or later (https://ffmpeg.org/legal.html) and Phosphor Icons under the MIT License (https://phosphoricons.com). Other notices are in THIRD-PARTY.txt.",
    );
    resource.set("OriginalFilename", "ezcap.exe");
    resource.compile().unwrap();
    let Some(dir) = std::env::var_os("FFMPEG_DIR").map(PathBuf::from) else {
        return;
    };
    let lib_dir = dir.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    let ffmpeg = [
        "avcodec",
        "avdevice",
        "avfilter",
        "avformat",
        "avutil",
        "swresample",
        "swscale",
    ];
    let mut libs = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(lib_dir.join("pkgconfig")) {
        for entry in entries.flatten() {
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            for line in text.lines() {
                let Some(rest) = line
                    .strip_prefix("Libs.private:")
                    .or_else(|| line.strip_prefix("Libs:"))
                else {
                    continue;
                };
                for flag in rest.split_whitespace() {
                    if let Some(name) = flag.strip_prefix("-l") {
                        if !ffmpeg.contains(&name) {
                            libs.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }
    for name in libs {
        if lib_dir.join(format!("{name}.lib")).is_file() {
            println!("cargo:rustc-link-lib=static={name}");
        } else {
            println!("cargo:rustc-link-lib={name}");
        }
    }
}
