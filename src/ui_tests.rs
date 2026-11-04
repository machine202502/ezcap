use super::*;
use media::{Frame, Profile, VideoSettings};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::TBM_SETPOS;

fn snapshots_dir() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots");
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn capture_window(hwnd: HWND) -> Frame {
    unsafe {
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
    let width = 640;
    let height = 480;
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
fn main_window_snapshots_for_every_scale() {
    media::init();
    register_test_classes();
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
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
    unsafe { assert!(IsWindowVisible(app().controls.get(ID_REFRESH)).as_bool()) };
    app().controls.set_idle(false, false);
    unsafe { assert!(!IsWindowEnabled(app().controls.get(ID_REFRESH)).as_bool()) };
    app().controls.set_idle(true, false);
    for scale in [100, 125, 150, 175, 200, 225, 250, 300] {
        app().settings.scale = scale;
        app().controls.apply_scale(&app().settings);
        app().show_window();
        unsafe { UpdateWindow(app().hwnd) };
        std::thread::sleep(std::time::Duration::from_millis(40));
        snapshot(
            &format!("main-capture-{scale}"),
            &capture_window(app().hwnd),
        );
    }
    app().settings.scale = 200;
    app().controls.select(ID_SCALE, 5);
    app().controls.apply_scale(&app().settings);
    app().controls.show_page(1);
    assert_eq!(app().controls.page, 1);
    unsafe {
        assert!(IsWindowVisible(app().controls.get(ID_AUTOSTART)).as_bool());
        assert!(IsWindowVisible(app().controls.get(ID_REFRESH)).as_bool());
    }
    std::thread::sleep(std::time::Duration::from_millis(40));
    snapshot("main-settings-200", &capture_window(app().hwnd));
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
    assert_eq!(
        (
            app().settings.dyn_area.video[1].height,
            app().settings.dyn_area.video[1].fps
        ),
        (480, 30)
    );
    assert_eq!(
        (
            app().settings.dyn_video.video[0].height,
            app().settings.dyn_video.video[0].fps
        ),
        (1080, 60)
    );
    app().settings.ui.audio.device = "{missing-device}".into();
    app().settings.dyn_area.audio.device = "{missing-device}".into();
    app().settings.dyn_video.audio.device = "{missing-device}".into();
    app().settings.dyn_audio.device = "{missing-device}".into();
    app().settings.save();
    let reloaded = Settings::load();
    assert_eq!(reloaded.ui.audio.device, "Auto");
    assert_eq!(reloaded.dyn_area.audio.device, "Auto");
    assert_eq!(reloaded.dyn_video.audio.device, "Auto");
    assert_eq!(reloaded.dyn_audio.device, "Auto");
    app().controls.load_ui(&app().settings);
    for id in [
        ID_MIC_DEVICE,
        ID_DYN_AREA_DEVICE,
        ID_DYN_VIDEO_DEVICE,
        ID_DYN_AUDIO_DEVICE,
    ] {
        assert_eq!(app().controls.selected(id), 0);
    }
    app().command(ID_REFRESH, 0);
    assert_eq!(app().settings.ui.audio.device, "Auto");
    assert_eq!(app().settings.dyn_area.audio.device, "Auto");
    assert_eq!(app().settings.dyn_video.audio.device, "Auto");
    assert_eq!(app().settings.dyn_audio.device, "Auto");
    let set_fps = |id, fps| {
        let slider = app().controls.get(id);
        unsafe {
            SendMessageW(slider, TBM_SETPOS, WPARAM(1), LPARAM(fps));
            SendMessageW(app().hwnd, WM_HSCROLL, WPARAM(0), LPARAM(slider.0 as isize));
        }
    };
    app().command(ID_DYN_AREA_FORMAT, 0);
    app().controls.select(ID_DYN_AREA_PRESET, 2);
    app().command(ID_DYN_AREA_PRESET, CBN_SELCHANGE as u16);
    app().command(ID_DYN_AREA_FORMAT + 1, 0);
    app().controls.select(ID_DYN_AREA_PRESET, 2);
    app().command(ID_DYN_AREA_PRESET, CBN_SELCHANGE as u16);
    set_fps(ID_DYN_AREA_FORMAT + 4, 15);
    app().command(ID_DYN_AREA_FORMAT + 2, 0);
    set_fps(ID_DYN_AREA_FORMAT + 4, 25);
    app().command(ID_DYN_AREA_FORMAT, 0);
    assert_eq!(
        (
            app().controls.selected(ID_DYN_AREA_FORMAT + 3),
            app().settings.dyn_area.video[0].fps
        ),
        (3, 30)
    );
    app().command(ID_DYN_AREA_FORMAT + 1, 0);
    assert_eq!(
        (
            app().controls.selected(ID_DYN_AREA_FORMAT + 3),
            app().settings.dyn_area.video[1].fps
        ),
        (5, 15)
    );
    app().command(ID_DYN_AREA_FORMAT + 2, 0);
    assert_eq!(
        (
            app().controls.selected(ID_DYN_AREA_FORMAT + 3),
            app().settings.dyn_area.video[2].fps
        ),
        (4, 25)
    );
    app().command(ID_DYN_VIDEO_FORMAT + 2, 0);
    set_fps(ID_DYN_VIDEO_FORMAT + 4, 20);
    app().command(ID_DYN_VIDEO_FORMAT, 0);
    assert_eq!(
        (
            app().controls.selected(ID_DYN_VIDEO_FORMAT + 3),
            app().settings.dyn_video.video[0].fps
        ),
        (2, 60)
    );
    app().command(ID_DYN_VIDEO_FORMAT + 2, 0);
    assert_eq!(app().settings.dyn_video.video[2].fps, 20);
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
        unsafe { UpdateWindow(app().controls.get(ID_FORMAT + offset)) };
    }
    snapshot("main-capture-radio-gif-200", &capture_window(app().hwnd));
    app().command(ID_FORMAT + 2, 0);
    assert_eq!(
        (
            app().controls.selected(ID_RESOLUTION),
            app().settings.ui.video[2].fps
        ),
        (3, 40)
    );
    let saved = Settings::load();
    assert_eq!(
        (saved.dyn_area.video[0].height, saved.dyn_area.video[0].fps),
        (720, 30)
    );
    assert_eq!(
        (saved.dyn_area.video[1].height, saved.dyn_area.video[1].fps),
        (360, 15)
    );
    assert_eq!(
        (saved.dyn_area.video[2].height, saved.dyn_area.video[2].fps),
        (480, 25)
    );
    assert_eq!(
        (
            saved.dyn_video.video[0].height,
            saved.dyn_video.video[0].fps
        ),
        (1080, 60)
    );
    assert_eq!(
        (
            saved.dyn_video.video[2].height,
            saved.dyn_video.video[2].fps
        ),
        (480, 20)
    );
    assert_eq!((saved.ui.video[1].height, saved.ui.video[1].fps), (720, 50));
    assert_eq!((saved.ui.video[2].height, saved.ui.video[2].fps), (720, 40));
    app().request_exit();
    unsafe {
        drop(Box::from_raw(APP));
        APP = std::ptr::null_mut();
    }
}

#[test]
#[ignore]
fn region_snapshots_on_a_fake_monitor() {
    media::init();
    register_test_classes();
    for scale in [100, 150, 200] {
        region::open_with_frame(
            synthetic_frame(),
            Profile {
                format: Format::Gif,
                video: VideoSettings {
                    height: 480,
                    fps: 30,
                },
                audio: AudioSettings::default(),
            },
            scale,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(40));
        let selection = region::selection_screen_rect();
        for role in [
            region::Role::Copy,
            region::Role::Save,
            region::Role::Record,
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
        let before = region::chrome_screen_rect(region::Role::Copy).expect("copy control");
        unsafe {
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
        let after =
            region::chrome_screen_rect(region::Role::Copy).expect("copy control after drag");
        assert_ne!((before.left, before.top), (after.left, after.top));
        region::select_tool(region::Role::Text);
        unsafe {
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
            SendMessageW(edit, WM_LBUTTONDOWN, WPARAM(0), LPARAM(10 | (10 << 16)));
            SendMessageW(edit, WM_MOUSEMOVE, WPARAM(0), LPARAM(50 | (30 << 16)));
            SendMessageW(edit, WM_LBUTTONUP, WPARAM(0), LPARAM(50 | (30 << 16)));
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
    register_test_classes();
    let profile = Profile {
        format: Format::Gif,
        video: VideoSettings {
            height: 480,
            fps: 30,
        },
        audio: AudioSettings::default(),
    };
    region::open_with_frame(synthetic_frame(), profile, 100).unwrap();
    region::select_tool(region::Role::Text);
    let overlay = region::test_hwnd();
    unsafe {
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
fn history_stamp_reads_as_a_date_and_time() {
    assert_eq!(
        history_stamp(4, 11, 2026, 22, 56, 7),
        "4 Nov 2026, 22:56:07"
    );
}

#[test]
fn screenshots_stay_available_for_video_animation_and_audio() {
    for kind in ["video", "gif", "webp", "audio"] {
        for id in [ID_SHOT_SCREEN, ID_SHOT_REGION, ID_TRAY_SHOT, ID_TRAY_AREA] {
            assert!(
                accepts_command(Mode::Recording, id, false),
                "{kind} must still take a screenshot"
            );
        }
        for id in [
            ID_CAPTURE_SCREEN,
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
            accepts_command(Mode::Recording, ID_HISTORY, false),
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

#[test]
fn hiding_follows_the_window_and_not_the_tray() {
    assert!(hide_while_active(true, true));
    assert!(!hide_while_active(false, true));
    assert!(!hide_while_active(true, false));
}

#[test]
fn recording_keeps_screenshot_controls_and_blocks_another_capture() {
    media::init();
    register_test_classes();
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
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
    let visible = unsafe { IsWindowVisible(app().hwnd).as_bool() };
    app().command(ID_TRAY_AREA, 0);
    assert!(region::active(), "a screenshot area opens during recording");
    assert_eq!(
        unsafe { IsWindowVisible(app().hwnd).as_bool() },
        visible,
        "the tray does not hide the window"
    );
    region::select_sample();
    region::press_role(region::Role::Record);
    assert!(
        !region::has_outcome(),
        "Record stays off during a recording"
    );
    region::cancel();
    app().region_done();
    assert!(matches!(app().mode, Mode::Recording));
    unsafe {
        DestroyWindow(app().hwnd).ok();
    }
}

#[test]
fn record_stays_unavailable_while_a_capture_is_running() {
    register_test_classes();
    let profile = Profile {
        format: Format::Gif,
        video: VideoSettings {
            height: 480,
            fps: 30,
        },
        audio: AudioSettings::default(),
    };
    region::open_with_frame(synthetic_frame(), profile, 100).unwrap();
    region::set_record_allowed(false);
    region::press_role(region::Role::Record);
    assert!(
        !region::has_outcome(),
        "Record must not start another capture"
    );
    region::shutdown();
}

#[test]
#[ignore]
fn about_popup_snapshot() {
    media::init();
    register_test_classes();
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
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
        UpdateWindow(app().hwnd);
    }
    let frame = capture_about_dialog(app().hwnd);
    snapshot("about-popup", &frame);
    unsafe {
        DestroyWindow(app().hwnd).ok();
    }
}
