//! Sea breeze. One place for the interface colors.
//!
//! Windows stores COLORREF as 0x00BBGGRR.

use windows::Win32::Foundation::COLORREF;

/// Daylight on still water.
pub const BACKGROUND: COLORREF = COLORREF(0x00f6f3ee);
/// A slightly deeper mist, for controls that are present but not chosen.
pub const MIST: COLORREF = COLORREF(0x00eadfd4);
/// The main sea blue.
pub const ACCENT: COLORREF = COLORREF(0x00c48c3e);
/// Deeper water, for a pressed or emphasized control.
pub const ACCENT_DEEP: COLORREF = COLORREF(0x00a56e28);
/// Navy ink on the light surfaces.
pub const TEXT: COLORREF = COLORREF(0x00422e1c);
/// Light ink on the sea-blue controls.
pub const ON_ACCENT: COLORREF = COLORREF(0x00fbf8f4);
/// A control that cannot be used.
pub const DISABLED: COLORREF = COLORREF(0x00c8beb4);

/// Overlay buttons sit on the screenshot, so they are darker than the window.
pub const OVERLAY: COLORREF = COLORREF(0x0034281c);
pub const OVERLAY_INK: COLORREF = COLORREF(0x00f6f1e8);
pub const OVERLAY_DISABLED: COLORREF = COLORREF(0x003c332a);
pub const OVERLAY_DISABLED_INK: COLORREF = COLORREF(0x00a4988a);
pub const OVERLAY_EDGE: COLORREF = COLORREF(0x00584a3a);
/// The Copy action stands out in green; Cancel is red in every overlay.
pub const COPY: COLORREF = COLORREF(0x003ca83a);
pub const CANCEL: COLORREF = COLORREF(0x00323cd2);

/// The small size label on the selection.
pub const BADGE: COLORREF = COLORREF(0x00422e1c);
pub const BADGE_TEXT: COLORREF = COLORREF(0x00ffffff);

pub const WHITE: COLORREF = COLORREF(0x00ffffff);
pub const BLACK: COLORREF = COLORREF(0x00000000);
/// Shadow under the overlay hint, so the white letters stay readable.
pub const HINT_SHADOW: COLORREF = COLORREF(0x00101010);

/// Transparent key of the text editor. The caret is the inverse of this color,
/// which makes it the sea-blue accent.
pub const CHROMA: COLORREF = inverse(ACCENT);

/// Pen colors offered in the overlay.
pub const SWATCHES: [COLORREF; 8] = [
    COLORREF(0x002323ef),
    COLORREF(0x000095ff),
    COLORREF(0x000ad6ff),
    COLORREF(0x0037c734),
    COLORREF(0x0025840a),
    COLORREF(0x00e252af),
    WHITE,
    BLACK,
];
/// Highlighter stroke. It is drawn lighter than the chosen swatch.
pub const MARKER: COLORREF = COLORREF(0x004fc3f7);

/// Blinking frame around the area being recorded.
pub const RECORDING: COLORREF = COLORREF(0x005246ff);
pub const RECORDING_DIM: COLORREF = COLORREF(0x002e239a);
/// Stop and exit marks in the tray menu. Stored as 0x00BBGGRR, drawn red.
pub const ALERT: COLORREF = COLORREF(0x00323cd2);
/// Tray badge while a recording is in progress: red fill, darker red rim.
pub const BADGE_RECORDING: COLORREF = COLORREF(0x002020e6);
pub const BADGE_RECORDING_EDGE: COLORREF = COLORREF(0x0010108c);
/// Tray badge while the file is being saved: orange fill, darker orange rim.
pub const BADGE_SAVING: COLORREF = COLORREF(0x00208cff);
pub const BADGE_SAVING_EDGE: COLORREF = COLORREF(0x001050aa);

const fn inverse(color: COLORREF) -> COLORREF {
    COLORREF(0x00ffffff ^ (color.0 & 0x00ffffff))
}

/// BGRA bytes for a 32-bit bitmap. COLORREF stores the same color as 0x00BBGGRR.
pub const fn bgra(color: COLORREF) -> [u8; 4] {
    let value = color.0;
    [(value >> 16) as u8, (value >> 8) as u8, value as u8, 255]
}
