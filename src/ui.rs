use crate::{media, wide, AudioSettings, CaptureSettings, ProfileRef, Settings};
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
pub const ID_SHOT_SOURCE: i32 = 147;
pub const ID_SHOT_REGION: i32 = 131;
pub const ID_CAPTURE_SCREEN: i32 = 132;
pub const ID_CAPTURE_SOURCE: i32 = 148;
pub const ID_CAPTURE_REGION: i32 = 133;
pub const ID_CAPTURE_AUDIO: i32 = 134;
pub const ID_STOP: i32 = 135;
pub const ID_STATUS: i32 = 136;
pub const ID_HISTORY: i32 = 210;
pub const ID_HISTORY_COPY: i32 = 230;
pub const ID_HISTORY_REVEAL: i32 = 250;
pub const ID_HISTORY_OPEN: i32 = 270;
pub const HISTORY_ACK_TIMER: usize = 2;
pub const HISTORY_COUNT: i32 = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HistoryClick {
    Label,
    CopyFile,
    Reveal,
    Open,
}

pub fn history_id(id: i32) -> bool {
    (ID_HISTORY..ID_HISTORY + HISTORY_COUNT).contains(&id)
}

pub fn is_mic_gain(id: i32) -> bool {
    matches!(
        id,
        ID_MIC_GAIN | ID_TRAY_AREA_GAIN | ID_TRAY_VIDEO_GAIN | ID_TRAY_AUDIO_GAIN
    )
}

pub fn is_mic_gain_value(id: i32) -> bool {
    matches!(
        id,
        ID_MIC_GAIN_VALUE
            | ID_TRAY_AREA_GAIN_VALUE
            | ID_TRAY_VIDEO_GAIN_VALUE
            | ID_TRAY_AUDIO_GAIN_VALUE
    )
}

pub fn microphone_control(source: ProfileRef) -> i32 {
    match source {
        ProfileRef::Capture => ID_MIC_GAIN,
        ProfileRef::Area => ID_TRAY_AREA_GAIN,
        ProfileRef::Video => ID_TRAY_VIDEO_GAIN,
        ProfileRef::Audio => ID_TRAY_AUDIO_GAIN,
    }
}

pub fn profile_for_microphone(id: i32) -> Option<ProfileRef> {
    match id {
        ID_MIC_GAIN => Some(ProfileRef::Capture),
        ID_TRAY_AREA_GAIN => Some(ProfileRef::Area),
        ID_TRAY_VIDEO_GAIN => Some(ProfileRef::Video),
        ID_TRAY_AUDIO_GAIN => Some(ProfileRef::Audio),
        _ => None,
    }
}

pub fn gain_value_id(id: i32) -> i32 {
    match id {
        ID_MIC_GAIN => ID_MIC_GAIN_VALUE,
        ID_TRAY_AREA_GAIN => ID_TRAY_AREA_GAIN_VALUE,
        ID_TRAY_VIDEO_GAIN => ID_TRAY_VIDEO_GAIN_VALUE,
        ID_TRAY_AUDIO_GAIN => ID_TRAY_AUDIO_GAIN_VALUE,
        _ => ID_MIC_GAIN_VALUE,
    }
}

pub fn history_click(id: i32) -> Option<(usize, HistoryClick)> {
    let index = |base| {
        let index = id - base;
        (0..HISTORY_COUNT)
            .contains(&index)
            .then_some(index as usize)
    };
    if let Some(index) = index(ID_HISTORY) {
        Some((index, HistoryClick::Label))
    } else if let Some(index) = index(ID_HISTORY_COPY) {
        Some((index, HistoryClick::CopyFile))
    } else if let Some(index) = index(ID_HISTORY_REVEAL) {
        Some((index, HistoryClick::Reveal))
    } else if let Some(index) = index(ID_HISTORY_OPEN) {
        Some((index, HistoryClick::Open))
    } else {
        None
    }
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
pub const ID_NO_SIGNAL: i32 = 158;
pub const ID_CARD: i32 = 159;
pub const ID_TRAY_AREA_FORMAT: i32 = 160;
pub const ID_TRAY_AREA_FPS_VALUE: i32 = 165;
pub const ID_TRAY_AREA_DEVICE: i32 = 168;
pub const ID_TRAY_AREA_PRESET: i32 = 169;
pub const ID_TRAY_VIDEO_FORMAT: i32 = 170;
pub const ID_TRAY_VIDEO_FPS_VALUE: i32 = 175;
pub const ID_TRAY_VIDEO_DEVICE: i32 = 178;
pub const ID_TRAY_VIDEO_PRESET: i32 = 179;
pub const ID_THREADS: i32 = 180;
pub const ID_AREA_HEADING: i32 = 181;
pub const ID_VIDEO_HEADING: i32 = 182;
pub const ID_THREADS_HEADING: i32 = 183;
pub const ID_SCALE_HEADING: i32 = 184;
pub const ID_SCALE: i32 = 185;
pub const ID_CURSOR: i32 = 186;
pub const ID_TRAY_AREA_CURSOR: i32 = 187;
pub const ID_TRAY_VIDEO_CURSOR: i32 = 188;
pub const ID_TRAY_AUDIO_SYSTEM: i32 = 190;
pub const ID_TRAY_AUDIO_MIC: i32 = 191;
pub const ID_TRAY_AUDIO_DEVICE: i32 = 192;
pub const ID_TRAY_AUDIO_HEADING: i32 = 193;
pub const ID_MIC_GAIN: i32 = 194;
pub const ID_TRAY_AREA_GAIN: i32 = 195;
pub const ID_TRAY_VIDEO_GAIN: i32 = 196;
pub const ID_TRAY_AUDIO_GAIN: i32 = 197;
pub const ID_MIC_GAIN_VALUE: i32 = 198;
pub const ID_TRAY_AREA_GAIN_VALUE: i32 = 199;
pub const ID_TRAY_VIDEO_GAIN_VALUE: i32 = 207;
pub const ID_TRAY_AUDIO_GAIN_VALUE: i32 = 208;
pub const MIC_GAIN_MAX: u32 = 300;
const PRESET_ITEMS: &[&str] = &["Quality", "Balance", "Speed", "Custom"];
const PRESET_CUSTOM: usize = 3;
pub use crate::theme::{ACCENT, ACCENT_DEEP, BACKGROUND, DISABLED, MIST, TEXT};

const WINDOW_WIDTH: i32 = 760;
/// Tallest page (Settings) plus its bottom margin. Both pages use it, so switching
/// tabs never changes the window height and new settings never grow the window.
const WINDOW_HEIGHT: i32 = 1181;

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
    p(ID_REFRESH, 592, 10, 100, 32),
    p(ID_ABOUT, 700, 10, 32, 32),
];
const CAPTURE: [Place; 26] = [
    p(ID_FORMAT, 28, 58, 228, 26),
    p(ID_FORMAT + 1, 264, 58, 228, 26),
    p(ID_FORMAT + 2, 500, 58, 228, 26),
    p(ID_PRESET, 28, 96, 346, 300),
    p(ID_RESOLUTION, 386, 96, 346, 300),
    p(ID_FPS, 28, 136, 616, 28),
    p(ID_FPS_VALUE, 652, 140, 80, 24),
    p(ID_SYSTEM_AUDIO, 28, 178, 704, 26),
    p(ID_MIC, 28, 208, 160, 26),
    p(ID_MIC_DEVICE, 196, 207, 260, 300),
    p(ID_MIC_GAIN, 464, 206, 180, 28),
    p(ID_MIC_GAIN_VALUE, 652, 207, 80, 26),
    p(ID_CURSOR, 28, 244, 704, 26),
    p(ID_SHOT_LABEL, 28, 284, 704, 22),
    p(ID_SHOT_SCREEN, 28, 310, 228, 36),
    p(ID_SHOT_SOURCE, 264, 310, 228, 36),
    p(ID_SHOT_REGION, 500, 310, 228, 36),
    p(ID_CAPTURE_LABEL, 28, 358, 704, 22),
    p(ID_CAPTURE_SCREEN, 28, 384, 228, 36),
    p(ID_CAPTURE_SOURCE, 264, 384, 228, 36),
    p(ID_CAPTURE_REGION, 500, 384, 228, 36),
    p(ID_AUDIO_LABEL, 28, 432, 704, 22),
    p(ID_CAPTURE_AUDIO, 28, 458, 704, 36),
    p(ID_STOP_LABEL, 28, 506, 704, 22),
    p(ID_STOP, 28, 532, 704, 36),
    p(ID_STATUS, 28, 584, 704, 28),
];

const SETTINGS: [Place; 48] = [
    p(ID_AUTOSTART, 28, 56, 704, 26),
    p(ID_START_MINIMIZED, 28, 87, 704, 26),
    p(ID_HIDE_ACTIVE, 28, 118, 704, 26),
    p(ID_CLOSE_TRAY, 28, 149, 704, 26),
    p(ID_CLIPBOARD, 28, 180, 704, 26),
    p(ID_TRAY_CAPTURE, 28, 211, 704, 26),
    p(ID_HIGHLIGHT, 28, 255, 704, 26),
    p(ID_NOTIFY, 28, 286, 704, 26),
    p(ID_NO_SIGNAL, 28, 317, 704, 26),
    p(ID_CARD, 28, 348, 704, 26),
    p(ID_AREA_HEADING, 28, 434, 704, 24),
    p(ID_TRAY_AREA_FORMAT, 28, 460, 228, 26),
    p(ID_TRAY_AREA_FORMAT + 1, 264, 460, 228, 26),
    p(ID_TRAY_AREA_FORMAT + 2, 500, 460, 228, 26),
    p(ID_TRAY_AREA_PRESET, 28, 498, 346, 300),
    p(ID_TRAY_AREA_FORMAT + 3, 386, 498, 346, 300),
    p(ID_TRAY_AREA_FORMAT + 4, 28, 536, 616, 28),
    p(ID_TRAY_AREA_FPS_VALUE, 652, 540, 80, 24),
    p(ID_TRAY_AREA_FORMAT + 6, 28, 576, 704, 26),
    p(ID_TRAY_AREA_FORMAT + 7, 28, 608, 160, 26),
    p(ID_TRAY_AREA_DEVICE, 196, 607, 260, 300),
    p(ID_TRAY_AREA_GAIN, 464, 606, 180, 28),
    p(ID_TRAY_AREA_GAIN_VALUE, 652, 607, 80, 26),
    p(ID_TRAY_AREA_CURSOR, 28, 648, 704, 26),
    p(ID_VIDEO_HEADING, 28, 696, 704, 24),
    p(ID_TRAY_VIDEO_FORMAT, 28, 722, 704, 26),
    p(ID_TRAY_VIDEO_FORMAT + 1, 264, 722, 228, 26),
    p(ID_TRAY_VIDEO_FORMAT + 2, 500, 722, 228, 26),
    p(ID_TRAY_VIDEO_PRESET, 28, 760, 346, 300),
    p(ID_TRAY_VIDEO_FORMAT + 3, 386, 760, 346, 300),
    p(ID_TRAY_VIDEO_FORMAT + 4, 28, 798, 616, 28),
    p(ID_TRAY_VIDEO_FPS_VALUE, 652, 802, 80, 24),
    p(ID_TRAY_VIDEO_FORMAT + 6, 28, 838, 704, 26),
    p(ID_TRAY_VIDEO_FORMAT + 7, 28, 870, 160, 26),
    p(ID_TRAY_VIDEO_DEVICE, 196, 869, 260, 300),
    p(ID_TRAY_VIDEO_GAIN, 464, 868, 180, 28),
    p(ID_TRAY_VIDEO_GAIN_VALUE, 652, 869, 80, 26),
    p(ID_TRAY_VIDEO_CURSOR, 28, 908, 704, 26),
    p(ID_TRAY_AUDIO_HEADING, 28, 958, 704, 24),
    p(ID_TRAY_AUDIO_SYSTEM, 28, 984, 704, 26),
    p(ID_TRAY_AUDIO_MIC, 28, 1016, 160, 26),
    p(ID_TRAY_AUDIO_DEVICE, 196, 1015, 260, 300),
    p(ID_TRAY_AUDIO_GAIN, 464, 1014, 180, 28),
    p(ID_TRAY_AUDIO_GAIN_VALUE, 652, 1015, 80, 26),
    p(ID_THREADS_HEADING, 28, 1076, 346, 24),
    p(ID_THREADS, 28, 1102, 346, 300),
    p(ID_SCALE_HEADING, 386, 1076, 346, 24),
    p(ID_SCALE, 386, 1102, 346, 300),
];

pub struct Controls {
    pub tab: HWND,
    pub settings_tab: HWND,
    pub capture: Vec<HWND>,
    pub settings: Vec<HWND>,
    pub values: Vec<(i32, HWND)>,
    pub page: usize,
    pub font: HFONT,
    bold: HFONT,
    pub format_index: usize,
    pub tray_area_format_index: usize,
    pub tray_video_format_index: usize,
    hwnd: HWND,
    scale_percent: u32,
    scale_setting: u32,
    history_ack: Option<i32>,
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
            bold: HFONT::default(),
            format_index: 0,
            tray_area_format_index: 1,
            tray_video_format_index: 0,
            hwnd,
            scale_percent: 100,
            scale_setting: u32::MAX,
            history_ack: None,
        }
    }

    pub fn acknowledge_copy(&mut self, id: i32) {
        self.history_ack = Some(id);
        unsafe {
            // SAFETY: The HWND is the copy button this window created. The timer id is cleared from WM_TIMER.
            InvalidateRect(self.get(id), None, true);
            SetTimer(self.hwnd, HISTORY_ACK_TIMER, 800, None);
        }
    }

    pub fn clear_history_ack(&mut self) {
        let Some(id) = self.history_ack.take() else {
            return;
        };
        unsafe {
            // SAFETY: The HWND is the copy button this window created.
            InvalidateRect(self.get(id), None, true);
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
            // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

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
            // SAFETY: The HWND is a control this program created. The theme names are literals.

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
        self.font = scaled_font(self.scale_percent, FW_NORMAL.0 as i32);
        self.bold = scaled_font(self.scale_percent, FW_BOLD.0 as i32);
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
        self.combo(ID_PRESET, PRESET_ITEMS, 0);
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
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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
        self.mic_gain_control(ID_MIC_GAIN, ID_MIC_GAIN_VALUE, 0);
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
            (ID_SHOT_SOURCE, "Sources"),
            (ID_SHOT_REGION, "Area"),
            (ID_CAPTURE_SCREEN, "Screen"),
            (ID_CAPTURE_SOURCE, "Sources"),
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
            self.create_control(
                w!("BUTTON"),
                "",
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                WINDOW_EX_STYLE(0),
                ID_HISTORY_COPY + index,
                2,
            );
            self.create_control(
                w!("BUTTON"),
                "",
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                WINDOW_EX_STYLE(0),
                ID_HISTORY_REVEAL + index,
                2,
            );
            self.create_control(
                w!("BUTTON"),
                "",
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                WINDOW_EX_STYLE(0),
                ID_HISTORY_OPEN + index,
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
            (
                ID_NO_SIGNAL,
                "Keep recording with a no-signal card if the source disappears",
            ),
            (ID_CARD, "Show a no-signal card instead of the last frame"),
            (ID_CURSOR, "Include pointer"),
            (ID_TRAY_AREA_CURSOR, "Include pointer"),
            (ID_TRAY_VIDEO_CURSOR, "Include pointer"),
        ] {
            let page = if id == ID_CURSOR { 0 } else { 1 };
            self.create_control(
                w!("BUTTON"),
                text,
                WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                WINDOW_EX_STYLE(0),
                id,
                page,
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
            ID_TRAY_AUDIO_HEADING,
            1,
        );
        self.create_control(
            w!("BUTTON"),
            "System audio",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            ID_TRAY_AUDIO_SYSTEM,
            1,
        );
        self.create_control(
            w!("BUTTON"),
            "Microphone",
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            ID_TRAY_AUDIO_MIC,
            1,
        );
        self.combo(ID_TRAY_AUDIO_DEVICE, &["Auto"], 1);
        self.mic_gain_control(ID_TRAY_AUDIO_GAIN, ID_TRAY_AUDIO_GAIN_VALUE, 1);
        self.tray_controls(true);
        self.tray_controls(false);
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
                // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

                SendMessageW(
                    hwnd,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(item.as_ptr() as isize),
                )
            };
        }
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
            SendMessageW(hwnd, CB_SETCURSEL, WPARAM(0), LPARAM(0))
        };
    }

    fn tray_controls(&mut self, area: bool) {
        let base = if area {
            ID_TRAY_AREA_FORMAT
        } else {
            ID_TRAY_VIDEO_FORMAT
        };
        if area {
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
        } else {
            self.create_control(
                w!("STATIC"),
                "Format: MKV",
                WINDOW_STYLE(0),
                WINDOW_EX_STYLE(0),
                base,
                1,
            );
        }
        self.combo(base + 9, PRESET_ITEMS, 1);
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
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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
        let (gain, value) = if area {
            (ID_TRAY_AREA_GAIN, ID_TRAY_AREA_GAIN_VALUE)
        } else {
            (ID_TRAY_VIDEO_GAIN, ID_TRAY_VIDEO_GAIN_VALUE)
        };
        self.mic_gain_control(gain, value, 1);
    }

    fn mic_gain_control(&mut self, slider: i32, value: i32, page: usize) {
        let trackbar = self.create_control(
            TRACKBAR_CLASS,
            "",
            WINDOW_STYLE(TBS_HORZ | TBS_AUTOTICKS),
            WINDOW_EX_STYLE(0),
            slider,
            page,
        );
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
            SendMessageW(
                trackbar,
                TBM_SETRANGE,
                WPARAM(1),
                LPARAM(((MIC_GAIN_MAX as i32) << 16) as isize),
            );
            SendMessageW(trackbar, TBM_SETTICFREQ, WPARAM(25), LPARAM(0));
        }
        self.create_control(
            w!("STATIC"),
            "Mic 100%",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            value,
            page,
        );
    }

    pub fn show_page(&mut self, page: usize) {
        self.page = page;
        unsafe {
            // SAFETY: The HWND is a window or child control this program created and still owns.

            ShowWindow(self.tab, SW_SHOW);
            ShowWindow(self.settings_tab, SW_SHOW);
            ShowWindow(self.get(ID_REFRESH), SW_SHOW);
            ShowWindow(self.get(ID_ABOUT), SW_SHOW)
        };
        for h in &self.capture {
            unsafe {
                // SAFETY: The HWND is a window or child control this program created and still owns.
                ShowWindow(*h, if page == 0 { SW_SHOW } else { SW_HIDE })
            };
        }
        for h in &self.settings {
            unsafe {
                // SAFETY: The HWND is a window or child control this program created and still owns.
                ShowWindow(*h, if page == 1 { SW_SHOW } else { SW_HIDE })
            };
        }
        self.layout();
        self.resize_window();
        self.refresh_formats();
        unsafe {
            // SAFETY: The HWND is a window this program created.
            InvalidateRect(self.hwnd, None, true)
        };
        // The window clips children, so invalidating it leaves the tab buttons
        // showing the previous page. They have to be redrawn on their own.
        for hwnd in [self.tab, self.settings_tab] {
            unsafe {
                // SAFETY: Both HWNDs are the tab buttons this window created.
                RedrawWindow(hwnd, None, None, RDW_INVALIDATE | RDW_UPDATENOW).ok();
            }
        }
    }

    fn place(&self, place: &Place) {
        self.place_at(place, place.y * self.scale_percent as i32 / 100);
    }

    fn place_at(&self, place: &Place, y: i32) {
        let scale = |value: i32| value * self.scale_percent as i32 / 100;
        unsafe {
            // SAFETY: The HWND is a child control of the main window.

            MoveWindow(
                self.get(place.id),
                scale(place.x),
                y,
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
                let y = 620 + index * 28;
                self.place(&p(ID_HISTORY + index, 28, y, 608, 24));
                self.place(&p(ID_HISTORY_OPEN + index, 644, y, 24, 24));
                self.place(&p(ID_HISTORY_COPY + index, 676, y, 24, 24));
                self.place(&p(ID_HISTORY_REVEAL + index, 708, y, 24, 24));
            }
        } else {
            for place in &SETTINGS {
                self.place(place);
            }
        }
        self.sync_history();
    }

    /// `100` is the list entry labeled "1". Auto stays `0`.
    pub fn set_interface_scale(&mut self, settings: &mut Settings, scale: u32) {
        settings.scale = scale;
        self.select(ID_SCALE, scale_index(scale));
        self.apply_scale(settings);
        settings.save();
    }

    pub fn apply_scale(&mut self, settings: &Settings) {
        let percent = interface_scale(settings.scale);
        if self.scale_percent == percent && self.scale_setting == settings.scale {
            return;
        }
        self.scale_percent = percent;
        self.scale_setting = settings.scale;
        if !self.font.is_invalid() {
            unsafe {
                // SAFETY: This font was created for the window and is no longer selected into a DC.
                DeleteObject(self.font)
            };
        }
        if !self.bold.is_invalid() {
            unsafe {
                // SAFETY: This font was created for the window and is no longer selected into a DC.
                DeleteObject(self.bold)
            };
        }
        self.font = scaled_font(percent, FW_NORMAL.0 as i32);
        self.bold = scaled_font(percent, FW_BOLD.0 as i32);
        for (_, hwnd) in &self.values {
            unsafe {
                // SAFETY: The HWND is a child control of the main window, and the font stays alive for that window.
                SendMessageW(*hwnd, WM_SETFONT, WPARAM(self.font.0 as usize), LPARAM(1))
            };
        }
        self.layout();
        self.resize_window();
    }

    fn resize_window(&self) {
        let percent = self.scale_percent;
        let width = WINDOW_WIDTH * percent as i32 / 100;
        let height = WINDOW_HEIGHT * percent as i32 / 100;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        unsafe {
            // SAFETY: The HWND belongs to a window this process created. The RECT is a writable local.

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
        self.load_audio(
            ID_SYSTEM_AUDIO,
            ID_MIC,
            ID_MIC_DEVICE,
            ID_MIC_GAIN,
            &settings.ui.audio,
        );
        self.load_tray(settings, true);
        self.load_tray(settings, false);
        self.load_audio(
            ID_TRAY_AUDIO_SYSTEM,
            ID_TRAY_AUDIO_MIC,
            ID_TRAY_AUDIO_DEVICE,
            ID_TRAY_AUDIO_GAIN,
            &settings.tray_audio,
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
            (ID_NO_SIGNAL, settings.show_no_signal),
            (ID_CARD, settings.show_card),
            (ID_CURSOR, settings.ui.cursor),
            (ID_TRAY_AREA_CURSOR, settings.tray_area.cursor),
            (ID_TRAY_VIDEO_CURSOR, settings.tray_video.cursor),
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

    pub fn set_microphone_gain(&mut self, id: i32, gain: u32) {
        let gain = gain.min(MIC_GAIN_MAX);
        unsafe {
            // SAFETY: The target is a trackbar this program created. The position is a plain integer.
            SendMessageW(self.get(id), TBM_SETPOS, WPARAM(1), LPARAM(gain as isize));
        }
        self.set_text(gain_value_id(id), &format!("Mic {gain}%"));
    }

    pub fn microphone_gain(&self, id: i32) -> u32 {
        unsafe {
            // SAFETY: The target is a trackbar this program created. TBM_GETPOS returns a plain integer.
            (SendMessageW(self.get(id), WM_USER, WPARAM(0), LPARAM(0)).0 as u32).min(MIC_GAIN_MAX)
        }
    }

    pub fn load_main_video(&mut self, settings: &CaptureSettings) {
        let v = settings.video[settings.format.index()];
        self.select(ID_RESOLUTION, resolution_index(v.height));
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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

    fn load_audio(&mut self, system: i32, mic: i32, device: i32, gain: i32, audio: &AudioSettings) {
        self.check(system, audio.system);
        self.check(mic, audio.microphone);
        self.set_microphone_gain(gain, audio.gain);
        let hwnd = self.get(device);
        let selected = if audio.device == "Auto" {
            "Auto".to_string()
        } else {
            media::device_name(&audio.device).unwrap_or_else(|| "Auto".to_string())
        };
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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

    pub fn load_tray(&mut self, settings: &Settings, area: bool) {
        let (base, p) = if area {
            (ID_TRAY_AREA_FORMAT, &settings.tray_area)
        } else {
            (ID_TRAY_VIDEO_FORMAT, &settings.tray_video)
        };
        let format_index = if area {
            p.format.index()
        } else {
            Format::Mkv.index()
        };
        let video = p.video[format_index];
        if area {
            self.tray_area_format_index = format_index;
        } else {
            self.tray_video_format_index = Format::Mkv.index();
        }
        let format = if area { p.format } else { Format::Mkv };
        self.select(base + 9, preset_index(format, video));
        self.select(base + 3, resolution_index(video.height));
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

            SendMessageW(
                self.get(base + 4),
                TBM_SETPOS,
                WPARAM(1),
                LPARAM(video.fps as isize),
            )
        };
        self.set_text(base + 5, &format!("{} FPS", video.fps));
        let gain = if area {
            ID_TRAY_AREA_GAIN
        } else {
            ID_TRAY_VIDEO_GAIN
        };
        self.load_audio(base + 6, base + 7, base + 8, gain, &p.audio);
    }

    pub fn check(&self, id: i32, checked: bool) {
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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
            // SAFETY: The HWND is a checkbox this window created. The message takes no pointer.
            SendMessageW(self.get(id), BM_GETCHECK, WPARAM(0), LPARAM(0)).0
                == BST_CHECKED.0 as isize
        }
    }

    pub fn select(&self, id: i32, index: usize) {
        unsafe {
            // SAFETY: The HWND is a child control of the main window.
            SendMessageW(self.get(id), CB_SETCURSEL, WPARAM(index), LPARAM(0))
        };
    }

    pub fn selected(&self, id: i32) -> usize {
        unsafe {
            // SAFETY: The HWND is a child control of the main window.
            SendMessageW(self.get(id), CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize
        }
    }

    pub fn refresh_formats(&self) {
        for base in [ID_FORMAT, ID_TRAY_AREA_FORMAT, ID_TRAY_VIDEO_FORMAT] {
            for offset in 0..=2 {
                unsafe {
                    // SAFETY: The HWND is a child control of the main window.

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
        unsafe {
            // SAFETY: The HWND is a child control, and the text is NUL-terminated until the call returns.
            SetWindowTextW(self.get(id), PCWSTR(wide(text).as_ptr()))
        }
        .ok();
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
            let filled = unsafe {
                // SAFETY: The HWND is a window or child control this program created and still owns.
                GetWindowTextW(hwnd, &mut probe)
            } > 0;
            let show = if self.page == 0 && filled {
                SW_SHOW
            } else {
                SW_HIDE
            };
            for id in [
                ID_HISTORY + index,
                ID_HISTORY_OPEN + index,
                ID_HISTORY_COPY + index,
                ID_HISTORY_REVEAL + index,
            ] {
                unsafe {
                    // SAFETY: The HWND is a window or child control this program created and still owns.
                    ShowWindow(self.get(id), show);
                }
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
        let count = unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
            SendMessageW(self.get(device), CB_GETCOUNT, WPARAM(0), LPARAM(0)).0 as usize
        };
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
            gain: previous.gain,
        }
    }

    pub fn read_ui(&mut self, settings: &mut Settings) {
        let format = Format::from_index(self.format_index);
        settings.ui.format = format;
        settings.ui.video[format.index()] = VideoSettings {
            height: resolution_value(self.selected(ID_RESOLUTION)),
            fps: unsafe {
                // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
                SendMessageW(self.get(ID_FPS), WM_USER, WPARAM(0), LPARAM(0)).0 as u32
            }
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
        settings.ui.cursor = self.checked(ID_CURSOR);
        settings.tray_area = self.read_tray(settings, true);
        settings.tray_video = self.read_tray(settings, false);
        settings.tray_audio = self.audio_from(
            ID_TRAY_AUDIO_SYSTEM,
            ID_TRAY_AUDIO_MIC,
            ID_TRAY_AUDIO_DEVICE,
            &settings.tray_audio.clone(),
        );
        settings.autostart = self.checked(ID_AUTOSTART);
        settings.start_minimized = self.checked(ID_START_MINIMIZED);
        settings.hide_active = self.checked(ID_HIDE_ACTIVE);
        settings.show_no_signal = self.checked(ID_NO_SIGNAL);
        settings.show_card = self.checked(ID_CARD);
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

    fn read_tray(&mut self, settings: &Settings, area: bool) -> CaptureSettings {
        let base = if area {
            ID_TRAY_AREA_FORMAT
        } else {
            ID_TRAY_VIDEO_FORMAT
        };
        let previous = if area {
            &settings.tray_area
        } else {
            &settings.tray_video
        };
        let format_index = if area {
            self.tray_area_format_index
        } else {
            Format::Mkv.index()
        };
        let format = Format::from_index(format_index);
        let fps = unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
            SendMessageW(self.get(base + 4), WM_USER, WPARAM(0), LPARAM(0)).0 as u32
        }
        .clamp(5, 60);
        let video = VideoSettings {
            height: resolution_value(self.selected(base + 3)),
            fps,
        };
        let mut profile = previous.clone();
        profile.format = if area { format } else { Format::Mkv };
        profile.video[format_index] = video;
        profile.audio = self.audio_from(base + 6, base + 7, base + 8, &previous.audio);
        profile.cursor = self.checked(if area {
            ID_TRAY_AREA_CURSOR
        } else {
            ID_TRAY_VIDEO_CURSOR
        });
        self.select(base + 9, preset_index(format, video));
        profile
    }

    pub fn refresh_devices(&mut self, id: i32) {
        let hwnd = self.get(id);
        let devices = media::microphones();
        let current = unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
            SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0
        } as isize;
        let previous = if current <= 0 {
            "Auto".to_string()
        } else {
            media::device_at((current - 1) as usize)
                .map(|d| d.id)
                .unwrap_or_else(|| "Auto".to_string())
        };
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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
                // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

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
        unsafe {
            // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.
            SendMessageW(hwnd, CB_SETCURSEL, WPARAM(selected), LPARAM(0))
        };
    }

    pub fn refresh_all_devices(&mut self, settings: &mut Settings) {
        for id in [
            ID_MIC_DEVICE,
            ID_TRAY_AREA_DEVICE,
            ID_TRAY_VIDEO_DEVICE,
            ID_TRAY_AUDIO_DEVICE,
        ] {
            self.refresh_devices(id);
        }
        self.read_ui(settings);
    }

    pub fn apply_preset(&mut self, settings: &mut Settings) {
        let format = Format::from_index(self.format_index);
        if self.selected(ID_PRESET) == PRESET_CUSTOM {
            self.select(
                ID_PRESET,
                preset_index(format, settings.ui.video[format.index()]),
            );
            return;
        }
        settings.ui.video[format.index()] = preset_value(format, self.selected(ID_PRESET));
        self.load_main_video(&settings.ui);
        self.read_ui(settings);
    }

    pub fn apply_tray_preset(&mut self, settings: &mut Settings, area: bool) {
        let base = if area {
            ID_TRAY_AREA_FORMAT
        } else {
            ID_TRAY_VIDEO_FORMAT
        };
        let format_index = if area {
            self.tray_area_format_index
        } else {
            self.tray_video_format_index
        };
        let format = Format::from_index(format_index);
        let video = if area {
            settings.tray_area.video[format_index]
        } else {
            settings.tray_video.video[format_index]
        };
        if self.selected(base + 9) == PRESET_CUSTOM {
            self.select(base + 9, preset_index(format, video));
            return;
        }
        let value = preset_value(format, self.selected(base + 9));
        if area {
            settings.tray_area.video[format_index] = value;
        } else {
            settings.tray_video.video[format_index] = value;
        }
        self.load_tray(settings, area);
        self.read_ui(settings);
    }

    pub fn set_idle(&self, idle: bool, recording: bool) {
        unsafe {
            // SAFETY: The HWND is a child control created with the main window.

            // Both pages stay reachable while recording so every microphone volume
            // control can be used without stopping the take.
            EnableWindow(self.tab, idle || recording);
            EnableWindow(self.settings_tab, idle || recording);
            EnableWindow(self.get(ID_REFRESH), idle);
        }
        for h in self.capture.iter().chain(&self.settings) {
            if *h == self.get(ID_STOP) || *h == self.get(ID_STATUS) {
                continue;
            }
            let screenshot = *h == self.get(ID_SHOT_SCREEN)
                || *h == self.get(ID_SHOT_SOURCE)
                || *h == self.get(ID_SHOT_REGION);
            let gain = self
                .values
                .iter()
                .any(|(id, hwnd)| *hwnd == *h && (is_mic_gain(*id) || is_mic_gain_value(*id)));
            unsafe {
                // SAFETY: The HWND is a child control created with the main window.
                EnableWindow(*h, idle || (recording && (screenshot || gain)))
            };
        }
        unsafe {
            // SAFETY: The HWND is a child control created with the main window.

            EnableWindow(self.get(ID_STOP), recording);
            EnableWindow(self.get(ID_STATUS), true);
        }
    }
}

fn scaled_font(percent: u32, weight: i32) -> HFONT {
    unsafe {
        // SAFETY: The face name is NUL-terminated and outlives the call. The font is stored and later deleted.

        CreateFontW(
            -(17 * percent as i32 / 100),
            0,
            0,
            0,
            weight,
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
        _ => PRESET_CUSTOM,
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
        // SAFETY: The color is a plain COLORREF. The brush is stored for the process or deleted in this block.

        let item = &mut *(lparam.0 as *mut DRAWITEMSTRUCT);
        let id = item.CtlID as i32;
        if let Some((_, click)) = history_click(id) {
            let pressed = (item.itemState.0 & ODS_SELECTED.0) != 0;
            let acknowledged = controls.history_ack == Some(id);
            draw_history_icon(item, click, controls.scale_percent, pressed, acknowledged);
            return;
        }
        let radio = if (ID_FORMAT..=ID_FORMAT + 2).contains(&id) {
            Some((ID_FORMAT, controls.format_index))
        } else if (ID_TRAY_AREA_FORMAT..=ID_TRAY_AREA_FORMAT + 2).contains(&id) {
            Some((ID_TRAY_AREA_FORMAT, controls.tray_area_format_index))
        } else if (ID_TRAY_VIDEO_FORMAT..=ID_TRAY_VIDEO_FORMAT + 2).contains(&id) {
            Some((ID_TRAY_VIDEO_FORMAT, controls.tray_video_format_index))
        } else {
            None
        };
        if let Some((base, format_index)) = radio {
            let selected = format_index == (id - base) as usize;
            if base == ID_TRAY_AREA_FORMAT {
                let parent = CreateSolidBrush(BACKGROUND);
                FillRect(item.hDC, &item.rcItem, parent);
                DeleteObject(parent);
                let radius = (8 * controls.scale_percent as i32 / 100).max(4);
                let fill = if selected { ACCENT } else { BACKGROUND };
                let edge = if selected { ACCENT } else { MIST };
                let brush = CreateSolidBrush(fill);
                let pen = CreatePen(PS_SOLID, 1, edge);
                let old_brush = SelectObject(item.hDC, brush);
                let old_pen = SelectObject(item.hDC, pen);
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
                let mut rect = item.rcItem;
                SetBkMode(item.hDC, TRANSPARENT);
                SetTextColor(
                    item.hDC,
                    if selected {
                        crate::theme::ON_ACCENT
                    } else {
                        TEXT
                    },
                );
                let old = SelectObject(item.hDC, controls.font);
                DrawTextW(
                    item.hDC,
                    &mut text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(item.hDC, old);
                return;
            }
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
            if selected {
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
        let tab = id == ID_TAB || id == ID_SETTINGS_TAB;
        let active_tab =
            (id == ID_TAB && controls.page == 0) || (id == ID_SETTINGS_TAB && controls.page == 1);
        let enabled = IsWindowEnabled(item.hwndItem).as_bool();
        let resting = tab && !active_tab && enabled;
        let color = if !enabled {
            DISABLED
        } else if id == ID_STOP {
            ACCENT_DEEP
        } else if resting {
            BACKGROUND
        } else {
            ACCENT
        };
        let parent = CreateSolidBrush(BACKGROUND);
        FillRect(item.hDC, &item.rcItem, parent);
        DeleteObject(parent);
        let radius = (12 * controls.scale_percent as i32 / 100).max(4);
        if resting {
            let pen = CreatePen(PS_SOLID, 1, MIST);
            let old_brush = SelectObject(item.hDC, GetStockObject(NULL_BRUSH));
            let old_pen = SelectObject(item.hDC, pen);
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
            DeleteObject(pen);
        } else {
            let brush = CreateSolidBrush(color);
            let pen = CreatePen(PS_SOLID, 1, color);
            let old_brush = SelectObject(item.hDC, brush);
            let old_pen = SelectObject(item.hDC, pen);
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
        }
        let mut text = vec![0u16; 256];
        let len = GetWindowTextW(item.hwndItem, &mut text) as usize;
        text.truncate(len);
        SetBkMode(item.hDC, TRANSPARENT);
        let on_light = !enabled || resting;
        SetTextColor(
            item.hDC,
            if on_light {
                TEXT
            } else {
                crate::theme::ON_ACCENT
            },
        );
        let old = SelectObject(
            item.hDC,
            if tab && active_tab {
                controls.bold
            } else {
                controls.font
            },
        );
        DrawTextW(
            item.hDC,
            &mut text,
            &mut item.rcItem,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(item.hDC, old);
    }
}

fn draw_history_icon(
    item: &DRAWITEMSTRUCT,
    click: HistoryClick,
    percent: u32,
    pressed: bool,
    acknowledged: bool,
) {
    let (shift, check) = copy_feedback(pressed, acknowledged && click == HistoryClick::CopyFile);
    let scale = |value: i32| (value * percent as i32 / 100).max(1);
    unsafe {
        // SAFETY: The DC belongs to the button being drawn, and the pens are deleted before return.
        let parent = CreateSolidBrush(if check { ACCENT } else { BACKGROUND });
        FillRect(item.hDC, &item.rcItem, parent);
        DeleteObject(parent);
        let color = if check {
            crate::theme::ON_ACCENT
        } else {
            ACCENT
        };
        let pen = CreatePen(PS_SOLID, scale(2).max(1), color);
        let old = SelectObject(item.hDC, pen);
        SelectObject(item.hDC, GetStockObject(NULL_BRUSH));
        let nudge = scale(shift);
        let left = item.rcItem.left;
        let top = item.rcItem.top;
        let right = item.rcItem.right;
        let bottom = item.rcItem.bottom;
        let x = left + scale(5) + nudge;
        let y = top + scale(6) + nudge;
        let w = (right - left - scale(10)).max(8);
        let h = (bottom - top - scale(12)).max(8);
        match click {
            HistoryClick::CopyFile if check => {
                MoveToEx(item.hDC, x, y + h / 2, None);
                LineTo(item.hDC, x + w / 3, y + h - scale(1));
                LineTo(item.hDC, x + w, y);
            }
            HistoryClick::CopyFile => {
                Rectangle(item.hDC, x + scale(3), y, x + w, y + h - scale(3));
                Rectangle(item.hDC, x, y + scale(3), x + w - scale(3), y + h);
            }
            HistoryClick::Open => {
                Rectangle(item.hDC, x, y + scale(4), x + w - scale(5), y + h);
                MoveToEx(item.hDC, x + w / 2, y + h / 2, None);
                LineTo(item.hDC, x + w, y);
                MoveToEx(item.hDC, x + w - scale(4), y, None);
                LineTo(item.hDC, x + w, y);
                LineTo(item.hDC, x + w, y + scale(4));
            }
            HistoryClick::Reveal => {
                MoveToEx(item.hDC, x, y + scale(3), None);
                LineTo(item.hDC, x + scale(5), y + scale(3));
                LineTo(item.hDC, x + scale(7), y);
                LineTo(item.hDC, x + w, y);
                LineTo(item.hDC, x + w, y + h);
                LineTo(item.hDC, x, y + h);
                LineTo(item.hDC, x, y + scale(3));
            }
            HistoryClick::Label => {}
        }
        SelectObject(item.hDC, old);
        DeleteObject(pen);
    }
}

fn copy_feedback(pressed: bool, acknowledged: bool) -> (i32, bool) {
    if acknowledged {
        (0, true)
    } else if pressed {
        (2, false)
    } else {
        (0, false)
    }
}

#[cfg(test)]
mod layout_tests {
    use super::{
        ID_AREA_HEADING, ID_AUTOSTART, ID_CARD, ID_CLIPBOARD, ID_CLOSE_TRAY, ID_HIDE_ACTIVE,
        ID_HIGHLIGHT, ID_NOTIFY, ID_NO_SIGNAL, ID_START_MINIMIZED, ID_TRAY_CAPTURE, SETTINGS,
    };

    fn y(id: i32) -> i32 {
        SETTINGS
            .iter()
            .find(|place| place.id == id)
            .map(|place| place.y)
            .unwrap()
    }

    #[test]
    fn default_off_checkboxes_sit_above_the_ones_that_start_on() {
        let off = [
            ID_AUTOSTART,
            ID_START_MINIMIZED,
            ID_HIDE_ACTIVE,
            ID_CLOSE_TRAY,
            ID_CLIPBOARD,
            ID_TRAY_CAPTURE,
        ];
        let on = [ID_HIGHLIGHT, ID_NOTIFY, ID_NO_SIGNAL, ID_CARD];
        let mut last = 0;
        for id in off {
            assert!(y(id) > last);
            last = y(id);
        }
        assert!(
            y(on[0]) - (last + 26) > 10,
            "the enabled-by-default checkboxes should sit in their own group"
        );
        for id in on {
            assert!(y(id) > last);
            last = y(id);
        }
        assert!(
            y(ID_AREA_HEADING) >= last + 26 + 28,
            "Tray area recording must sit clear of the last checkbox"
        );
    }

    #[test]
    fn each_profile_keeps_its_own_microphone_slider() {
        use crate::ProfileRef;

        for source in ProfileRef::ALL {
            let id = super::microphone_control(source);
            assert_eq!(super::profile_for_microphone(id), Some(source));
        }
        assert_eq!(super::profile_for_microphone(super::ID_STOP), None);
    }

    #[test]
    fn microphone_volume_has_a_precise_range_past_one_hundred() {
        assert_eq!(super::MIC_GAIN_MAX, 300);
        assert!(
            super::MIC_GAIN_MAX > 100,
            "the volume range must offer more than 100%"
        );
        assert!(super::is_mic_gain(super::ID_TRAY_AREA_GAIN));
        assert!(super::is_mic_gain_value(super::ID_TRAY_AREA_GAIN_VALUE));
    }

    #[test]
    fn tray_area_defaults_are_quality_for_mkv_and_balance_for_animations() {
        use crate::media::Format;

        let area = crate::settings::Settings::default().tray_area;
        assert_eq!(
            super::preset_index(Format::Mkv, area.video[Format::Mkv.index()]),
            0
        );
        assert_eq!(
            super::preset_index(Format::Gif, area.video[Format::Gif.index()]),
            1
        );
        assert_eq!(
            super::preset_index(Format::Webp, area.video[Format::Webp.index()]),
            1
        );
    }

    #[test]
    fn settings_off_a_preset_read_as_custom() {
        use crate::media::{Format, VideoSettings};

        assert_eq!(
            super::preset_index(
                Format::Mkv,
                VideoSettings {
                    height: 1440,
                    fps: 60,
                }
            ),
            super::PRESET_CUSTOM
        );
        assert_eq!(
            super::preset_index(
                Format::Gif,
                VideoSettings {
                    height: 720,
                    fps: 50,
                }
            ),
            super::PRESET_CUSTOM
        );
        assert_eq!(
            super::preset_index(
                Format::Mkv,
                VideoSettings {
                    height: 1080,
                    fps: 60,
                }
            ),
            0
        );
        assert_eq!(
            super::preset_index(
                Format::Webp,
                VideoSettings {
                    height: 360,
                    fps: 30,
                }
            ),
            2
        );
    }

    #[test]
    fn copying_to_the_clipboard_answers_with_a_check() {
        assert_eq!(super::copy_feedback(false, false), (0, false));
        assert_eq!(super::copy_feedback(true, false), (2, false));
        assert_eq!(super::copy_feedback(true, true), (0, true));
    }
}
