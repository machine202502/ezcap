use std::collections::BTreeSet;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-changed=assets/ezcap.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    let mut resource = winres::WindowsResource::new();
    resource.set_icon("assets/ezcap.ico");
    resource.set_manifest_file("assets/app.manifest");
    resource.set("ProductName", "ezcap");
    resource.set("FileDescription", "Screen, region and audio capture");
    resource.set(
        "Comments",
        "https://github.com/machine202502/ezcap — includes FFmpeg and x264 under the GNU GPL v2 or later (https://ffmpeg.org/legal.html).",
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
