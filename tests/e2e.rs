#![cfg(windows)]

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

struct TestApp {
    child: Child,
    created: Vec<PathBuf>,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for path in &self.created {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn click(hwnd: HWND, id: i32) {
    unsafe {
        let button = GetDlgItem(hwnd, id).unwrap();
        SendMessageW(button, BM_CLICK, WPARAM(0), LPARAM(0));
    }
}

fn select(hwnd: HWND, id: i32, index: usize) {
    unsafe {
        SendMessageW(
            GetDlgItem(hwnd, id).unwrap(),
            CB_SETCURSEL,
            WPARAM(index),
            LPARAM(0),
        );
        SendMessageW(
            hwnd,
            WM_COMMAND,
            WPARAM((CBN_SELCHANGE as usize) << 16 | id as usize),
            LPARAM(0),
        );
    }
}

fn control_text(hwnd: HWND, id: i32) -> String {
    let mut text = vec![0u16; 32768];
    let len = unsafe { GetDlgItemTextW(hwnd, id, &mut text) } as usize;
    String::from_utf16_lossy(&text[..len])
}

fn status(hwnd: HWND) -> String {
    control_text(hwnd, 136)
}

fn find_app_window(process_id: u32) -> HWND {
    unsafe {
        let mut hwnd = FindWindowW(
            windows::core::w!("ezcap.window"),
            windows::core::w!("ezcap"),
        )
        .unwrap_or_default();
        while !hwnd.is_invalid() {
            let mut window_process = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut window_process));
            if window_process == process_id {
                return hwnd;
            }
            hwnd = GetWindow(hwnd, GW_HWNDNEXT).unwrap_or_default();
        }
        HWND::default()
    }
}

fn wait_saved(hwnd: HWND) -> PathBuf {
    for _ in 0..100 {
        let text = status(hwnd);
        if let Some(path) = text.strip_prefix("Saved: ") {
            return PathBuf::from(path);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("Output was not saved: {}", status(hwnd));
}

fn record(hwnd: HWND, format: i32, audio_only: bool) -> PathBuf {
    click(hwnd, 110 + format);
    select(hwnd, 121, 4);
    let previous = status(hwnd);
    click(hwnd, if audio_only { 134 } else { 132 });
    std::thread::sleep(Duration::from_millis(1100));
    click(hwnd, 135);
    for _ in 0..100 {
        let text = status(hwnd);
        if text.starts_with("Saved: ") && text != previous {
            return PathBuf::from(text.trim_start_matches("Saved: "));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("Output was not saved: {}", status(hwnd));
}

fn check(path: PathBuf, prefix: &str, extension: &str) {
    let name = path.file_name().unwrap().to_string_lossy();
    println!("checking {name} against {prefix} / {extension}");
    assert!(name.starts_with(prefix), "{name}");
    assert!(name.ends_with(extension), "{name}");
    assert!(path.is_file(), "{} is missing", path.display());
}

fn check_stream(path: &PathBuf, kind: ffmpeg_next::media::Type) {
    let input = ffmpeg_next::format::input(path).unwrap();
    assert!(
        input.streams().best(kind).is_some(),
        "missing {kind:?} stream in {}",
        path.display()
    );
}

fn check_decodable_audio(path: &PathBuf, minimum_samples: usize) {
    let mut input = ffmpeg_next::format::input(path).unwrap();
    let stream = input
        .streams()
        .best(ffmpeg_next::media::Type::Audio)
        .unwrap();
    let index = stream.index();
    let mut decoder = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .unwrap()
        .decoder()
        .audio()
        .unwrap();
    let mut decoded = ffmpeg_next::frame::Audio::empty();
    let mut samples = 0usize;
    for (source, packet) in input.packets() {
        if source.index() != index {
            continue;
        }
        decoder.send_packet(&packet).unwrap();
        while decoder.receive_frame(&mut decoded).is_ok() {
            samples += decoded.samples();
        }
    }
    decoder.send_eof().unwrap();
    while decoder.receive_frame(&mut decoded).is_ok() {
        samples += decoded.samples();
    }
    assert!(
        samples >= minimum_samples,
        "only {samples} audio samples decoded from {}",
        path.display()
    );
}

#[test]
#[ignore]
fn records_every_output_format_through_the_real_ui() {
    ffmpeg_next::init().unwrap();
    let mut app = TestApp {
        child: Command::new(env!("CARGO_BIN_EXE_ezcap"))
            .arg("--test-mode")
            .spawn()
            .unwrap(),
        created: Vec::new(),
    };
    let process_id = app.child.id();
    let mut hwnd = HWND::default();
    for _ in 0..50 {
        hwnd = find_app_window(process_id);
        if !hwnd.is_invalid() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!hwnd.is_invalid(), "Main window was not created");
    for (format, prefix, extension) in [
        (0, "Video_", ".mkv"),
        (1, "Animation_", ".gif"),
        (2, "Animation_", ".webp"),
    ] {
        let path = record(hwnd, format, false);
        check(path.clone(), prefix, extension);
        check_stream(&path, ffmpeg_next::media::Type::Video);
        if format == 0 {
            check_stream(&path, ffmpeg_next::media::Type::Audio);
            check_decodable_audio(&path, 40000);
        }
        app.created.push(path);
    }
    let path = record(hwnd, 0, true);
    check(path.clone(), "Audio_", ".mp3");
    check_stream(&path, ffmpeg_next::media::Type::Audio);
    check_decodable_audio(&path, 40000);
    app.created.push(path);
    click(hwnd, 130);
    let path = wait_saved(hwnd);
    check(path.clone(), "Image_", ".png");
    app.created.push(path);
    unsafe {
        SendMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    let _ = app.child.wait();
}
