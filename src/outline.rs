//! A click-through red frame around the area being recorded.
//!
//! The frame is excluded from capture and drawn outside the recorded pixels.
//! It blinks between a bright and a dim red, the same way the original did.

use crate::geom::{self, OutlineBox};
use crate::media::Rect;
use crate::theme;
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("ezcap.outline");

struct Draw {
    width: i32,
    height: i32,
    stroke: i32,
    color: COLORREF,
}

static DRAW: Mutex<Option<Draw>> = Mutex::new(None);

pub struct RegionOutline {
    hwnd: HWND,
}

impl RegionOutline {
    pub fn new() -> Self {
        Self {
            hwnd: HWND::default(),
        }
    }

    pub fn sync(&mut self, rect: Option<Rect>, elapsed_ms: u128) {
        let Some(rect) = rect else {
            self.hide();
            return;
        };
        let dpi = dpi_at(rect.x, rect.y);
        let frame = geom::recording_outline(rect.x, rect.y, rect.w, rect.h, dpi);
        let color = if geom::outline_blink(elapsed_ms) {
            theme::RECORDING
        } else {
            theme::RECORDING_DIM
        };
        self.show(frame, color);
    }

    pub fn hide(&mut self) {
        if self.hwnd.is_invalid() {
            return;
        }
        // SAFETY: `hwnd` is the outline window this value created. The invalid case returned above.
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    fn show(&mut self, frame: OutlineBox, color: COLORREF) {
        if self.hwnd.is_invalid() && !self.create() {
            return;
        }
        let hwnd = self.hwnd;
        if let Ok(mut draw) = DRAW.lock() {
            *draw = Some(Draw {
                width: frame.width,
                height: frame.height,
                stroke: frame.stroke,
                color,
            });
        }
        // SAFETY: `hwnd` belongs to this outline. The DC from GetDC is released in the same block.
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                frame.x,
                frame.y,
                frame.width,
                frame.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .ok();
            let dc = GetDC(hwnd);
            if !dc.is_invalid() {
                paint(dc);
                ReleaseDC(hwnd, dc);
            }
        }
    }

    fn create(&mut self) -> bool {
        // SAFETY: the class name lives for the process. The created HWND is stored here and destroyed in Drop.
        unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassW(&class);
            let Ok(hwnd) = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                CLASS,
                w!("ezcap recording area"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                instance,
                None,
            ) else {
                return false;
            };
            SetLayeredWindowAttributes(hwnd, theme::BLACK, 255, LWA_COLORKEY).ok();
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).ok();
            self.hwnd = hwnd;
            true
        }
    }
}

impl Drop for RegionOutline {
    fn drop(&mut self) {
        if !self.hwnd.is_invalid() {
            // SAFETY: this is the only owner of the outline window, and Drop runs once.
            unsafe { DestroyWindow(self.hwnd) }.ok();
            self.hwnd = HWND::default();
        }
    }
}

fn dpi_at(x: i32, y: i32) -> u32 {
    // SAFETY: any point is a valid monitor query. The DPI integers are writable locals.
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok() {
            dpi_x.max(96)
        } else {
            96
        }
    }
}

fn paint(dc: HDC) {
    let Some(draw) = DRAW.lock().ok().and_then(|guard| {
        guard.as_ref().map(|draw| Draw {
            width: draw.width,
            height: draw.height,
            stroke: draw.stroke,
            color: draw.color,
        })
    }) else {
        return;
    };
    // SAFETY: `dc` is the paint DC of the outline window. Both brushes are deleted before return.
    unsafe {
        let black = CreateSolidBrush(theme::BLACK);
        let border = CreateSolidBrush(draw.color);
        let whole = RECT {
            left: 0,
            top: 0,
            right: draw.width,
            bottom: draw.height,
        };
        FillRect(dc, &whole, black);
        let stroke = draw.stroke.clamp(1, draw.width.min(draw.height).max(1));
        for edge in [
            RECT {
                left: 0,
                top: 0,
                right: draw.width,
                bottom: stroke,
            },
            RECT {
                left: 0,
                top: draw.height - stroke,
                right: draw.width,
                bottom: draw.height,
            },
            RECT {
                left: 0,
                top: 0,
                right: stroke,
                bottom: draw.height,
            },
            RECT {
                left: draw.width - stroke,
                top: 0,
                right: draw.width,
                bottom: draw.height,
            },
        ] {
            FillRect(dc, &edge, border);
        }
        DeleteObject(black);
        DeleteObject(border);
    }
}

// SAFETY: Windows calls this for an outline window of this class. BeginPaint and EndPaint bracket the DC.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut info = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut info);
            if !dc.is_invalid() {
                paint(dc);
            }
            EndPaint(hwnd, &info);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
