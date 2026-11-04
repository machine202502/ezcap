use crate::{media, wide, AudioSettings, CaptureSettings, Settings};
use media::{Format, VideoSettings};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const ID_TAB: i32 = 100;
pub const ID_SETTINGS_TAB: i32 = 101;
pub const ID_REFRESH: i32 = 102;
pub const ID_ABOUT: i32 = 103;
pub const ID_FORMAT: i32 = 110;
pub const ID_PRESET: i32 = 120;
pub const ID_RESOLUTION: i32 = 121;
pub const ID_FPS: i32 = 122;
pub const ID_FPS_VALUE: i32 = 123;
pub const ID_SYSTEM_AUDIO: i32 = 124;
pub const ID_MIC: i32 = 125;
pub const ID_MIC_DEVICE: i32 = 126;
pub const ID_SHOT_SCREEN: i32 = 130;
pub const ID_SHOT_REGION: i32 = 131;
pub const ID_CAPTURE_SCREEN: i32 = 132;
pub const ID_CAPTURE_REGION: i32 = 133;
pub const ID_CAPTURE_AUDIO: i32 = 134;
pub const ID_STOP: i32 = 135;
pub const ID_STATUS: i32 = 136;
pub const ID_HISTORY: i32 = 210;
pub const HISTORY_COUNT: i32 = 12;

pub fn history_id(id: i32) -> bool {
    (ID_HISTORY..ID_HISTORY + HISTORY_COUNT).contains(&id)
}
pub const ID_SHOT_LABEL: i32 = 137;
pub const ID_CAPTURE_LABEL: i32 = 138;
pub const ID_AUDIO_LABEL: i32 = 139;
pub const ID_STOP_LABEL: i32 = 140;
pub const ID_AUTOSTART: i32 = 150;
pub const ID_START_MINIMIZED: i32 = 151;
pub const ID_HIDE_ACTIVE: i32 = 152;
pub const ID_CLOSE_TRAY: i32 = 153;
pub const ID_CLIPBOARD: i32 = 154;
pub const ID_TRAY_CAPTURE: i32 = 155;
pub const ID_HIGHLIGHT: i32 = 156;
pub const ID_NOTIFY: i32 = 157;
pub const ID_DYN_AREA_FORMAT: i32 = 160;
pub const ID_DYN_AREA_FPS_VALUE: i32 = 165;
pub const ID_DYN_AREA_DEVICE: i32 = 168;
pub const ID_DYN_AREA_PRESET: i32 = 169;
pub const ID_DYN_VIDEO_FORMAT: i32 = 170;
pub const ID_DYN_VIDEO_FPS_VALUE: i32 = 175;
pub const ID_DYN_VIDEO_DEVICE: i32 = 178;
pub const ID_DYN_VIDEO_PRESET: i32 = 179;
pub const ID_THREADS: i32 = 180;
pub const ID_AREA_HEADING: i32 = 181;
pub const ID_VIDEO_HEADING: i32 = 182;
pub const ID_THREADS_HEADING: i32 = 183;
pub const ID_SCALE_HEADING: i32 = 184;
pub const ID_SCALE: i32 = 185;
pub const ID_DYN_AUDIO_SYSTEM: i32 = 190;
pub const ID_DYN_AUDIO_MIC: i32 = 191;
pub const ID_DYN_AUDIO_DEVICE: i32 = 192;
pub const ID_DYN_AUDIO_HEADING: i32 = 193;
pub use crate::theme::{ACCENT, ACCENT_DEEP, BACKGROUND, DISABLED, MIST, TEXT};

struct Place {
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

const fn p(id: i32, x: i32, y: i32, w: i32, h: i32) -> Place {
    Place { id, x, y, w, h }
}

const TABS: [Place; 4] = [
    p(ID_TAB, 12, 10, 110, 32),
    p(ID_SETTINGS_TAB, 126, 10, 110, 32),
    p(ID_REFRESH, 468, 10, 100, 32),
    p(ID_ABOUT, 580, 10, 32, 32),
];
const CAPTURE: [Place; 21] = [
    p(ID_FORMAT, 28, 58, 184, 26),
    p(ID_FORMAT + 1, 222, 58, 184, 26),
    p(ID_FORMAT + 2, 416, 58, 184, 26),
    p(ID_PRESET, 28, 96, 280, 300),
    p(ID_RESOLUTION, 320, 96, 280, 300),
    p(ID_FPS, 28, 136, 480, 28),
    p(ID_FPS_VALUE, 520, 140, 80, 24),
    p(ID_SYSTEM_AUDIO, 28, 178, 220, 26),
    p(ID_MIC, 28, 208, 160, 26),
    p(ID_MIC_DEVICE, 196, 207, 404, 300),
    p(ID_SHOT_LABEL, 28, 252, 572, 22),
    p(ID_SHOT_SCREEN, 28, 278, 280, 36),
    p(ID_SHOT_REGION, 320, 278, 280, 36),
    p(ID_CAPTURE_LABEL, 28, 326, 572, 22),
    p(ID_CAPTURE_SCREEN, 28, 352, 280, 36),
    p(ID_CAPTURE_REGION, 320, 352, 280, 36),
    p(ID_AUDIO_LABEL, 28, 400, 572, 22),
    p(ID_CAPTURE_AUDIO, 28, 426, 572, 36),
    p(ID_STOP_LABEL, 28, 474, 572, 22),
    p(ID_STOP, 28, 500, 572, 36),
    p(ID_STATUS, 28, 552, 572, 28),
];

const SETTINGS: [Place; 38] = [
    p(ID_AUTOSTART, 28, 56, 572, 26),
    p(ID_START_MINIMIZED, 28, 87, 572, 26),
    p(ID_HIDE_ACTIVE, 28, 118, 572, 26),
    p(ID_CLOSE_TRAY, 28, 149, 572, 26),
    p(ID_CLIPBOARD, 28, 180, 572, 26),
    p(ID_TRAY_CAPTURE, 28, 211, 572, 26),
    p(ID_HIGHLIGHT, 28, 242, 572, 26),
    p(ID_NOTIFY, 28, 273, 572, 26),
    p(ID_AREA_HEADING, 28, 322, 400, 24),
    p(ID_DYN_AREA_FORMAT, 28, 348, 184, 26),
    p(ID_DYN_AREA_FORMAT + 1, 222, 348, 184, 26),
    p(ID_DYN_AREA_FORMAT + 2, 416, 348, 184, 26),
    p(ID_DYN_AREA_PRESET, 28, 386, 280, 300),
    p(ID_DYN_AREA_FORMAT + 3, 320, 386, 280, 300),
    p(ID_DYN_AREA_FORMAT + 4, 28, 424, 480, 28),
    p(ID_DYN_AREA_FPS_VALUE, 520, 428, 80, 24),
    p(ID_DYN_AREA_FORMAT + 6, 28, 464, 240, 26),
    p(ID_DYN_AREA_FORMAT + 7, 28, 496, 160, 26),
    p(ID_DYN_AREA_DEVICE, 196, 495, 404, 300),
    p(ID_VIDEO_HEADING, 28, 552, 400, 24),
    p(ID_DYN_VIDEO_FORMAT, 28, 578, 184, 26),
    p(ID_DYN_VIDEO_FORMAT + 1, 222, 578, 184, 26),
    p(ID_DYN_VIDEO_FORMAT + 2, 416, 578, 184, 26),
    p(ID_DYN_VIDEO_PRESET, 28, 616, 280, 300),
    p(ID_DYN_VIDEO_FORMAT + 3, 320, 616, 280, 300),
    p(ID_DYN_VIDEO_FORMAT + 4, 28, 654, 480, 28),
    p(ID_DYN_VIDEO_FPS_VALUE, 520, 658, 80, 24),
    p(ID_DYN_VIDEO_FORMAT + 6, 28, 694, 240, 26),
    p(ID_DYN_VIDEO_FORMAT + 7, 28, 726, 160, 26),
    p(ID_DYN_VIDEO_DEVICE, 196, 725, 404, 300),
    p(ID_DYN_AUDIO_HEADING, 28, 782, 400, 24),
    p(ID_DYN_AUDIO_SYSTEM, 28, 808, 240, 26),
    p(ID_DYN_AUDIO_MIC, 28, 840, 160, 26),
    p(ID_DYN_AUDIO_DEVICE, 196, 839, 404, 300),
    p(ID_THREADS_HEADING, 28, 900, 250, 24),
    p(ID_THREADS, 28, 926, 180, 300),
    p(ID_SCALE_HEADING, 320, 900, 250, 24),
    p(ID_SCALE, 320, 926, 180, 300),
];

pub struct Controls {
    pub tab: HWND,
    pub settings_tab: HWND,
    pub capture: Vec<HWND>,
    pub settings: Vec<HWND>,
    pub values: Vec<(i32, HWND)>,
    pub page: usize,
    pub font: HFONT,
    pub format_index: usize,
    pub dyn_area_format_index: usize,
    pub dyn_video_format_index: usize,
    hwnd: HWND,
    scale_percent: u32,
    scale_setting: u32,
}

impl Controls {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            tab: HWND::default(),
            settings_tab: HWND::default(),
            capture: Vec::new(),
            settings: Vec::new(),
            values: Vec::new(),
            page: 0,
            font: HFONT::default(),
            format_index: 0,
            dyn_area_format_index: 1,
            dyn_video_format_index: 0,
            hwnd,
            scale_percent: 100,
            scale_setting: u32::MAX,
        }
    }

    pub fn get(&self, id: i32) -> HWND {
        self.values
            .iter()
            .find(|v| v.0 == id)
            .map(|v| v.1)
            .unwrap_or_default()
    }

    fn create_control(
        &mut self,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        ex: WINDOW_EX_STYLE,
        id: i32,
        page: usize,
    ) -> HWND {
        let mut text = wide(text);
        let menu = HMENU(id as usize as *mut std::ffi::c_void);
        let hwnd = unsafe {
            CreateWindowExW(
                ex,
                class,
                PCWSTR(text.as_mut_ptr()),
                WS_CHILD | style,
                0,
                0,
                10,
                10,
                self.hwnd,
                menu,
                GetModuleHandleW(None).unwrap(),
                None,
            )
        }
        .unwrap();
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, WPARAM(self.font.0 as usize), LPARAM(1));
            windows::Win32::UI::Controls::SetWindowTheme(hwnd, w!("Explorer"), PCWSTR::null()).ok();
        }
        if page == 0 {
            self.capture.push(hwnd);
        } else if page == 1 {
            self.settings.push(hwnd);
        }
        self.values.push((id, hwnd));
        hwnd
    }

    pub fn build(&mut self, settings: &mut Settings) {
        self.font = scaled_font(self.scale_percent);
        let style = WINDOW_STYLE(WS_TABSTOP.0 | BS_OWNERDRAW as u32);
        self.tab = self.create_control(
            w!("BUTTON"),
            "Capture",
            style,
            WINDOW_EX_STYLE(0),
            ID_TAB,
            2,
        );
        self.settings_tab = self.create_control(
            w!("BUTTON"),
            "Settings",
            style,
            WINDOW_EX_STYLE(0),
            ID_SETTINGS_TAB,
            2,
        );
        self.create_control(
            w!("BUTTON"),
            "Refresh",
            style,
            WINDOW_EX_STYLE(0),
            ID_REFRESH,
            2,
        );
        self.create_control(w!("BUTTON"), "i", style, WINDOW_EX_STYLE(0), ID_ABOUT, 2);
        for (i, text) in ["MKV", "GIF", "WebP"].iter().enumerate() {
            self.create_control(
                w!("BUTTON"),
                text,
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                WINDOW_EX_STYLE(0),
                ID_FORMAT + i as i32,
                0,
            );
        }
        self.combo(ID_PRESET, &["Quality", "Balance", "Speed"], 0);
        self.combo(
            ID_RESOLUTION,
            &["2160 (4K)", "1440 (2K)", "1080 (FHD)", "720", "480", "360"],
            0,
        );
        self.create_control(
            TRACKBAR_CLASS,
            "",
            WINDOW_STYLE(TBS_HORZ | TBS_AUTOTICKS),
            WINDOW_EX_STYLE(0),
            ID_FPS,
            0,
        );
        let fps = self.get(ID_FPS);
        unsafe {
            SendMessageW(
                fps,
                TBM_SETRANGE,
                WPARAM(1),
                LPARAM((5 | (60 << 16)) as isize),
            );
            SendMessageW(fps, TBM_SETTICFREQ, WPARAM(5), LPARAM(0));
        }
        self.create_control(
            w!("STATIC"),
            "60 FPS",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            ID_FPS_VALUE,
            0,
        );
        self.create_control(
            w!("BUTTON"),
            "System audio",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            ID_SYSTEM_AUDIO,
            0,
        );
        self.create_control(
            w!("BUTTON"),
            "Microphone",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            ID_MIC,
            0,
        );
        self.combo(ID_MIC_DEVICE, &["Auto"], 0);
        for (id, text) in [
            (ID_SHOT_LABEL, "Screenshot:"),
            (ID_CAPTURE_LABEL, "Capture:"),
            (ID_AUDIO_LABEL, "Audio:"),
            (ID_STOP_LABEL, "Stop:"),
        ] {
            self.create_control(
                w!("STATIC"),
                text,
                WINDOW_STYLE(0),
                WINDOW_EX_STYLE(0),
                id,
                0,
            );
        }
        for (id, text) in [
            (ID_SHOT_SCREEN, "Screen"),
            (ID_SHOT_REGION, "Area"),
            (ID_CAPTURE_SCREEN, "Screen"),
            (ID_CAPTURE_REGION, "Area"),
            (ID_CAPTURE_AUDIO, "Audio"),
            (ID_STOP, "Stop capture"),
        ] {
            self.create_control(
                w!("BUTTON"),
                text,
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                WINDOW_EX_STYLE(0),
                id,
                0,
            );
        }
        self.create_control(
            w!("STATIC"),
            "Ready",
            WINDOW_STYLE(0x0100 | 0x4000),
            WINDOW_EX_STYLE(0),
            ID_STATUS,
            0,
        );
        for index in 0..HISTORY_COUNT {
            self.create_control(
                w!("STATIC"),
                "",
                WINDOW_STYLE(0x0100 | 0x4000),
                WINDOW_EX_STYLE(0),
                ID_HISTORY + index,
                2,
            );
        }
        for (id, text) in [
            (ID_AUTOSTART, "Start with Windows"),
            (ID_START_MINIMIZED, "Start minimized"),
            (
                ID_HIDE_ACTIVE,
                "Hide to tray while recording or selecting an area",
            ),
            (ID_CLOSE_TRAY, "Close to tray"),
            (ID_CLIPBOARD, "Copy the saved file to clipboard"),
            (ID_TRAY_CAPTURE, "Select an area on tray icon click"),
            (ID_HIGHLIGHT, "Highlight the recording area"),
            (ID_NOTIFY, "Play a sound when recording finishes"),
        ] {
            self.create_control(
                w!("BUTTON"),
                text,
                WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                WINDOW_EX_STYLE(0),
                id,
                1,
            );
        }
        self.create_control(
            w!("STATIC"),
            "Tray area recording",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            ID_AREA_HEADING,
            1,
        );
        self.create_control(
            w!("STATIC"),
            "Tray video recording",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            ID_VIDEO_HEADING,
            1,
        );
        self.create_control(
            w!("STATIC"),
            "FFmpeg threads",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            ID_THREADS_HEADING,
            1,
        );
        self.create_control(
            w!("STATIC"),
            "Interface scale",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            ID_SCALE_HEADING,
            1,
        );
        self.create_control(
            w!("STATIC"),
            "Tray audio recording",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            ID_DYN_AUDIO_HEADING,
            1,
        );
        self.create_control(
            w!("BUTTON"),
            "System audio",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            ID_DYN_AUDIO_SYSTEM,
            1,
        );
        self.create_control(
            w!("BUTTON"),
            "Microphone",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            ID_DYN_AUDIO_MIC,
            1,
        );
        self.combo(ID_DYN_AUDIO_DEVICE, &["Auto"], 1);
        self.dynamic_controls(true);
        self.dynamic_controls(false);
        self.combo(ID_THREADS, &["Auto", "1", "2", "4"], 1);
        self.combo(
            ID_SCALE,
            &["Auto", "1", "1.25", "1.5", "1.75", "2", "2.25", "2.5", "3"],
            1,
        );
        self.load_ui(settings);
        self.show_page(0);
        self.apply_scale(settings);
    }

    fn combo(&mut self, id: i32, items: &[&str], page: usize) {
        let hwnd = self.create_control(
            w!("COMBOBOX"),
            "",
            WINDOW_STYLE(CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
            WINDOW_EX_STYLE(0),
            id,
            page,
        );
        for item in items {
            let item = wide(item);
            unsafe {
                SendMessageW(
                    hwnd,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(item.as_ptr() as isize),
                )
            };
        }
        unsafe { SendMessageW(hwnd, CB_SETCURSEL, WPARAM(0), LPARAM(0)) };
    }

    fn dynamic_controls(&mut self, area: bool) {
        let base = if area {
            ID_DYN_AREA_FORMAT
        } else {
            ID_DYN_VIDEO_FORMAT
        };
        for (i, text) in ["MKV", "GIF", "WebP"].iter().enumerate() {
            self.create_control(
                w!("BUTTON"),
                text,
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                WINDOW_EX_STYLE(0),
                base + i as i32,
                1,
            );
        }
        self.combo(base + 9, &["Quality", "Balance", "Speed"], 1);
        self.combo(
            base + 3,
            &["2160 (4K)", "1440 (2K)", "1080 (FHD)", "720", "480", "360"],
            1,
        );
        let fps = self.create_control(
            TRACKBAR_CLASS,
            "",
            WINDOW_STYLE(TBS_HORZ | TBS_AUTOTICKS),
            WINDOW_EX_STYLE(0),
            base + 4,
            1,
        );
        unsafe {
            SendMessageW(
                fps,
                TBM_SETRANGE,
                WPARAM(1),
                LPARAM((5 | (60 << 16)) as isize),
            );
            SendMessageW(fps, TBM_SETTICFREQ, WPARAM(5), LPARAM(0));
        }
        self.create_control(
            w!("STATIC"),
            "60 FPS",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            base + 5,
            1,
        );
        self.create_control(
            w!("BUTTON"),
            "System audio",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            base + 6,
            1,
        );
        self.create_control(
            w!("BUTTON"),
            "Microphone",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            base + 7,
            1,
        );
        self.combo(base + 8, &["Auto"], 1);
    }

    pub fn show_page(&mut self, page: usize) {
        self.page = page;
        unsafe {
            ShowWindow(self.tab, SW_SHOW);
            ShowWindow(self.settings_tab, SW_SHOW);
            ShowWindow(self.get(ID_REFRESH), SW_SHOW);
            ShowWindow(self.get(ID_ABOUT), SW_SHOW)
        };
        for h in &self.capture {
            unsafe { ShowWindow(*h, if page == 0 { SW_SHOW } else { SW_HIDE }) };
        }
        for h in &self.settings {
            unsafe { ShowWindow(*h, if page == 1 { SW_SHOW } else { SW_HIDE }) };
        }
        self.layout();
        self.resize_window();
        self.refresh_formats();
        unsafe { InvalidateRect(self.hwnd, None, true) };
    }

    fn place(&self, place: &Place) {
        let scale = |value: i32| value * self.scale_percent as i32 / 100;
        unsafe {
            MoveWindow(
                self.get(place.id),
                scale(place.x),
                scale(place.y),
                scale(place.w),
                scale(place.h),
                true,
            )
        }
        .ok();
    }

    pub fn layout(&self) {
        for place in &TABS {
            self.place(place);
        }
        if self.page == 0 {
            for place in &CAPTURE {
                self.place(place);
            }
            for index in 0..HISTORY_COUNT {
                self.place(&p(ID_HISTORY + index, 28, 588 + index * 28, 572, 24));
            }
        } else {
            for place in &SETTINGS {
                self.place(place);
            }
        }
        self.sync_history();
    }

    pub fn apply_scale(&mut self, settings: &Settings) {
        let percent = interface_scale(settings.scale);
        if self.scale_percent == percent && self.scale_setting == settings.scale {
            return;
        }
        self.scale_percent = percent;
        self.scale_setting = settings.scale;
        if !self.font.is_invalid() {
            unsafe { DeleteObject(self.font) };
        }
        self.font = scaled_font(percent);
        for (_, hwnd) in &self.values {
            unsafe { SendMessageW(*hwnd, WM_SETFONT, WPARAM(self.font.0 as usize), LPARAM(1)) };
        }
        self.layout();
        self.resize_window();
    }

    fn resize_window(&self) {
        let percent = self.scale_percent;
        let width = 628 * percent as i32 / 100;
        let height = 1002 * percent as i32 / 100;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        unsafe {
            AdjustWindowRectEx(&mut rect, style, false, WINDOW_EX_STYLE(0)).ok();
            let mut current = RECT::default();
            GetWindowRect(self.hwnd, &mut current).ok();
            SetWindowPos(
                self.hwnd,
                None,
                current.left,
                current.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER,
            )
            .ok();
        }
    }

    pub fn load_ui(&mut self, settings: &Settings) {
        self.format_index = settings.ui.format.index();
        self.load_main_video(&settings.ui);
        self.load_audio(ID_SYSTEM_AUDIO, ID_MIC, ID_MIC_DEVICE, &settings.ui.audio);
        self.load_dynamic(settings, true);
        self.load_dynamic(settings, false);
        self.load_audio(
            ID_DYN_AUDIO_SYSTEM,
            ID_DYN_AUDIO_MIC,
            ID_DYN_AUDIO_DEVICE,
            &settings.dyn_audio,
        );
        for (id, value) in [
            (ID_AUTOSTART, settings.autostart),
            (ID_START_MINIMIZED, settings.start_minimized),
            (ID_HIDE_ACTIVE, settings.hide_active),
            (ID_CLOSE_TRAY, settings.close_tray),
            (ID_CLIPBOARD, settings.clipboard),
            (ID_TRAY_CAPTURE, settings.tray_capture),
            (ID_HIGHLIGHT, settings.highlight_region),
            (ID_NOTIFY, settings.completion_sound),
        ] {
            self.check(id, value);
        }
        self.select(
            ID_THREADS,
            match settings.threads {
                1 => 1,
                2 => 2,
                4 => 3,
                _ => 0,
            },
        );
        self.select(ID_SCALE, scale_index(settings.scale));
    }

    pub fn load_main_video(&mut self, settings: &CaptureSettings) {
        let v = settings.video[settings.format.index()];
        self.select(ID_RESOLUTION, resolution_index(v.height));
        unsafe {
            SendMessageW(
                self.get(ID_FPS),
                TBM_SETPOS,
                WPARAM(1),
                LPARAM(v.fps as isize),
            )
        };
        self.set_text(ID_FPS_VALUE, &format!("{} FPS", v.fps));
        self.select(ID_PRESET, preset_index(settings.format, v));
    }

    fn load_audio(&mut self, system: i32, mic: i32, device: i32, audio: &AudioSettings) {
        self.check(system, audio.system);
        self.check(mic, audio.microphone);
        let hwnd = self.get(device);
        let selected = if audio.device == "Auto" {
            "Auto".to_string()
        } else {
            media::device_name(&audio.device).unwrap_or_else(|| "Auto".to_string())
        };
        unsafe {
            SendMessageW(hwnd, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
            SendMessageW(
                hwnd,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(wide("Auto").as_ptr() as isize),
            );
            if selected != "Auto" {
                SendMessageW(
                    hwnd,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(wide(&selected).as_ptr() as isize),
                );
            }
            SendMessageW(
                hwnd,
                CB_SETCURSEL,
                WPARAM(if selected == "Auto" { 0 } else { 1 }),
                LPARAM(0),
            );
        }
    }

    pub fn load_dynamic(&mut self, settings: &Settings, area: bool) {
        let (base, p) = if area {
            (ID_DYN_AREA_FORMAT, &settings.dyn_area)
        } else {
            (ID_DYN_VIDEO_FORMAT, &settings.dyn_video)
        };
        let format_index = p.format.index();
        let video = p.video[format_index];
        if area {
            self.dyn_area_format_index = format_index;
        } else {
            self.dyn_video_format_index = format_index;
        }
        self.select(base + 9, preset_index(p.format, video));
        self.select(base + 3, resolution_index(video.height));
        unsafe {
            SendMessageW(
                self.get(base + 4),
                TBM_SETPOS,
                WPARAM(1),
                LPARAM(video.fps as isize),
            )
        };
        self.set_text(base + 5, &format!("{} FPS", video.fps));
        self.load_audio(base + 6, base + 7, base + 8, &p.audio);
    }

    pub fn check(&self, id: i32, checked: bool) {
        unsafe {
            SendMessageW(
                self.get(id),
                BM_SETCHECK,
                WPARAM(if checked {
                    BST_CHECKED.0 as usize
                } else {
                    BST_UNCHECKED.0 as usize
                }),
                LPARAM(0),
            )
        };
    }

    pub fn checked(&self, id: i32) -> bool {
        unsafe {
            SendMessageW(self.get(id), BM_GETCHECK, WPARAM(0), LPARAM(0)).0
                == BST_CHECKED.0 as isize
        }
    }

    pub fn select(&self, id: i32, index: usize) {
        unsafe { SendMessageW(self.get(id), CB_SETCURSEL, WPARAM(index), LPARAM(0)) };
    }

    pub fn selected(&self, id: i32) -> usize {
        unsafe { SendMessageW(self.get(id), CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize }
    }

    pub fn refresh_formats(&self) {
        for base in [ID_FORMAT, ID_DYN_AREA_FORMAT, ID_DYN_VIDEO_FORMAT] {
            for offset in 0..=2 {
                unsafe {
                    RedrawWindow(
                        self.get(base + offset),
                        None,
                        None,
                        RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW,
                    )
                    .ok();
                }
            }
        }
    }

    pub fn set_text(&self, id: i32, text: &str) {
        unsafe { SetWindowTextW(self.get(id), PCWSTR(wide(text).as_ptr())) }.ok();
    }

    pub fn set_history(&self, lines: &[String]) {
        for index in 0..HISTORY_COUNT {
            let line = lines.get(index as usize).map(String::as_str).unwrap_or("");
            self.set_text(ID_HISTORY + index, line);
        }
        self.sync_history();
    }

    fn sync_history(&self) {
        for index in 0..HISTORY_COUNT {
            let hwnd = self.get(ID_HISTORY + index);
            let mut probe = [0u16; 2];
            let filled = unsafe { GetWindowTextW(hwnd, &mut probe) } > 0;
            unsafe {
                ShowWindow(
                    hwnd,
                    if self.page == 0 && filled {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
        }
    }

    fn audio_from(
        &self,
        system: i32,
        mic: i32,
        device: i32,
        previous: &AudioSettings,
    ) -> AudioSettings {
        let selected = self.selected(device);
        let count =
            unsafe { SendMessageW(self.get(device), CB_GETCOUNT, WPARAM(0), LPARAM(0)).0 as usize };
        let device_id = if selected == 0 {
            "Auto".into()
        } else if count == 2 && previous.device != "Auto" {
            previous.device.clone()
        } else {
            media::device_at(selected - 1)
                .map(|d| d.id)
                .unwrap_or_else(|| previous.device.clone())
        };
        AudioSettings {
            system: self.checked(system),
            microphone: self.checked(mic),
            device: device_id,
        }
    }

    pub fn read_ui(&mut self, settings: &mut Settings) {
        let format = Format::from_index(self.format_index);
        settings.ui.format = format;
        settings.ui.video[format.index()] = VideoSettings {
            height: resolution_value(self.selected(ID_RESOLUTION)),
            fps: unsafe { SendMessageW(self.get(ID_FPS), WM_USER, WPARAM(0), LPARAM(0)).0 as u32 }
                .clamp(5, 60),
        };
        self.select(
            ID_PRESET,
            preset_index(format, settings.ui.video[format.index()]),
        );
        settings.ui.audio = self.audio_from(
            ID_SYSTEM_AUDIO,
            ID_MIC,
            ID_MIC_DEVICE,
            &settings.ui.audio.clone(),
        );
        settings.dyn_area = self.read_dynamic(settings, true);
        settings.dyn_video = self.read_dynamic(settings, false);
        settings.dyn_audio = self.audio_from(
            ID_DYN_AUDIO_SYSTEM,
            ID_DYN_AUDIO_MIC,
            ID_DYN_AUDIO_DEVICE,
            &settings.dyn_audio.clone(),
        );
        settings.autostart = self.checked(ID_AUTOSTART);
        settings.start_minimized = self.checked(ID_START_MINIMIZED);
        settings.hide_active = self.checked(ID_HIDE_ACTIVE);
        settings.close_tray = self.checked(ID_CLOSE_TRAY);
        settings.clipboard = self.checked(ID_CLIPBOARD);
        settings.tray_capture = self.checked(ID_TRAY_CAPTURE);
        settings.highlight_region = self.checked(ID_HIGHLIGHT);
        settings.completion_sound = self.checked(ID_NOTIFY);
        settings.threads = match self.selected(ID_THREADS) {
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 0,
        };
        settings.scale = scale_value(self.selected(ID_SCALE));
        settings.save();
        self.apply_scale(settings);
    }

    fn read_dynamic(&mut self, settings: &Settings, area: bool) -> CaptureSettings {
        let base = if area {
            ID_DYN_AREA_FORMAT
        } else {
            ID_DYN_VIDEO_FORMAT
        };
        let previous = if area {
            &settings.dyn_area
        } else {
            &settings.dyn_video
        };
        let format_index = if area {
            self.dyn_area_format_index
        } else {
            self.dyn_video_format_index
        };
        let format = Format::from_index(format_index);
        let fps =
            unsafe { SendMessageW(self.get(base + 4), WM_USER, WPARAM(0), LPARAM(0)).0 as u32 }
                .clamp(5, 60);
        let mut profile = previous.clone();
        profile.format = format;
        profile.video[format_index] = VideoSettings {
            height: resolution_value(self.selected(base + 3)),
            fps,
        };
        profile.audio = self.audio_from(base + 6, base + 7, base + 8, &previous.audio);
        self.select(base + 9, preset_index(format, profile.video[format_index]));
        profile
    }

    pub fn refresh_devices(&mut self, id: i32) {
        let hwnd = self.get(id);
        let devices = media::microphones();
        let current = unsafe { SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 } as isize;
        let previous = if current <= 0 {
            "Auto".to_string()
        } else {
            media::device_at((current - 1) as usize)
                .map(|d| d.id)
                .unwrap_or_else(|| "Auto".to_string())
        };
        unsafe {
            SendMessageW(hwnd, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
            SendMessageW(
                hwnd,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(wide("Auto").as_ptr() as isize),
            );
        }
        for device in &devices {
            unsafe {
                SendMessageW(
                    hwnd,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(wide(&device.name).as_ptr() as isize),
                )
            };
        }
        let selected = if previous == "Auto" {
            0
        } else {
            devices
                .iter()
                .position(|d| d.id == previous)
                .map(|i| i + 1)
                .unwrap_or(0)
        };
        unsafe { SendMessageW(hwnd, CB_SETCURSEL, WPARAM(selected), LPARAM(0)) };
    }

    pub fn refresh_all_devices(&mut self, settings: &mut Settings) {
        for id in [
            ID_MIC_DEVICE,
            ID_DYN_AREA_DEVICE,
            ID_DYN_VIDEO_DEVICE,
            ID_DYN_AUDIO_DEVICE,
        ] {
            self.refresh_devices(id);
        }
        self.read_ui(settings);
    }

    pub fn apply_preset(&mut self, settings: &mut Settings) {
        let format = Format::from_index(self.format_index);
        settings.ui.video[format.index()] = preset_value(format, self.selected(ID_PRESET));
        self.load_main_video(&settings.ui);
        self.read_ui(settings);
    }

    pub fn apply_dynamic_preset(&mut self, settings: &mut Settings, area: bool) {
        let base = if area {
            ID_DYN_AREA_FORMAT
        } else {
            ID_DYN_VIDEO_FORMAT
        };
        let format_index = if area {
            self.dyn_area_format_index
        } else {
            self.dyn_video_format_index
        };
        let value = preset_value(Format::from_index(format_index), self.selected(base + 9));
        if area {
            settings.dyn_area.video[format_index] = value;
        } else {
            settings.dyn_video.video[format_index] = value;
        }
        self.load_dynamic(settings, area);
        self.read_ui(settings);
    }

    pub fn set_idle(&self, idle: bool, recording: bool) {
        unsafe {
            EnableWindow(self.tab, idle);
            EnableWindow(self.settings_tab, idle);
            EnableWindow(self.get(ID_REFRESH), idle);
        }
        for h in self.capture.iter().chain(&self.settings) {
            if *h == self.get(ID_STOP) || *h == self.get(ID_STATUS) {
                continue;
            }
            let screenshot = *h == self.get(ID_SHOT_SCREEN) || *h == self.get(ID_SHOT_REGION);
            unsafe { EnableWindow(*h, idle || (recording && screenshot)) };
        }
        unsafe {
            EnableWindow(self.get(ID_STOP), recording);
            EnableWindow(self.get(ID_STATUS), true);
        }
    }
}

fn scaled_font(percent: u32) -> HFONT {
    unsafe {
        CreateFontW(
            -(17 * percent as i32 / 100),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            w!("Segoe UI"),
        )
    }
}

fn interface_scale(setting: u32) -> u32 {
    crate::geom::interface_scale(setting, media::cursor_monitor_rect().h)
}

fn scale_index(value: u32) -> usize {
    [0, 100, 125, 150, 175, 200, 225, 250, 300]
        .iter()
        .position(|v| *v == value)
        .unwrap_or(0)
}

fn scale_value(index: usize) -> u32 {
    [0, 100, 125, 150, 175, 200, 225, 250, 300]
        .get(index)
        .copied()
        .unwrap_or(0)
}

fn preset_index(format: Format, video: VideoSettings) -> usize {
    match (format, video.height, video.fps) {
        (Format::Mkv, 1080, 60) => 0,
        (Format::Mkv, 1080, 30) => 1,
        (Format::Mkv, 720, 30) => 2,
        (_, 720, 60) => 0,
        (_, 480, 30) => 1,
        (_, 360, 30) => 2,
        _ => usize::MAX,
    }
}

fn preset_value(format: Format, index: usize) -> VideoSettings {
    match (format, index) {
        (Format::Mkv, 0) => VideoSettings {
            height: 1080,
            fps: 60,
        },
        (Format::Mkv, 1) => VideoSettings {
            height: 1080,
            fps: 30,
        },
        (Format::Mkv, 2) => VideoSettings {
            height: 720,
            fps: 30,
        },
        (_, 0) => VideoSettings {
            height: 720,
            fps: 60,
        },
        (_, 1) => VideoSettings {
            height: 480,
            fps: 30,
        },
        _ => VideoSettings {
            height: 360,
            fps: 30,
        },
    }
}

fn resolution_index(height: u32) -> usize {
    [2160, 1440, 1080, 720, 480, 360]
        .iter()
        .position(|v| *v == height)
        .unwrap_or(3)
}

fn resolution_value(index: usize) -> u32 {
    [2160, 1440, 1080, 720, 480, 360]
        .get(index)
        .copied()
        .unwrap_or(720)
}

pub fn draw_button(lparam: LPARAM, controls: &Controls) {
    unsafe {
        let item = &mut *(lparam.0 as *mut DRAWITEMSTRUCT);
        let id = item.CtlID as i32;
        let radio = if (ID_FORMAT..=ID_FORMAT + 2).contains(&id) {
            Some((ID_FORMAT, controls.format_index))
        } else if (ID_DYN_AREA_FORMAT..=ID_DYN_AREA_FORMAT + 2).contains(&id) {
            Some((ID_DYN_AREA_FORMAT, controls.dyn_area_format_index))
        } else if (ID_DYN_VIDEO_FORMAT..=ID_DYN_VIDEO_FORMAT + 2).contains(&id) {
            Some((ID_DYN_VIDEO_FORMAT, controls.dyn_video_format_index))
        } else {
            None
        };
        if let Some((base, format_index)) = radio {
            let brush = CreateSolidBrush(BACKGROUND);
            FillRect(item.hDC, &item.rcItem, brush);
            DeleteObject(brush);
            let radius = 8 * controls.scale_percent as i32 / 100;
            let x = item.rcItem.left + 6 * controls.scale_percent as i32 / 100 + radius;
            let y = (item.rcItem.top + item.rcItem.bottom) / 2;
            let outer = CreateSolidBrush(ACCENT);
            SelectObject(item.hDC, outer);
            Ellipse(item.hDC, x - radius, y - radius, x + radius, y + radius);
            DeleteObject(outer);
            if format_index == (id - base) as usize {
                let inner_radius = (radius / 2).max(2);
                let inner = CreateSolidBrush(TEXT);
                SelectObject(item.hDC, inner);
                Ellipse(
                    item.hDC,
                    x - inner_radius,
                    y - inner_radius,
                    x + inner_radius,
                    y + inner_radius,
                );
                DeleteObject(inner);
            }
            let mut text = vec![0u16; 256];
            let len = GetWindowTextW(item.hwndItem, &mut text) as usize;
            text.truncate(len);
            let mut rect = item.rcItem;
            rect.left = x + radius + 8 * controls.scale_percent as i32 / 100;
            SetBkMode(item.hDC, TRANSPARENT);
            SetTextColor(item.hDC, TEXT);
            let old = SelectObject(item.hDC, controls.font);
            DrawTextW(
                item.hDC,
                &mut text,
                &mut rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            SelectObject(item.hDC, old);
            return;
        }
        let active_tab =
            (id == ID_TAB && controls.page == 0) || (id == ID_SETTINGS_TAB && controls.page == 1);
        let enabled = IsWindowEnabled(item.hwndItem).as_bool();
        let color = if !enabled {
            DISABLED
        } else if id == ID_STOP {
            ACCENT_DEEP
        } else if id == ID_TAB || id == ID_SETTINGS_TAB {
            if active_tab {
                ACCENT
            } else {
                MIST
            }
        } else {
            ACCENT
        };
        let parent = CreateSolidBrush(BACKGROUND);
        FillRect(item.hDC, &item.rcItem, parent);
        DeleteObject(parent);
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, color);
        let old_brush = SelectObject(item.hDC, brush);
        let old_pen = SelectObject(item.hDC, pen);
        let radius = (12 * controls.scale_percent as i32 / 100).max(4);
        RoundRect(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            item.rcItem.right,
            item.rcItem.bottom,
            radius,
            radius,
        );
        SelectObject(item.hDC, old_brush);
        SelectObject(item.hDC, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);
        let mut text = vec![0u16; 256];
        let len = GetWindowTextW(item.hwndItem, &mut text) as usize;
        text.truncate(len);
        SetBkMode(item.hDC, TRANSPARENT);
        let on_light = !enabled || ((id == ID_TAB || id == ID_SETTINGS_TAB) && !active_tab);
        SetTextColor(
            item.hDC,
            if on_light {
                TEXT
            } else {
                crate::theme::ON_ACCENT
            },
        );
        let old = SelectObject(item.hDC, controls.font);
        DrawTextW(
            item.hDC,
            &mut text,
            &mut item.rcItem,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(item.hDC, old);
    }
}
