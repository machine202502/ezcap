use super::*;
use media::Frame;
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{TBM_SETPOS, TB_ENDTRACK};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

/// The app and the area overlay live in process-wide pointers. Two tests that drove
/// either of them at the same time would overwrite each other's window, so every
/// test that opens one of them takes this lock.
static UI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize_ui() -> std::sync::MutexGuard<'static, ()> {
    UI_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn snapshots_dir() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots");
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn capture_window(hwnd: HWND) -> Frame {
    unsafe {
        // SAFETY: The HWND belongs to a window this process created. The RECT is a writable local.

        // A centered window runs off the monitor and PrintWindow keeps only the visible part.
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
        UpdateWindow(hwnd);
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).unwrap();
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let window_dc = GetWindowDC(hwnd);
        let memory = CreateCompatibleDC(window_dc);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let bitmap =
            CreateDIBSection(window_dc, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let old = SelectObject(memory, bitmap);
        PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(0)).unwrap();
        let mut data = vec![0u8; width as usize * height as usize * 4];
        std::ptr::copy_nonoverlapping(bits.cast::<u8>(), data.as_mut_ptr(), data.len());
        for pixel in data.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        SelectObject(memory, old);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(hwnd, window_dc);
        Frame {
            width: width as u32,
            height: height as u32,
            data,
        }
    }
}

fn snapshot(name: &str, frame: &Frame) {
    let path = snapshots_dir().join(format!("{name}.png"));
    if std::env::var_os("EZCAP_UPDATE_SNAPSHOTS").is_some() {
        media::save_frame(frame, &path).unwrap();
        return;
    }
    let expected = media::read_image(&path).unwrap();
    let different_size = expected.width != frame.width
        || expected.height != frame.height
        || expected.data.len() != frame.data.len();
    let delta = if different_size {
        255.0
    } else {
        expected
            .data
            .iter()
            .zip(&frame.data)
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum::<u64>() as f64
            / expected.data.len() as f64
    };
    if delta > 2.0 {
        let actual = snapshots_dir().join(format!("{name}.actual.png"));
        media::save_frame(frame, &actual).unwrap();
        panic!(
            "Snapshot mismatch: {name}, mean channel delta {delta}; actual saved to {}",
            actual.display()
        );
    }
}

fn register_test_classes() {
    unsafe {
        // SAFETY: The module is this process, and the icon id is the resource embedded in the executable.

        let instance = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hIcon: LoadIconW(instance, module_icon()).unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut std::ffi::c_void),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        region::register(instance.into());
    }
}

fn synthetic_frame() -> Frame {
    synthetic_frame_sized(640, 480)
}

fn synthetic_frame_sized(width: usize, height: usize) -> Frame {
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&[(x / 4) as u8, (y / 3) as u8, 80, 255]);
        }
    }
    Frame {
        width: width as u32,
        height: height as u32,
        data,
    }
}

#[test]
#[ignore]
fn main_window_snapshots_at_scale_one() {
    let _ui = serialize_ui();
    media::init();
    register_test_classes();
    unsafe {
        // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            0,
            0,
            640,
            680,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .unwrap();
    }
    app().settings = Settings::default();
    app().controls.load_ui(&app().settings);
    assert_eq!(app().settings.scale, 0, "interface scale defaults to Auto");
    assert!(
        app().settings.highlight_region,
        "the recording outline is on by default"
    );
    assert!(app().controls.checked(ID_HIGHLIGHT));
    assert!(
        app().settings.completion_sound,
        "the completion sound is on by default"
    );
    assert!(app().controls.checked(ID_NOTIFY));
    assert_eq!(app().controls.selected(ID_SCALE), 0);
    app().show_window();
    unsafe {
        // SAFETY: The HWND is a child control created with the main window.
        assert!(IsWindowVisible(app().controls.get(ID_REFRESH)).as_bool())
    };
    app().controls.set_idle(false, false);
    unsafe {
        // SAFETY: The HWND is a child control created with the main window.
        assert!(!IsWindowEnabled(app().controls.get(ID_REFRESH)).as_bool())
    };
    app().controls.set_idle(true, false);
    app().settings.scale = 100;
    app().controls.apply_scale(&app().settings);
    app().show_window();
    unsafe {
        // SAFETY: The HWND is a window this program created.
        UpdateWindow(app().hwnd)
    };
    std::thread::sleep(std::time::Duration::from_millis(40));
    snapshot("main-capture-100", &capture_window(app().hwnd));
    app().controls.select(ID_SCALE, 1);
    app().controls.apply_scale(&app().settings);
    app().controls.show_page(1);
    assert_eq!(app().controls.page, 1);
    unsafe {
        // SAFETY: The HWND is a window this program created.

        assert!(IsWindowVisible(app().controls.get(ID_AUTOSTART)).as_bool());
        assert!(IsWindowVisible(app().controls.get(ID_REFRESH)).as_bool());
    }
    std::thread::sleep(std::time::Duration::from_millis(40));
    snapshot("main-settings-100", &capture_window(app().hwnd));
    assert_eq!(
        (
            app().settings.ui.video[0].height,
            app().settings.ui.video[0].fps
        ),
        (1080, 60)
    );
    assert_eq!(
        (
            app().settings.ui.video[1].height,
            app().settings.ui.video[1].fps
        ),
        (480, 30)
    );
    assert_eq!(
        (
            app().settings.ui.video[2].height,
            app().settings.ui.video[2].fps
        ),
        (480, 30)
    );
    assert_eq!(app().settings.tray_video.format, Format::Mkv);
    app().settings.ui.audio.device = "{missing-device}".into();
    app().settings.tray_area.audio.device = "{missing-device}".into();
    app().settings.tray_video.audio.device = "{missing-device}".into();
    app().settings.tray_audio.device = "{missing-device}".into();
    app().settings.save();
    let reloaded = Settings::load();
    assert_eq!(reloaded.ui.audio.device, "Auto");
    assert_eq!(reloaded.tray_area.audio.device, "Auto");
    assert_eq!(reloaded.tray_video.audio.device, "Auto");
    assert_eq!(reloaded.tray_audio.device, "Auto");
    app().controls.load_ui(&app().settings);
    for id in [
        ID_MIC_DEVICE,
        ID_TRAY_AREA_DEVICE,
        ID_TRAY_VIDEO_DEVICE,
        ID_TRAY_AUDIO_DEVICE,
    ] {
        assert_eq!(app().controls.selected(id), 0);
    }
    app().command(ID_REFRESH, 0);
    assert_eq!(app().settings.ui.audio.device, "Auto");
    assert_eq!(app().settings.tray_area.audio.device, "Auto");
    assert_eq!(app().settings.tray_video.audio.device, "Auto");
    assert_eq!(app().settings.tray_audio.device, "Auto");
    let set_fps = |id, fps| {
        let slider = app().controls.get(id);
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

            SendMessageW(slider, TBM_SETPOS, WPARAM(1), LPARAM(fps));
            SendMessageW(app().hwnd, WM_HSCROLL, WPARAM(0), LPARAM(slider.0 as isize));
        }
    };
    // Tray area has three format toggles; each remembers its own picture.
    let mut area_pictures = [app().settings.tray_area.video[0]; 3];
    for (offset, fps) in [30, 15, 25].into_iter().enumerate() {
        app().command(ID_TRAY_AREA_FORMAT + offset as i32, 0);
        app().controls.select(ID_TRAY_AREA_PRESET, 2);
        app().command(ID_TRAY_AREA_PRESET, CBN_SELCHANGE as u16);
        set_fps(ID_TRAY_AREA_FORMAT + 4, fps);
        app().command(ID_REFRESH, 0);
        area_pictures[offset] = app().settings.tray_area.video[offset];
        assert_eq!(area_pictures[offset].fps, fps as u32);
    }
    app().command(ID_TRAY_AREA_FORMAT, 0);

    // Tray video has no format choice and is always MKV.
    app().controls.select(ID_TRAY_VIDEO_PRESET, 2);
    app().command(ID_TRAY_VIDEO_PRESET, CBN_SELCHANGE as u16);
    set_fps(ID_TRAY_VIDEO_FORMAT + 4, 20);
    app().command(ID_REFRESH, 0);
    assert_eq!(app().settings.tray_video.format, Format::Mkv);
    assert_eq!(app().settings.tray_video.video[Format::Mkv.index()].fps, 20);

    // The Capture tab keeps its per-format picture settings.
    app().controls.show_page(0);
    app().command(ID_FORMAT + 1, 0);
    app().controls.select(ID_PRESET, 0);
    app().command(ID_PRESET, CBN_SELCHANGE as u16);
    set_fps(ID_FPS, 50);
    app().command(ID_FORMAT + 2, 0);
    app().controls.select(ID_PRESET, 0);
    app().command(ID_PRESET, CBN_SELCHANGE as u16);
    set_fps(ID_FPS, 40);
    app().command(ID_FORMAT, 0);
    assert_eq!(
        (
            app().controls.selected(ID_RESOLUTION),
            app().settings.ui.video[0].fps
        ),
        (2, 60)
    );
    app().command(ID_FORMAT + 1, 0);
    assert_eq!(
        (
            app().controls.selected(ID_RESOLUTION),
            app().settings.ui.video[1].fps
        ),
        (3, 50)
    );
    for offset in 0..=2 {
        unsafe {
            // SAFETY: The HWND is a window this program created and has not destroyed.
            UpdateWindow(app().controls.get(ID_FORMAT + offset))
        };
    }
    snapshot("main-capture-radio-gif-100", &capture_window(app().hwnd));
    app().command(ID_FORMAT + 2, 0);
    assert_eq!(
        (
            app().controls.selected(ID_RESOLUTION),
            app().settings.ui.video[2].fps
        ),
        (3, 40)
    );
    let saved = Settings::load();
    assert_eq!(saved.tray_area.video, area_pictures);
    assert_eq!(saved.tray_video.format, Format::Mkv);
    assert_eq!(saved.tray_video.video[Format::Mkv.index()].fps, 20);
    assert_eq!((saved.ui.video[1].height, saved.ui.video[1].fps), (720, 50));
    assert_eq!((saved.ui.video[2].height, saved.ui.video[2].fps), (720, 40));
    app().controls.show_page(1);
    app().command(ID_TRAY_AREA_FORMAT, 0);
    app().controls.select(ID_TRAY_AREA_FORMAT + 3, 1);
    app().command(ID_TRAY_AREA_FORMAT + 3, CBN_SELCHANGE as u16);
    set_fps(ID_TRAY_AREA_FORMAT + 4, 45);
    app().controls.check(ID_TRAY_AREA_FORMAT + 6, false);
    app().command(ID_TRAY_AREA_FORMAT + 6, 0);
    set_fps(ID_TRAY_AREA_GAIN, 180);
    app().controls.select(ID_TRAY_VIDEO_PRESET, 2);
    app().command(ID_TRAY_VIDEO_PRESET, CBN_SELCHANGE as u16);
    set_fps(ID_TRAY_VIDEO_FORMAT + 4, 40);
    app().controls.check(ID_TRAY_VIDEO_FORMAT + 7, false);
    app().command(ID_TRAY_VIDEO_FORMAT + 7, 0);
    set_fps(ID_TRAY_VIDEO_GAIN, 40);
    app().controls.check(ID_TRAY_AUDIO_SYSTEM, false);
    app().command(ID_TRAY_AUDIO_SYSTEM, 0);
    set_fps(ID_TRAY_AUDIO_GAIN, 250);
    app().command(ID_REFRESH, 0);
    let area_video = app().settings.tray_area.video[0];
    assert_eq!(
        (
            area_video.height,
            area_video.fps,
            app().settings.tray_area.audio.system,
            app().settings.tray_area.audio.gain
        ),
        (1440, 45, false, 180)
    );
    assert_eq!(
        app().settings.tray_area.video[Format::Gif.index()],
        area_pictures[1]
    );
    assert_eq!(
        app().settings.tray_area.video[Format::Webp.index()],
        area_pictures[2]
    );
    assert_eq!(
        (
            app().settings.tray_video.format,
            app().settings.tray_video.video[Format::Mkv.index()].fps,
            app().settings.tray_video.audio.microphone,
            app().settings.tray_video.audio.gain
        ),
        (Format::Mkv, 40, false, 40)
    );
    assert_eq!(
        (
            app().settings.tray_audio.system,
            app().settings.tray_audio.microphone,
            app().settings.tray_audio.gain
        ),
        (false, true, 250)
    );
    assert_eq!(
        (
            app().settings.ui.video[0].height,
            app().settings.ui.video[0].fps
        ),
        (1080, 60)
    );
    assert_eq!(
        (
            app().settings.ui.video[1].height,
            app().settings.ui.video[1].fps
        ),
        (720, 50)
    );
    unsafe {
        // SAFETY: The HWND is a window this program created.
        UpdateWindow(app().hwnd)
    };
    std::thread::sleep(std::time::Duration::from_millis(40));
    snapshot("main-settings-profiles-100", &capture_window(app().hwnd));
    app().request_exit();
    unsafe {
        // SAFETY: The test owns the App stored in APP, and nothing else reads it after this.

        drop(Box::from_raw(APP));
        APP = std::ptr::null_mut();
    }
}

#[test]
#[ignore]
fn region_snapshots_on_a_fake_monitor() {
    let _ui = serialize_ui();
    media::init();
    register_test_classes();
    for scale in [100] {
        region::open_with_frame(synthetic_frame_sized(1366, 768), scale).unwrap();
        region::set_overlay(region::Overlay::Tray);
        region::hide_selection();
        std::thread::sleep(std::time::Duration::from_millis(40));
        if scale == 100 {
            snapshot("region-open-100", &capture_window(region::test_hwnd()));
        }
        region::restore_sample_selection();
        let before_selection = region::selection_screen_rect();
        let selection = before_selection;
        for role in [
            region::Role::Copy,
            region::Role::Save,
            region::Role::RecordMkv,
            region::Role::RecordGif,
            region::Role::RecordWebp,
            region::Role::Video,
            region::Role::Cancel,
        ] {
            let button =
                region::chrome_screen_rect(role).unwrap_or_else(|| panic!("{role:?} is missing"));
            let overlaps = button.left < selection.right
                && button.right > selection.left
                && button.top < selection.bottom
                && button.bottom > selection.top;
            assert!(!overlaps, "{role:?} covers the selection");
        }
        snapshot(
            &format!("region-{scale}"),
            &capture_window(region::test_hwnd()),
        );
        let overlay = region::test_hwnd();
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

            SendMessageW(
                overlay,
                WM_LBUTTONDOWN,
                WPARAM(0),
                LPARAM(150 | (250 << 16)),
            );
            SendMessageW(overlay, WM_MOUSEMOVE, WPARAM(0), LPARAM(180 | (270 << 16)));
            assert!(region::chrome_screen_rect(region::Role::Copy).is_none());
            SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(180 | (270 << 16)));
        }
        region::chrome_screen_rect(region::Role::Copy).expect("copy control after drag");
        let dragged = region::selection_screen_rect();
        assert_ne!(
            (dragged.left, dragged.top, dragged.right, dragged.bottom),
            (
                before_selection.left,
                before_selection.top,
                before_selection.right,
                before_selection.bottom
            ),
            "dragging did not create a new area"
        );
        region::select_tool(region::Role::Text);
        unsafe {
            // SAFETY: The HWND belongs to a window this process created. The RECT is a writable local.

            SendMessageW(
                overlay,
                WM_LBUTTONDOWN,
                WPARAM(0),
                LPARAM(280 | (220 << 16)),
            );
            SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(280 | (220 << 16)));
            let edit = region::text_edit();
            assert!(!edit.is_invalid(), "text tool did not open an editor");
            assert!(IsWindowVisible(edit).as_bool());
            let mut edit_before = RECT::default();
            GetWindowRect(edit, &mut edit_before).unwrap();
            SendMessageW(edit, WM_MBUTTONDOWN, WPARAM(0), LPARAM(10 | (10 << 16)));
            SendMessageW(overlay, WM_MOUSEMOVE, WPARAM(0), LPARAM(330 | (250 << 16)));
            SendMessageW(overlay, WM_MBUTTONUP, WPARAM(0), LPARAM(330 | (250 << 16)));
            let mut edit_after = RECT::default();
            GetWindowRect(edit, &mut edit_after).unwrap();
            assert_eq!(
                (
                    edit_after.left - edit_before.left,
                    edit_after.top - edit_before.top
                ),
                (40, 20)
            );
            UpdateWindow(edit);
            let dc = GetDC(edit);
            let pixel = GetPixel(dc, 5, 5);
            ReleaseDC(edit, dc);
            let channels = [pixel.0 & 255, (pixel.0 >> 8) & 255, (pixel.0 >> 16) & 255];
            assert!(
                channels.iter().any(|value| *value < 245),
                "text edit background is opaque: {channels:?}"
            );
            SetWindowTextW(edit, PCWSTR(wide("Hello").as_ptr())).unwrap();
            UpdateWindow(edit);
            UpdateWindow(overlay);
            if scale == 100 {
                snapshot("region-text", &capture_window(overlay));
            }
            SendMessageW(overlay, WM_LBUTTONDOWN, WPARAM(0), LPARAM(20 | (20 << 16)));
            SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(20 | (20 << 16)));
            assert!(
                !IsWindowVisible(edit).as_bool(),
                "clicking outside the text box should apply it"
            );
            if scale == 100 {
                region::demonstrate_tools();
                let frame = region::composited().expect("composited region");
                let red = frame
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[2] > 200 && pixel[0] < 40 && pixel[1] < 40)
                    .count();
                assert!(
                    red > 50,
                    "red stroke is missing from the finished picture, found {red} pixels"
                );
                assert!(
                    frame
                        .data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|pixel| pixel[2] > 200 && pixel[0] < 40)
                        .all(|pixel| pixel[3] == 255),
                    "red stroke is transparent and would look like a hole"
                );
                snapshot("region-result", &frame);
            }
        }
        region::shutdown();
    }
}

#[test]
fn text_tool_opens_an_editor() {
    let _ui = serialize_ui();
    register_test_classes();
    region::open_with_frame(synthetic_frame(), 100).unwrap();
    region::select_tool(region::Role::Text);
    let overlay = region::test_hwnd();
    unsafe {
        // SAFETY: The HWND belongs to a window this process created. The RECT is a writable local.

        SendMessageW(
            overlay,
            WM_LBUTTONDOWN,
            WPARAM(0),
            LPARAM(180 | (160 << 16)),
        );
        SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(180 | (160 << 16)));
        let edit = region::text_edit();
        assert!(
            !edit.is_invalid() && IsWindowVisible(edit).as_bool(),
            "choosing text and clicking must open the editor"
        );
        let mut before = RECT::default();
        GetWindowRect(edit, &mut before).unwrap();
        SendMessageW(edit, WM_MBUTTONDOWN, WPARAM(0), LPARAM(20 | (20 << 16)));
        SendMessageW(overlay, WM_MOUSEMOVE, WPARAM(0), LPARAM(200 | (190 << 16)));
        SendMessageW(overlay, WM_MBUTTONUP, WPARAM(0), LPARAM(200 | (190 << 16)));
        let mut after = RECT::default();
        GetWindowRect(edit, &mut after).unwrap();
        assert_ne!(
            (before.left, before.top),
            (after.left, after.top),
            "the mouse wheel button should drag the text box"
        );
    }
    region::shutdown();
}

#[test]
fn arrow_keys_stop_at_the_screen_edge_without_shrinking_the_area() {
    let _ui = serialize_ui();
    register_test_classes();
    let frame_width = 640;
    let frame_height = 480;
    region::open_with_frame(synthetic_frame(), 100).unwrap();
    let overlay = region::test_hwnd();
    let start = region::selection_screen_rect();
    let width = start.right - start.left;
    let height = start.bottom - start.top;
    let key = |key: u16| unsafe {
        // SAFETY: The target is the overlay this test created. The message carries no pointer.
        SendMessageW(overlay, WM_KEYDOWN, WPARAM(key as usize), LPARAM(0));
    };
    for _ in 0..start.left + 40 {
        key(VK_LEFT.0);
    }
    let left = region::selection_screen_rect();
    assert_eq!(
        (left.left, left.top),
        (0, start.top),
        "left arrow left the screen"
    );
    assert_eq!(
        (left.right - left.left, left.bottom - left.top),
        (width, height),
        "nudging against the left edge squeezed the area"
    );
    for _ in 0..frame_width + 40 {
        key(VK_RIGHT.0);
    }
    let right = region::selection_screen_rect();
    assert_eq!(
        right.left,
        frame_width - width,
        "right arrow did not stop at the edge"
    );
    assert_eq!(
        (right.right - right.left, right.bottom - right.top),
        (width, height),
        "nudging against the right edge squeezed the area"
    );
    for _ in 0..frame_height + 40 {
        key(VK_UP.0);
    }
    let top = region::selection_screen_rect();
    assert_eq!(top.top, 0, "up arrow left the screen");
    assert_eq!(
        (top.right - top.left, top.bottom - top.top),
        (width, height),
        "nudging against the top edge squeezed the area"
    );
    for _ in 0..frame_height + 40 {
        key(VK_DOWN.0);
    }
    let bottom = region::selection_screen_rect();
    assert_eq!(
        bottom.top,
        frame_height - height,
        "down arrow did not stop at the edge"
    );
    assert_eq!(
        (bottom.right - bottom.left, bottom.bottom - bottom.top),
        (width, height),
        "nudging against the bottom edge squeezed the area"
    );
    region::shutdown();
}

#[test]
fn closing_the_text_box_keeps_the_text_tool_usable() {
    let _ui = serialize_ui();
    register_test_classes();
    region::open_with_frame(synthetic_frame(), 100).unwrap();
    let overlay = region::test_hwnd();
    unsafe {
        // SAFETY: The target is a window this process created. Any pointer in the message addresses memory that outlives the call.
        region::select_tool(region::Role::Text);
        SendMessageW(
            overlay,
            WM_LBUTTONDOWN,
            WPARAM(0),
            LPARAM(180 | (160 << 16)),
        );
        SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(180 | (160 << 16)));
        let edit = region::text_edit();
        assert!(
            IsWindowVisible(edit).as_bool(),
            "the text tool did not open its editor"
        );
        SendMessageW(edit, WM_CLOSE, WPARAM(0), LPARAM(0));
        assert!(
            IsWindow(edit).as_bool(),
            "a close request destroyed only the text editor"
        );
        assert!(
            !IsWindowVisible(edit).as_bool(),
            "the text editor stayed on screen after the close request"
        );
        region::select_tool(region::Role::Text);
        SendMessageW(
            overlay,
            WM_LBUTTONDOWN,
            WPARAM(0),
            LPARAM(200 | (180 << 16)),
        );
        SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(200 | (180 << 16)));
        assert!(
            IsWindowVisible(region::text_edit()).as_bool(),
            "the text tool cannot open an editor after the first one was closed"
        );
    }
    region::shutdown();
}

#[test]
fn each_start_place_records_with_the_profile_tied_to_it() {
    use super::media::VideoSettings;

    let mut settings = Settings::default();
    settings.ui.format = Format::Gif;
    settings.ui.video[Format::Gif.index()] = VideoSettings {
        height: 360,
        fps: 12,
    };
    settings.ui.audio.device = "capture".into();
    settings.ui.audio.gain = 11;
    settings.tray_area.format = Format::Webp;
    settings.tray_area.video[Format::Webp.index()] = VideoSettings {
        height: 480,
        fps: 18,
    };
    settings.tray_area.audio.device = "area".into();
    settings.tray_area.audio.gain = 22;
    settings.tray_video.format = Format::Mkv;
    settings.tray_video.video[Format::Mkv.index()] = VideoSettings {
        height: 1440,
        fps: 48,
    };
    settings.tray_video.audio.device = "video".into();
    settings.tray_video.audio.gain = 33;
    settings.tray_audio.device = "audio".into();
    settings.tray_audio.gain = 44;

    let started = [
        (ID_CAPTURE_SCREEN, "capture", Some((360, 12))),
        (ID_CAPTURE_SOURCE, "capture", Some((360, 12))),
        (ID_CAPTURE_REGION, "capture", Some((360, 12))),
        (ID_CAPTURE_AUDIO, "capture", None),
        (ID_SHOT_REGION, "capture", Some((360, 12))),
        (ID_TRAY_AREA, "area", Some((480, 18))),
        (ID_TRAY_VIDEO, "video", Some((1440, 48))),
        (ID_TRAY_AUDIO, "audio", None),
    ];
    for (id, device, video) in started {
        let place =
            command_place(id).unwrap_or_else(|| panic!("control {id} does not start a recording"));
        let (source, take) = start_plan(place);
        let profile = settings.applied(source, take == Take::Picture);
        assert_eq!(profile.audio().device, device, "{place:?}");
        assert_eq!(
            profile.audio().gain,
            match device {
                "capture" => 11,
                "area" => 22,
                "video" => 33,
                "audio" => 44,
                _ => 0,
            }
        );
        assert_eq!(
            profile.video().map(|item| (item.height, item.fps)),
            video,
            "{place:?}"
        );
    }
    assert!(command_place(ID_TRAY_SHOT).is_none());
    assert!(command_place(ID_TRAY_STOP).is_none());

    assert_eq!(
        overlay_for(StartPlace::CaptureRegion),
        region::Overlay::Capture
    );
    assert_eq!(
        overlay_for(StartPlace::ShotRegion),
        region::Overlay::Screenshot
    );
    assert_eq!(overlay_for(StartPlace::TrayArea), region::Overlay::Tray);
    assert!(!region::Overlay::Capture.shows_video());
    assert!(!region::Overlay::Screenshot.shows_video());
    assert!(region::Overlay::Tray.shows_video());
    let (full_screen, take) = start_plan(area_video_place());
    let profile = settings.applied(full_screen, take == Take::Picture);
    assert_eq!(full_screen, ProfileRef::Video);
    assert_eq!(profile.audio().device, "video");
    assert_eq!(
        profile.video().map(|item| (item.height, item.fps)),
        Some((1440, 48))
    );

    for source in [ProfileRef::Capture, ProfileRef::Area] {
        let (resolved, take) = start_plan(place_for_source(source));
        let profile = settings.applied(resolved, take == Take::Picture);
        let device = if source == ProfileRef::Area {
            "area"
        } else {
            "capture"
        };
        assert_eq!(resolved, source);
        assert_eq!(take, Take::Picture);
        assert_eq!(profile.audio().device, device, "{source:?}");
    }
}

#[test]
fn tray_badge_stays_red_while_recording_and_orange_while_saving() {
    let recording = theme::bgra(theme::BADGE_RECORDING);
    let saving = theme::bgra(theme::BADGE_SAVING);
    assert!(
        recording[2] > 200 && recording[0] < 50 && recording[1] < 50,
        "recording badge is red, got {recording:?}"
    );
    assert!(
        saving[2] > 200 && saving[1] > 100 && saving[0] < 50,
        "saving badge is orange, got {saving:?}"
    );
    assert_eq!(recording, [32, 32, 230, 255]);
    assert_eq!(theme::bgra(theme::BADGE_RECORDING_EDGE), [16, 16, 140, 255]);
    assert_eq!(saving, [32, 140, 255, 255]);
    assert_eq!(theme::bgra(theme::BADGE_SAVING_EDGE), [16, 80, 170, 255]);
}

#[test]
fn the_tray_tip_names_area_video_and_audio() {
    assert_eq!(
        recording_kind_for(false, false, Format::Mkv),
        RecordingKind::Video
    );
    assert_eq!(
        recording_kind_for(false, false, Format::Gif),
        RecordingKind::Animation
    );
    assert_eq!(
        recording_kind_for(false, false, Format::Webp),
        RecordingKind::Animation
    );
    assert_eq!(
        recording_kind_for(false, true, Format::Mkv),
        RecordingKind::AreaVideo
    );
    assert_eq!(
        recording_kind_for(false, true, Format::Gif),
        RecordingKind::AreaAnimation
    );
    assert_eq!(
        recording_kind_for(false, true, Format::Webp),
        RecordingKind::AreaAnimation
    );
    assert_eq!(
        recording_kind_for(true, false, Format::Gif),
        RecordingKind::Audio
    );
    assert_eq!(
        recording_tip(RecordingKind::AreaVideo, 75),
        "ezcap — recording area video 01:15"
    );
    assert_eq!(
        recording_tip(RecordingKind::AreaAnimation, 75),
        "ezcap — recording area animation 01:15"
    );
    assert_eq!(
        recording_tip(RecordingKind::Video, 5),
        "ezcap — recording video 00:05"
    );
    assert_eq!(
        recording_tip(RecordingKind::Animation, 5),
        "ezcap — recording animation 00:05"
    );
    assert_eq!(
        recording_tip(RecordingKind::Audio, 0),
        "ezcap — recording audio 00:00"
    );
}

#[test]
fn the_red_outline_is_only_for_a_region_recording() {
    assert!(show_recording_outline(true, true));
    assert!(!show_recording_outline(true, false));
    assert!(!show_recording_outline(false, true));
    assert!(!show_recording_outline(false, false));
}

#[test]
fn reset_scale_stays_available_from_the_tray() {
    for mode in [Mode::Idle, Mode::Recording, Mode::Saving, Mode::Selecting] {
        assert!(
            accepts_command(mode, ID_TRAY_RESET_SCALE, false),
            "{mode:?}"
        );
    }
}

#[test]
fn the_tray_window_item_closes_an_open_window_and_opens_a_hidden_one() {
    assert_eq!(tray_window_label(true), "Close window");
    assert_eq!(tray_window_label(false), "Open window");
}

#[test]
fn history_stamp_reads_as_a_date_and_time() {
    assert_eq!(
        history_stamp(4, 11, 2026, 22, 56, 7),
        "4 Nov 2026, 22:56:07"
    );
}

#[test]
fn screenshots_stay_available_for_video_animation_and_audio() {
    for kind in ["video", "gif", "webp", "audio"] {
        for id in [
            ID_SHOT_SCREEN,
            ID_SHOT_SOURCE,
            ID_SHOT_REGION,
            ID_TRAY_SHOT,
            ID_TRAY_AREA,
        ] {
            assert!(
                accepts_command(Mode::Recording, id, false),
                "{kind} must still take a screenshot"
            );
        }
        for id in [
            ID_CAPTURE_SCREEN,
            ID_CAPTURE_SOURCE,
            ID_CAPTURE_REGION,
            ID_CAPTURE_AUDIO,
            ID_TRAY_VIDEO,
            ID_TRAY_AUDIO,
        ] {
            assert!(
                !accepts_command(Mode::Recording, id, false),
                "{kind} must not start a second recording"
            );
        }
        assert!(accepts_command(Mode::Recording, ID_STOP, false), "{kind}");
        assert!(
            accepts_command(Mode::Recording, ID_STATUS, false),
            "{kind} status link"
        );
        assert!(
            accepts_command(Mode::Recording, ID_HISTORY, false)
                && accepts_command(Mode::Recording, ID_HISTORY_COPY, false)
                && accepts_command(Mode::Recording, ID_HISTORY_REVEAL, false)
                && accepts_command(Mode::Recording, ID_HISTORY_OPEN, false),
            "{kind} history link"
        );
        assert!(
            !accepts_command(Mode::Saving, ID_SHOT_SCREEN, false),
            "{kind} while the file is still being saved"
        );
        assert!(
            !accepts_command(Mode::Recording, ID_SHOT_REGION, true),
            "{kind} while an area is already open"
        );
        assert!(
            tray_item_enabled(Mode::Recording, ID_TRAY_SHOT),
            "{kind} tray screenshot"
        );
        assert!(
            tray_item_enabled(Mode::Recording, ID_TRAY_AREA),
            "{kind} tray area"
        );
        assert!(
            !tray_item_enabled(Mode::Recording, ID_TRAY_VIDEO),
            "{kind} tray video"
        );
        assert!(
            !tray_item_enabled(Mode::Recording, ID_TRAY_AUDIO),
            "{kind} tray audio"
        );
        assert!(
            tray_item_enabled(Mode::Recording, ID_TRAY_STOP),
            "{kind} tray stop"
        );
    }
}

fn window_visible() -> bool {
    unsafe {
        // SAFETY: The HWND is the test window created for this process.
        IsWindowVisible(app().hwnd).as_bool()
    }
}

#[test]
fn a_ui_recording_returns_after_a_stop_from_the_tray_or_the_window() {
    let _ui = serialize_ui();
    register_test_classes();
    unsafe {
        // SAFETY: The class was registered for this process. The HWND is destroyed at the end of the test.
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            0,
            0,
            640,
            680,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .unwrap();
    }
    app().settings.hide_active = true;
    let stops = [ID_STOP, ID_TRAY_STOP];
    for kind in ["audio", "video", "region"] {
        for stop in stops {
            app().show_window();
            app().restore_visible = false;
            app().mode = Mode::Idle;
            assert!(window_visible(), "{kind} begins with the window open");
            if kind == "region" {
                app().conceal_for_ui(true);
                assert!(!window_visible(), "{kind} selection hides the window");
            }
            app().conceal_for_ui(true);
            assert!(!window_visible(), "{kind} started from the window hides it");
            assert!(
                app().restore_visible,
                "{kind} remembers to return the window"
            );
            test_mark_recording(app());
            app().command(stop, 0);
            assert!(
                app().restore_visible,
                "{kind} stop {stop} does not cancel the return"
            );
            assert!(!window_visible());
            app().restore_window();
            assert!(window_visible(), "{kind} opens again after stop {stop}");
            assert!(!app().restore_visible);
        }
    }
    app().show_window();
    app().restore_visible = false;
    app().conceal_for_ui(false);
    assert!(
        window_visible(),
        "a tray recording does not hide the window"
    );
    app().hide_window();
    app().restore_window();
    assert!(
        !window_visible(),
        "a tray recording does not open a window the user had closed"
    );
    unsafe {
        // SAFETY: The HWND is the test window created above and is not used after this call.
        DestroyWindow(app().hwnd).ok();
    }
}

#[test]
fn hiding_follows_the_window_and_not_the_tray() {
    assert!(hide_while_active(true, true));
    assert!(!hide_while_active(false, true));
    assert!(!hide_while_active(true, false));
    assert!(
        remember_restore(true, false),
        "a region started from the window still returns after a tray stop"
    );
    assert!(remember_restore(false, true));
    assert!(!remember_restore(false, false));
}

#[test]
fn recording_keeps_screenshot_controls_and_blocks_another_capture() {
    let _ui = serialize_ui();
    media::init();
    register_test_classes();
    unsafe {
        // SAFETY: The HWND is a child control created with the main window.

        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            0,
            0,
            640,
            680,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .unwrap();
    }
    app().show_window();
    app().controls.check(ID_HIDE_ACTIVE, true);
    test_mark_recording(app());
    unsafe {
        // SAFETY: The HWND is a child control created with the main window.

        assert!(IsWindowEnabled(app().controls.get(ID_SHOT_SCREEN)).as_bool());
        assert!(IsWindowEnabled(app().controls.get(ID_SHOT_REGION)).as_bool());
        assert!(!IsWindowEnabled(app().controls.get(ID_CAPTURE_SCREEN)).as_bool());
        assert!(!IsWindowEnabled(app().controls.get(ID_CAPTURE_REGION)).as_bool());
        assert!(!IsWindowEnabled(app().controls.get(ID_CAPTURE_AUDIO)).as_bool());
        assert!(IsWindowEnabled(app().controls.get(ID_STOP)).as_bool());
        assert!(IsWindowEnabled(app().controls.get(ID_STATUS)).as_bool());
    }
    app().command(ID_CAPTURE_SCREEN, 0);
    app().command(ID_CAPTURE_AUDIO, 0);
    app().command(ID_TRAY_VIDEO, 0);
    assert!(matches!(app().mode, Mode::Recording));
    let visible = unsafe {
        // SAFETY: The HWND is a window this program created.
        IsWindowVisible(app().hwnd).as_bool()
    };
    app().command(ID_TRAY_AREA, 0);
    assert!(region::active(), "a screenshot area opens during recording");
    assert_eq!(
        unsafe {
            // SAFETY: The HWND is a window this program created.
            IsWindowVisible(app().hwnd).as_bool()
        },
        visible,
        "the tray does not hide the window"
    );
    region::select_sample();
    region::press_role(region::Role::RecordMkv);
    assert!(
        !region::has_outcome(),
        "the format buttons stay off during a recording"
    );
    region::cancel();
    app().region_done();
    assert!(matches!(app().mode, Mode::Recording));
    unsafe {
        // SAFETY: The HWND is the test window created above and is not used after this call.
        DestroyWindow(app().hwnd).ok();
    }
}

#[test]
fn the_tray_format_buttons_ask_for_their_format() {
    let _ui = serialize_ui();
    register_test_classes();
    for (role, expected) in [
        (region::Role::RecordMkv, media::Format::Mkv),
        (region::Role::RecordGif, media::Format::Gif),
        (region::Role::RecordWebp, media::Format::Webp),
    ] {
        region::open_with_frame(synthetic_frame(), 100).unwrap();
        region::set_overlay(region::Overlay::Tray);
        region::select_sample();
        region::press_role(role);
        match region::take_outcome() {
            Some(region::Outcome::Record(_, Some(format))) => assert_eq!(format, expected),
            _ => panic!("{role:?} did not request {expected:?}"),
        }
    }
}

#[test]
fn the_tray_overlay_applies_the_saved_picture_for_each_format() {
    let _ui = serialize_ui();
    register_test_classes();
    let picture = |format| match format {
        Format::Mkv => media::VideoSettings {
            height: 1080,
            fps: 60,
        },
        Format::Gif => media::VideoSettings {
            height: 720,
            fps: 30,
        },
        Format::Webp => media::VideoSettings {
            height: 480,
            fps: 15,
        },
    };
    for format in [Format::Mkv, Format::Gif, Format::Webp] {
        app().settings.tray_area.video[format.index()] = picture(format);
    }
    app().settings.tray_area.audio.gain = 120;
    app().settings.tray_area.cursor = false;

    for (role, format) in [
        (region::Role::RecordMkv, Format::Mkv),
        (region::Role::RecordGif, Format::Gif),
        (region::Role::RecordWebp, Format::Webp),
    ] {
        region::open_with_frame(synthetic_frame(), 100).unwrap();
        region::set_overlay(region::Overlay::Tray);
        region::select_sample();
        region::press_role(role);
        let Some(region::Outcome::Record(_, Some(requested))) = region::take_outcome() else {
            panic!("{role:?} did not request a recording");
        };
        assert_eq!(requested, format);
        let (profile, audio_only) = app()
            .recording_for(ProfileRef::Area, Take::Picture, Some(requested))
            .session();
        assert!(!audio_only);
        assert_eq!(profile.format, format);
        assert_eq!(profile.video, picture(format));
        assert_eq!(profile.audio.gain, 120);
        assert!(!profile.cursor);
    }
}

#[test]
fn selecting_an_area_in_the_capture_tab_waits_for_record() {
    let _ui = serialize_ui();
    register_test_classes();
    region::open_with_frame(synthetic_frame(), 100).unwrap();
    region::set_overlay(region::Overlay::Capture);
    region::hide_selection();
    let overlay = region::test_hwnd();
    unsafe {
        // SAFETY: The target is the overlay this test created. The messages carry no pointers.
        SendMessageW(
            overlay,
            WM_LBUTTONDOWN,
            WPARAM(0),
            LPARAM(150 | (250 << 16)),
        );
        SendMessageW(overlay, WM_MOUSEMOVE, WPARAM(0), LPARAM(180 | (270 << 16)));
        SendMessageW(overlay, WM_LBUTTONUP, WPARAM(0), LPARAM(180 | (270 << 16)));
    }
    assert!(!region::has_outcome(), "the selection must wait for Record");
    region::press_role(region::Role::Record);
    match region::take_outcome() {
        Some(region::Outcome::Record(_, None)) => {}
        _ => panic!("Record did not start the area recording"),
    }
}

#[test]
fn record_stays_unavailable_while_a_capture_is_running() {
    let _ui = serialize_ui();
    register_test_classes();
    region::open_with_frame(synthetic_frame(), 100).unwrap();
    region::set_overlay(region::Overlay::Capture);
    region::set_record_allowed(false);
    region::press_role(region::Role::Record);
    assert!(
        !region::has_outcome(),
        "Record must not start another capture"
    );
    region::shutdown();
}

fn set_mic_slider(id: i32, value: i32) {
    let slider = app().controls.get(id);
    unsafe {
        // SAFETY: The target is a trackbar of the test window. The scroll message carries its handle.
        SendMessageW(slider, TBM_SETPOS, WPARAM(1), LPARAM(value as isize));
        SendMessageW(
            app().hwnd,
            WM_HSCROLL,
            WPARAM(TB_ENDTRACK as usize),
            LPARAM(slider.0 as isize),
        );
    }
}

#[test]
fn microphone_gain_is_per_widget_and_live_for_the_active_recording() {
    let _ui = serialize_ui();
    register_test_classes();
    unsafe {
        // SAFETY: The class was registered for this process. The HWND is destroyed at the end of the test.
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            0,
            0,
            640,
            680,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .unwrap();
    }
    app().settings = Settings::default();
    app().controls.load_ui(&app().settings);
    let gains = [
        ID_MIC_GAIN,
        ID_TRAY_AREA_GAIN,
        ID_TRAY_VIDEO_GAIN,
        ID_TRAY_AUDIO_GAIN,
    ];
    for id in gains {
        assert_eq!(
            app().controls.microphone_gain(id),
            100,
            "widget {id} did not start at 100%"
        );
    }
    test_mark_recording(app());
    app().live_gain = Some(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(100)));
    app().live_gain_id = Some(ID_MIC_GAIN);
    set_mic_slider(ID_MIC_GAIN, 237);
    assert_eq!(app().settings.ui.audio.gain, 237);
    assert_eq!(app().settings.tray_area.audio.gain, 100);
    assert_eq!(app().settings.tray_video.audio.gain, 100);
    assert_eq!(app().settings.tray_audio.gain, 100);
    assert_eq!(app().controls.microphone_gain(ID_MIC_GAIN), 237);
    assert_eq!(app().controls.microphone_gain(ID_TRAY_AREA_GAIN), 100);
    let mut label = [0u16; 16];
    let label_len = unsafe {
        // SAFETY: The target is a static control of the window created above and the buffer is writable.
        GetWindowTextW(app().controls.get(ID_MIC_GAIN_VALUE), &mut label)
    } as usize;
    assert_eq!(String::from_utf16_lossy(&label[..label_len]), "237%");
    assert_eq!(
        app()
            .live_gain
            .as_ref()
            .unwrap()
            .load(std::sync::atomic::Ordering::SeqCst),
        237,
        "the running recording did not receive the active widget's volume"
    );
    let saved = Settings::load();
    assert_eq!(saved.ui.audio.gain, 237);
    assert_eq!(saved.tray_area.audio.gain, 100);
    assert_eq!(saved.tray_video.audio.gain, 100);
    assert_eq!(saved.tray_audio.gain, 100);
    set_mic_slider(ID_TRAY_AREA_GAIN, 150);
    assert_eq!(app().settings.tray_area.audio.gain, 150);
    assert_eq!(app().settings.ui.audio.gain, 237);
    assert_eq!(
        app()
            .live_gain
            .as_ref()
            .unwrap()
            .load(std::sync::atomic::Ordering::SeqCst),
        237,
        "a different widget must not change the running recording"
    );
    set_mic_slider(ID_TRAY_VIDEO_GAIN, 200);
    set_mic_slider(ID_TRAY_AUDIO_GAIN, 250);
    assert_eq!(app().settings.tray_video.audio.gain, 200);
    assert_eq!(app().settings.tray_audio.gain, 250);
    set_mic_slider(ID_MIC_GAIN, 125);
    assert_eq!(
        app()
            .live_gain
            .as_ref()
            .unwrap()
            .load(std::sync::atomic::Ordering::SeqCst),
        125
    );
    app().live_gain_id = Some(ID_TRAY_AREA_GAIN);
    set_mic_slider(ID_TRAY_AREA_GAIN, 175);
    assert_eq!(
        app()
            .live_gain
            .as_ref()
            .unwrap()
            .load(std::sync::atomic::Ordering::SeqCst),
        175,
        "the active tray widget must drive the running mix"
    );
    let saved = Settings::load();
    assert_eq!(saved.ui.audio.gain, 125);
    assert_eq!(saved.tray_area.audio.gain, 175);
    assert_eq!(saved.tray_video.audio.gain, 200);
    assert_eq!(saved.tray_audio.gain, 250);
    unsafe {
        // SAFETY: These are controls of the window created above.
        assert!(
            IsWindowEnabled(app().controls.get(ID_MIC_GAIN)).as_bool(),
            "the main microphone volume must stay usable while recording"
        );
        assert!(
            IsWindowEnabled(app().controls.get(ID_TRAY_AREA_GAIN)).as_bool(),
            "the tray microphone volume must stay usable while recording"
        );
        assert!(
            IsWindowEnabled(app().controls.tab).as_bool(),
            "settings must still be reachable while recording"
        );
        assert!(
            !IsWindowEnabled(app().controls.get(ID_MIC_DEVICE)).as_bool(),
            "only the volume, not the whole audio section, should stay enabled"
        );
    }
    app().command(ID_SETTINGS_TAB, 0);
    assert_eq!(app().controls.page, 1);
    app().live_gain = None;
    app().live_gain_id = None;
    app().settings.ui.audio.gain = 100;
    app().settings.tray_area.audio.gain = 100;
    app().settings.tray_video.audio.gain = 100;
    app().settings.tray_audio.gain = 100;
    app().controls.set_microphone_gain(ID_MIC_GAIN, 100);
    app().controls.set_microphone_gain(ID_TRAY_AREA_GAIN, 100);
    app().controls.set_microphone_gain(ID_TRAY_VIDEO_GAIN, 100);
    app().controls.set_microphone_gain(ID_TRAY_AUDIO_GAIN, 100);
    app().settings.save();
    unsafe {
        // SAFETY: The HWND is the test window created above and is not used after this call.
        DestroyWindow(app().hwnd).ok();
    }
}

#[test]
#[ignore]
fn about_popup_snapshot() {
    let _ui = serialize_ui();
    media::init();
    register_test_classes();
    unsafe {
        // SAFETY: The HWND is a window this program created and has not destroyed.

        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            0,
            0,
            640,
            680,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .unwrap();
    }
    app().show_window();
    unsafe {
        // SAFETY: The HWND is a window this program created and has not destroyed.

        UpdateWindow(app().hwnd);
    }
    let frame = capture_about_dialog(app().hwnd);
    snapshot("about-popup", &frame);
    unsafe {
        // SAFETY: The HWND was created by this program and is not used after this call.

        DestroyWindow(app().hwnd).ok();
    }
}
