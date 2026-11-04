use crate::geom::{self, Panel};
use crate::media::{self, Frame, Profile, Rect};
use crate::theme;
use std::path::PathBuf;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("ezcap.region");
static mut REGION: *mut Region = std::ptr::null_mut();
static mut EDIT_PROC: isize = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Select,
    Pen,
    Line,
    Arrow,
    Rectangle,
    Marker,
    Text,
    Color,
    Width,
    Undo,
    Redo,
    Delete,
    Copy,
    Save,
    Record,
    Cancel,
    Swatch,
}

#[derive(Clone, Copy, PartialEq)]
struct Selection {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Selection {
    fn new(a: Point, b: Point) -> Self {
        Self {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            w: (a.x - b.x).abs(),
            h: (a.y - b.y).abs(),
        }
    }
    fn right(self) -> i32 {
        self.x + self.w
    }
    fn bottom(self) -> i32 {
        self.y + self.h
    }
    fn usable(self) -> bool {
        self.w >= 2 && self.h >= 2
    }
    fn clamp(self, width: i32, height: i32) -> Self {
        let x0 = self.x.max(0).min(width);
        let y0 = self.y.max(0).min(height);
        let x1 = self.right().max(0).min(width);
        let y1 = self.bottom().max(0).min(height);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }
    fn output(self, monitor: Rect) -> Rect {
        Rect {
            x: monitor.x + self.x,
            y: monitor.y + self.y,
            w: self.w as u32,
            h: self.h as u32,
        }
    }
    fn panel(self) -> Panel {
        Panel {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Select,
    Pen,
    Line,
    Arrow,
    Rectangle,
    Marker,
    Text,
}

#[derive(Clone)]
enum ShapeKind {
    Pen(Vec<Point>),
    Line(Point, Point),
    Arrow(Point, Point),
    Rectangle(Point, Point),
    Marker(Vec<Point>),
    Text(Point, i32, String),
}

#[derive(Clone)]
struct Shape {
    kind: ShapeKind,
    color: COLORREF,
    width: i32,
    font_size: i32,
    font_name: String,
}

#[derive(Clone, Copy)]
enum Handle {
    Move,
    N,
    S,
    E,
    W,
    NW,
    NE,
    SW,
    SE,
}

#[derive(Clone, Copy)]
enum Drag {
    Create(Point),
    Edit(Handle, Point, Selection),
    Width(i32, i32),
    TextMove(Point, Point),
    TextSize(Handle, Point, Point, i32, i32),
}

#[derive(Clone, Copy)]
struct EditDrag {
    origin: Point,
    start_client: Point,
    start_screen: (i32, i32),
}

struct Button {
    role: Role,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    shown: bool,
    swatch: Option<usize>,
}

pub enum Outcome {
    Cancel,
    Record(Rect, Profile),
    Saved(PathBuf),
}

struct Region {
    hwnd: HWND,
    frame: Frame,
    monitor: Rect,
    monitors: Vec<Rect>,
    current_monitor: usize,
    profile: Profile,
    dynamic: bool,
    allow_record: bool,
    selection: Option<Selection>,
    drag: Option<Drag>,
    tool: Tool,
    shapes: Vec<Shape>,
    redo: Vec<Shape>,
    current: Option<Shape>,
    color: COLORREF,
    width: i32,
    font_size: i32,
    font_name: String,
    buttons: Vec<Button>,
    scale: u32,
    edit: HWND,
    edit_pos: Point,
    edit_w: i32,
    edit_h: i32,
    edit_font: HFONT,
    editing: bool,
    edit_drag: Option<EditDrag>,
    palette: bool,
    outcome: Option<Outcome>,
    font: HFONT,
    icon_font: HFONT,
    font_resource: HANDLE,
    font_scale: u32,
    back_dc: HDC,
    back_bmp: HBITMAP,
    back_w: u32,
    back_h: u32,
}

impl Region {
    fn new(
        hwnd: HWND,
        profile: Profile,
        dynamic: bool,
        allow_record: bool,
    ) -> Result<Box<Self>, String> {
        let current = media::cursor_monitor_rect();
        let frame = if allow_record {
            crate::capture::grab_still(current)?
        } else {
            crate::capture::grab_display(current)?
        };
        let mut region = Self::with_frame(hwnd, profile, dynamic, allow_record, frame, current);
        let monitors = monitor_rects();
        region.current_monitor = monitors
            .iter()
            .position(|rect| same_rect(*rect, current))
            .unwrap_or(0);
        region.monitors = monitors;
        Ok(region)
    }

    fn with_frame(
        hwnd: HWND,
        profile: Profile,
        dynamic: bool,
        allow_record: bool,
        frame: Frame,
        monitor: Rect,
    ) -> Box<Self> {
        let scale = if monitor.h >= 2160 {
            200
        } else if monitor.h >= 1440 {
            150
        } else {
            100
        };
        Box::new(Self {
            hwnd,
            frame,
            monitor,
            monitors: vec![monitor],
            current_monitor: 0,
            profile,
            dynamic,
            allow_record,
            selection: None,
            drag: None,
            tool: Tool::Select,
            shapes: Vec::new(),
            redo: Vec::new(),
            current: None,
            color: theme::SWATCHES[0],
            width: 4,
            font_size: 28,
            font_name: "Segoe UI".into(),
            buttons: Vec::new(),
            scale,
            edit: HWND::default(),
            edit_pos: Point { x: 0, y: 0 },
            edit_w: 240,
            edit_h: 80,
            edit_font: HFONT::default(),
            editing: false,
            edit_drag: None,
            palette: false,
            outcome: None,
            font: HFONT::default(),
            icon_font: HFONT::default(),
            font_resource: HANDLE::default(),
            font_scale: 0,
            back_dc: HDC::default(),
            back_bmp: HBITMAP::default(),
            back_w: 0,
            back_h: 0,
        })
    }

    fn show(&mut self) {
        self.layout();
        unsafe {
            // SAFETY: The HWND is a window or child control this program created and still owns.

            let disabled = 1i32;
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                (&disabled as *const i32).cast(),
                4,
            )
            .ok();
            SetWindowDisplayAffinity(self.hwnd, WDA_EXCLUDEFROMCAPTURE).ok();
            InvalidateRect(self.hwnd, None, false).ok();
            UpdateWindow(self.hwnd);
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
            SetFocus(self.hwnd);
        }
    }

    fn selected(&self) -> Option<Selection> {
        self.selection.filter(|selection| selection.usable())
    }

    fn finish(&mut self, outcome: Outcome) {
        self.commit_text();
        self.outcome = Some(outcome);
        unsafe {
            // SAFETY: The HWND is a window or child control this program created and still owns.

            ShowWindow(self.hwnd, SW_HIDE);
            SetWindowDisplayAffinity(self.hwnd, WDA_NONE).ok();
            crate::notify_region_closed();
        }
    }

    fn crop(&self) -> Option<Frame> {
        let selection = self.selected()?;
        let mut data = vec![0u8; selection.w as usize * selection.h as usize * 4];
        for y in 0..selection.h as usize {
            let source =
                ((selection.y as usize + y) * self.frame.width as usize + selection.x as usize) * 4;
            let target = y * selection.w as usize * 4;
            let end = source + selection.w as usize * 4;
            if end <= self.frame.data.len() {
                data[target..target + selection.w as usize * 4]
                    .copy_from_slice(&self.frame.data[source..end]);
            }
        }
        let mut frame = Frame {
            width: selection.w as u32,
            height: selection.h as u32,
            data,
        };
        draw_annotations(
            &mut frame,
            &self.shapes,
            Point {
                x: -selection.x,
                y: -selection.y,
            },
        );
        Some(frame)
    }

    fn save(&mut self, choose_path: bool) {
        self.commit_text();
        let Some(frame) = self.crop() else { return };
        let path = if choose_path {
            save_dialog()
        } else {
            media::output_path("Image", "png").ok()
        };
        let Some(path) = path else { return };
        match media::save_frame(&frame, &path) {
            Ok(()) => {
                if !choose_path {
                    let _ = media::copy_image_to_clipboard(&frame);
                }
                self.finish(Outcome::Saved(path));
            }
            Err(error) => {
                message(&error);
            }
        }
    }

    fn push_shape(&mut self, shape: Shape) {
        self.shapes.push(shape);
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(shape) = self.shapes.pop() {
            self.redo.push(shape);
        }
    }

    fn redo_shape(&mut self) {
        if let Some(shape) = self.redo.pop() {
            self.shapes.push(shape);
        }
    }

    fn role_enabled(&self, role: Role) -> bool {
        match role {
            Role::Undo | Role::Delete => !self.shapes.is_empty(),
            Role::Redo => !self.redo.is_empty(),
            Role::Record => self.allow_record,
            _ => true,
        }
    }

    fn press(&mut self, index: usize, point: Point) {
        let Some(button) = self
            .buttons
            .get(index)
            .map(|button| (button.role, button.swatch))
        else {
            return;
        };
        if !self.role_enabled(button.0) {
            return;
        }
        match button.0 {
            Role::Select => {
                self.commit_text();
                self.tool = Tool::Select;
            }
            Role::Pen => {
                self.commit_text();
                self.tool = Tool::Pen;
            }
            Role::Line => {
                self.commit_text();
                self.tool = Tool::Line;
            }
            Role::Arrow => {
                self.commit_text();
                self.tool = Tool::Arrow;
            }
            Role::Rectangle => {
                self.commit_text();
                self.tool = Tool::Rectangle;
            }
            Role::Marker => {
                self.commit_text();
                self.tool = Tool::Marker;
            }
            Role::Text => {
                self.commit_text();
                self.tool = Tool::Text;
            }
            Role::Color => self.palette = !self.palette,
            Role::Swatch => {
                if let Some(swatch) = button.1 {
                    self.color = theme::SWATCHES[swatch];
                }
                self.palette = false;
            }
            Role::Width => self.drag = Some(Drag::Width(point.x, self.width)),
            Role::Undo => self.undo(),
            Role::Redo => self.redo_shape(),
            Role::Delete => {
                self.undo();
            }
            Role::Copy => self.save(false),
            Role::Save => self.save(true),
            Role::Record => {
                if let Some(selection) = self.selected() {
                    self.finish(Outcome::Record(
                        selection.output(self.monitor),
                        self.profile.clone(),
                    ));
                }
            }
            Role::Cancel => self.finish(Outcome::Cancel),
        }
        self.layout();
        self.redraw();
    }

    #[cfg(test)]
    fn select_role(&mut self, role: Role) {
        self.tool = match role {
            Role::Pen => Tool::Pen,
            Role::Line => Tool::Line,
            Role::Arrow => Tool::Arrow,
            Role::Rectangle => Tool::Rectangle,
            Role::Marker => Tool::Marker,
            Role::Text => Tool::Text,
            _ => Tool::Select,
        };
        self.layout();
        self.redraw();
    }

    fn key_down(&mut self, key: u32) {
        if self.editing {
            return;
        }
        if key == VK_ESCAPE.0 as u32 {
            if self.tool != Tool::Select {
                self.tool = Tool::Select;
            } else {
                self.finish(Outcome::Cancel);
            }
            self.layout();
            return;
        }
        let control = unsafe {
            // SAFETY: The call only reads keyboard state and takes no pointer.
            GetKeyState(VK_CONTROL.0 as i32)
        } < 0;
        let shift = unsafe {
            // SAFETY: The call only reads keyboard state and takes no pointer.
            GetKeyState(VK_SHIFT.0 as i32)
        } < 0;
        if control && shift && key == 'Z' as u32 {
            self.redo_shape();
        } else if control && key == 'Z' as u32 {
            self.undo();
        } else if control && key == 'A' as u32 {
            self.selection = Some(Selection {
                x: 0,
                y: 0,
                w: self.frame.width as i32,
                h: self.frame.height as i32,
            });
        } else if control && key == 'C' as u32 {
            self.save(false);
        } else if control && key == 'S' as u32 {
            self.save(true);
        } else if key == VK_DELETE.0 as u32 {
            self.undo();
        } else if key == VK_TAB.0 as u32 {
            self.next_monitor();
        } else if key == VK_RETURN.0 as u32 {
            if self.dynamic {
                self.save(false);
            } else if self.allow_record {
                if let Some(selection) = self.selected() {
                    self.finish(Outcome::Record(
                        selection.output(self.monitor),
                        self.profile.clone(),
                    ));
                }
            }
        } else if let Some(mut selection) = self.selected() {
            let (dx, dy) = match key {
                value if value == VK_LEFT.0 as u32 => (-1, 0),
                value if value == VK_RIGHT.0 as u32 => (1, 0),
                value if value == VK_UP.0 as u32 => (0, -1),
                value if value == VK_DOWN.0 as u32 => (0, 1),
                _ => (0, 0),
            };
            if dx != 0 || dy != 0 {
                if shift {
                    selection.w = (selection.w + dx).max(2);
                    selection.h = (selection.h + dy).max(2);
                } else {
                    selection.x += dx;
                    selection.y += dy;
                }
                self.selection =
                    Some(selection.clamp(self.frame.width as i32, self.frame.height as i32));
            }
        }
        self.layout();
        self.redraw();
    }

    fn next_monitor(&mut self) {
        if self.monitors.len() < 2 {
            return;
        }
        self.current_monitor = (self.current_monitor + 1) % self.monitors.len();
        self.monitor = self.monitors[self.current_monitor];
        if let Ok(frame) = crate::capture::grab_still(self.monitor) {
            self.frame = frame;
            self.selection = None;
            self.shapes.clear();
            self.redo.clear();
            unsafe {
                // SAFETY: The HWND is a window this program created. The position is plain integers.

                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    self.monitor.x,
                    self.monitor.y,
                    self.monitor.w as i32,
                    self.monitor.h as i32,
                    SWP_NOACTIVATE,
                )
                .ok();
            }
        }
    }

    fn hit_button(&self, point: Point) -> Option<usize> {
        self.buttons
            .iter()
            .enumerate()
            .rev()
            .find(|(_, button)| {
                button.shown
                    && point.x >= button.x
                    && point.x < button.x + button.w
                    && point.y >= button.y
                    && point.y < button.y + button.h
            })
            .map(|(index, _)| index)
    }

    fn text_pad(&self) -> i32 {
        (8 * self.scale as i32 / 100).max(6)
    }

    fn text_outer(&self) -> Selection {
        let pad = self.text_pad();
        Selection {
            x: self.edit_pos.x - pad,
            y: self.edit_pos.y - pad,
            w: self.edit_w + pad * 2,
            h: self.edit_h + pad * 2,
        }
    }

    fn hit_text_handle(&self, point: Point) -> Option<Handle> {
        if !self.editing {
            return None;
        }
        let frame = self.text_outer();
        let spots = [
            (Handle::NW, frame.x, frame.y),
            (Handle::NE, frame.right(), frame.y),
            (Handle::SW, frame.x, frame.bottom()),
            (Handle::SE, frame.right(), frame.bottom()),
            (Handle::N, frame.x + frame.w / 2, frame.y),
            (Handle::S, frame.x + frame.w / 2, frame.bottom()),
            (Handle::W, frame.x, frame.y + frame.h / 2),
            (Handle::E, frame.right(), frame.y + frame.h / 2),
        ];
        spots
            .into_iter()
            .find(|(_, x, y)| (point.x - x).abs() <= 8 && (point.y - y).abs() <= 8)
            .map(|(handle, _, _)| handle)
    }

    fn over_text_box(&self, point: Point) -> bool {
        if !self.editing {
            return false;
        }
        let outer = self.text_outer();
        point.x >= outer.x
            && point.x < outer.right()
            && point.y >= outer.y
            && point.y < outer.bottom()
    }

    fn begin_text_move(&mut self, point: Point) {
        self.drag = Some(Drag::TextMove(self.edit_pos, point));
        unsafe {
            // SAFETY: The HWND is the visible overlay owned by this thread.

            SetCapture(self.hwnd);
        }
    }

    fn inside_edit(&self, point: Point) -> bool {
        point.x >= self.edit_pos.x
            && point.x < self.edit_pos.x + self.edit_w
            && point.y >= self.edit_pos.y
            && point.y < self.edit_pos.y + self.edit_h
    }

    fn forward_edit_click(&mut self, point: Point, double: bool) {
        let x = point.x - self.edit_pos.x;
        let y = point.y - self.edit_pos.y;
        let packed = (x & 0xffff) as isize | (((y & 0xffff) as isize) << 16);
        let message = if double {
            WM_LBUTTONDBLCLK
        } else {
            WM_LBUTTONDOWN
        };
        unsafe {
            // SAFETY: The HWND is the editor or main window this program created.

            SetFocus(self.edit);
            SendMessageW(self.edit, message, WPARAM(1), LPARAM(packed));
        }
    }

    fn in_text_border(&self, point: Point) -> bool {
        let outer = self.text_outer();
        let inside_outer = point.x >= outer.x
            && point.x < outer.right()
            && point.y >= outer.y
            && point.y < outer.bottom();
        let inside_edit = point.x >= self.edit_pos.x
            && point.x < self.edit_pos.x + self.edit_w
            && point.y >= self.edit_pos.y
            && point.y < self.edit_pos.y + self.edit_h;
        inside_outer && !inside_edit
    }

    fn place_edit(&mut self) {
        self.edit_pos.x = self
            .edit_pos
            .x
            .clamp(0, (self.frame.width as i32 - self.edit_w).max(0));
        self.edit_pos.y = self
            .edit_pos
            .y
            .clamp(0, (self.frame.height as i32 - self.edit_h).max(0));
        let origin = self.edit_screen_origin();
        unsafe {
            // SAFETY: The HWND is a window this program created. The position is plain integers.

            SetWindowPos(
                self.edit,
                HWND_TOPMOST,
                origin.x,
                origin.y,
                self.edit_w,
                self.edit_h,
                SWP_NOACTIVATE | SWP_NOCOPYBITS,
            )
            .ok();
            InvalidateRect(self.edit, None, true);
        }
        self.redraw();
    }

    fn edit_screen_origin(&self) -> POINT {
        let mut origin = POINT {
            x: self.edit_pos.x,
            y: self.edit_pos.y,
        };
        unsafe {
            // SAFETY: The POINT is in client coordinates of a window this program created.

            ClientToScreen(self.hwnd, &mut origin).ok();
        }
        origin
    }

    /// Glyph origin and wrap width inside the edit.
    ///
    /// The formatting rectangle does not include the control's left and right margins, so committing
    /// at the window corner draws the text a margin-width to the left of where it was typed.
    fn edit_text_box(&self) -> (Point, i32) {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: self.edit_w,
            bottom: self.edit_h,
        };
        let mut margins = 0i32;
        if !self.edit.is_invalid() {
            unsafe {
                // SAFETY: The target is a window this program created. Any pointer in the message addresses memory that outlives the call.

                let mut actual = RECT::default();
                SendMessageW(
                    self.edit,
                    0x00B2,
                    WPARAM(0),
                    LPARAM(&mut actual as *mut RECT as isize),
                );
                if actual.right > actual.left && actual.bottom > actual.top {
                    rect = actual;
                }
                margins = SendMessageW(self.edit, 0x00D4, WPARAM(0), LPARAM(0)).0 as i32;
            }
        }
        text_commit_box(self.edit_pos, self.edit_w, rect, margins)
    }

    fn resize_text(
        &mut self,
        handle: Handle,
        origin: Point,
        start: Point,
        width: i32,
        height: i32,
        point: Point,
    ) {
        let dx = point.x - origin.x;
        let dy = point.y - origin.y;
        let mut left = start.x;
        let mut top = start.y;
        let mut right = start.x + width;
        let mut bottom = start.y + height;
        if matches!(handle, Handle::W | Handle::NW | Handle::SW) {
            left += dx;
        }
        if matches!(handle, Handle::E | Handle::NE | Handle::SE) {
            right += dx;
        }
        if matches!(handle, Handle::N | Handle::NW | Handle::NE) {
            top += dy;
        }
        if matches!(handle, Handle::S | Handle::SW | Handle::SE) {
            bottom += dy;
        }
        if right - left < 96 {
            if matches!(handle, Handle::W | Handle::NW | Handle::SW) {
                left = right - 96;
            } else {
                right = left + 96;
            }
        }
        if bottom - top < 40 {
            if matches!(handle, Handle::N | Handle::NW | Handle::NE) {
                top = bottom - 40;
            } else {
                bottom = top + 40;
            }
        }
        self.edit_pos = Point { x: left, y: top };
        self.edit_w = right - left;
        self.edit_h = bottom - top;
        self.place_edit();
    }

    fn mouse_down(&mut self, point: Point, double: bool) {
        if self.editing {
            if let Some(handle) = self.hit_text_handle(point) {
                self.drag = Some(Drag::TextSize(
                    handle,
                    point,
                    self.edit_pos,
                    self.edit_w,
                    self.edit_h,
                ));
                unsafe {
                    // SAFETY: The HWND is the visible overlay owned by this thread.

                    SetCapture(self.hwnd);
                }
                return;
            }
            if self.in_text_border(point) {
                self.begin_text_move(point);
                return;
            }
            if self.inside_edit(point) {
                self.forward_edit_click(point, double);
                return;
            }
        }
        let was_editing = self.editing;
        if was_editing {
            self.commit_text();
        }
        if let Some(index) = self.hit_button(point) {
            self.press(index, point);
            if matches!(self.drag, Some(Drag::Width(_, _))) {
                unsafe {
                    // SAFETY: The HWND is the visible overlay owned by this thread.

                    SetCapture(self.hwnd);
                }
            }
            return;
        }
        if was_editing {
            return;
        }
        if double && self.tool == Tool::Select {
            self.selection = Some(Selection {
                x: 0,
                y: 0,
                w: self.frame.width as i32,
                h: self.frame.height as i32,
            });
            self.layout();
            self.redraw();
            return;
        }
        match self.tool {
            Tool::Select => {
                self.drag = Some(
                    match self
                        .selection
                        .and_then(|selection| hit_test(selection, point))
                    {
                        Some(handle) => Drag::Edit(handle, point, self.selection.unwrap()),
                        None => {
                            self.selection = Some(Selection {
                                x: point.x,
                                y: point.y,
                                w: 0,
                                h: 0,
                            });
                            Drag::Create(point)
                        }
                    },
                );
            }
            Tool::Text => self.start_text(point),
            tool => {
                let kind = match tool {
                    Tool::Pen => ShapeKind::Pen(vec![point]),
                    Tool::Line => ShapeKind::Line(point, point),
                    Tool::Arrow => ShapeKind::Arrow(point, point),
                    Tool::Rectangle => ShapeKind::Rectangle(point, point),
                    Tool::Marker => ShapeKind::Marker(vec![point]),
                    _ => return,
                };
                self.current = Some(Shape {
                    kind,
                    color: self.color,
                    width: self.width,
                    font_size: self.font_size,
                    font_name: self.font_name.clone(),
                });
            }
        }
        unsafe {
            // SAFETY: The HWND is the visible overlay owned by this thread.

            SetCapture(self.hwnd);
        }
        self.layout();
        self.redraw();
    }

    fn mouse_move(&mut self, point: Point) {
        let point = Point {
            x: point.x.clamp(0, self.frame.width as i32),
            y: point.y.clamp(0, self.frame.height as i32),
        };
        if let Some(Drag::Width(origin, start)) = self.drag {
            self.width = (start + (point.x - origin) / 8).clamp(1, 16);
            self.redraw();
            return;
        }
        if let Some(Drag::TextMove(origin, start)) = self.drag {
            self.edit_pos = Point {
                x: origin.x + point.x - start.x,
                y: origin.y + point.y - start.y,
            };
            self.place_edit();
            return;
        }
        if let Some(Drag::TextSize(handle, origin, start, width, height)) = self.drag {
            self.resize_text(handle, origin, start, width, height, point);
            return;
        }
        if let Some(drag) = self.drag {
            match drag {
                Drag::Create(anchor) => self.selection = Some(Selection::new(anchor, point)),
                Drag::Edit(handle, origin, start) => {
                    self.selection = Some(
                        edit_selection(start, handle, origin, point)
                            .clamp(self.frame.width as i32, self.frame.height as i32),
                    )
                }
                Drag::Width(_, _) | Drag::TextMove(_, _) | Drag::TextSize(_, _, _, _, _) => {}
            }
        } else if let Some(shape) = &mut self.current {
            match &mut shape.kind {
                ShapeKind::Pen(points) | ShapeKind::Marker(points) => points.push(point),
                ShapeKind::Line(_, end)
                | ShapeKind::Arrow(_, end)
                | ShapeKind::Rectangle(_, end) => *end = point,
                ShapeKind::Text(_, _, _) => {}
            }
        } else {
            return;
        }
        self.redraw();
    }

    fn mouse_up(&mut self, point: Point) {
        if matches!(
            self.drag,
            Some(Drag::TextMove(_, _)) | Some(Drag::TextSize(_, _, _, _, _))
        ) {
            self.drag = None;
            unsafe {
                // SAFETY: This thread captured the mouse on the overlay window.

                ReleaseCapture().ok();
            }
            self.redraw();
            return;
        }
        if let Some(Drag::Width(origin, start)) = self.drag.take() {
            if (point.x - origin).abs() < 4 {
                self.width = match start {
                    0..=2 => 4,
                    3..=4 => 8,
                    5..=8 => 12,
                    _ => 2,
                };
            }
            unsafe {
                // SAFETY: This thread captured the mouse on the overlay window.

                ReleaseCapture().ok();
            }
            self.layout();
            self.redraw();
            return;
        }
        self.drag = None;
        if self.selection.is_some_and(|selection| !selection.usable()) {
            self.selection = None;
        }
        if let Some(shape) = self.current.take() {
            self.push_shape(shape);
        }
        unsafe {
            // SAFETY: This thread captured the mouse on the overlay window.

            ReleaseCapture().ok();
        }
        self.layout();
        self.redraw();
    }

    fn start_text(&mut self, point: Point) {
        self.edit_pos = point;
        self.edit_w = 240;
        self.edit_h = 80;
        self.editing = true;
        if !self.edit_font.is_invalid() {
            unsafe {
                // SAFETY: The face name is NUL-terminated and outlives the call. The font is stored and later deleted.

                DeleteObject(self.edit_font);
            }
        }
        self.edit_font = unsafe {
            // SAFETY: The face name is NUL-terminated and outlives the call. The font is stored and later deleted.

            CreateFontW(
                -self.font_size.max(18),
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
                NONANTIALIASED_QUALITY.0 as u32,
                0,
                w!("Segoe UI"),
            )
        };
        if self.edit.is_invalid() {
            let origin = self.edit_screen_origin();
            let style = WS_POPUP
                | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_NOHIDESEL as u32);
            let Ok(edit) = (unsafe {
                // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    w!("EDIT"),
                    None,
                    style,
                    origin.x,
                    origin.y,
                    self.edit_w,
                    self.edit_h,
                    self.hwnd,
                    None,
                    GetModuleHandleW(None).unwrap(),
                    None,
                )
            }) else {
                self.editing = false;
                return;
            };
            self.edit = edit;
            unsafe {
                // SAFETY: The HWND is a control this program created. The theme names are literals.

                windows::Win32::UI::Controls::SetWindowTheme(self.edit, w!(""), w!("")).ok();
                SetLayeredWindowAttributes(self.edit, theme::CHROMA, 0, LWA_COLORKEY).ok();
                let previous = SetWindowLongPtrW(
                    self.edit,
                    GWLP_WNDPROC,
                    edit_proc as *const () as usize as isize,
                );
                EDIT_PROC = previous;
            }
        }
        self.place_edit();
        unsafe {
            // SAFETY: The HWND is a window or child control this program created and still owns.

            SendMessageW(
                self.edit,
                WM_SETFONT,
                WPARAM(self.edit_font.0 as usize),
                LPARAM(1),
            );
            let hint = crate::wide("Type, Enter to finish");
            SendMessageW(self.edit, 0x1501, WPARAM(1), LPARAM(hint.as_ptr() as isize));
            SetWindowTextW(self.edit, PCWSTR::null()).ok();
            ShowWindow(self.edit, SW_SHOW);
            SetFocus(self.edit);
        }
        self.layout();
        self.redraw();
    }

    fn commit_text(&mut self) {
        if !self.editing || self.edit.is_invalid() {
            return;
        }
        let len = unsafe {
            // SAFETY: The HWND is a window or child control this program created and still owns.
            GetWindowTextLengthW(self.edit)
        } as usize;
        let mut buffer = vec![0u16; len + 1];
        unsafe {
            // SAFETY: The HWND is a window or child control this program created and still owns.

            GetWindowTextW(self.edit, &mut buffer);
            ShowWindow(self.edit, SW_HIDE);
        }
        self.editing = false;
        let text = String::from_utf16_lossy(&buffer[..len]);
        if !text.trim().is_empty() {
            let (origin, wrap) = self.edit_text_box();
            self.push_shape(Shape {
                kind: ShapeKind::Text(origin, wrap, text),
                color: self.color,
                width: self.width,
                font_size: self.font_size.max(18),
                font_name: self.font_name.clone(),
            });
        }
        self.layout();
        self.redraw();
    }

    fn cancel_text(&mut self) {
        self.editing = false;
        if !self.edit.is_invalid() {
            unsafe {
                // SAFETY: The HWND is a window or child control this program created and still owns.

                ShowWindow(self.edit, SW_HIDE);
            }
        }
        self.layout();
        self.redraw();
    }

    fn ensure_fonts(&mut self) {
        if self.font_scale == self.scale && !self.icon_font.is_invalid() {
            return;
        }
        unsafe {
            // SAFETY: The face name is NUL-terminated and outlives the call. The font is stored and later deleted.

            if !self.font.is_invalid() {
                DeleteObject(self.font);
            }
            if !self.icon_font.is_invalid() {
                DeleteObject(self.icon_font);
            }
            if self.font_resource.is_invalid() {
                static FONT: &[u8] = include_bytes!("../assets/Phosphor.ttf");
                let count = 0u32;
                self.font_resource =
                    AddFontMemResourceEx(FONT.as_ptr().cast(), FONT.len() as u32, None, &count);
            }
        }
        let px = |size: i32| -(size * self.scale as i32 / 100).max(13);
        unsafe {
            // SAFETY: The face name is NUL-terminated and outlives the call. The font is stored and later deleted.

            self.font = CreateFontW(
                px(15),
                0,
                0,
                0,
                FW_SEMIBOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                0,
                w!("Segoe UI"),
            );
            self.icon_font = CreateFontW(
                px(17),
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
                0,
                w!("Phosphor"),
            );
        }
        self.font_scale = self.scale;
    }

    fn layout(&mut self) {
        self.ensure_fonts();
        self.buttons.clear();
        let scale = |value: i32| value * self.scale as i32 / 100;
        let screen = Panel {
            x: 0,
            y: 0,
            w: self.frame.width as i32,
            h: self.frame.height as i32,
        };
        let shown_cancel = !self.editing && self.drag.is_none() && self.current.is_none();
        let Some(selection) = self.selected() else {
            let width = action_width(self, Role::Cancel);
            let height = scale(32);
            self.buttons.push(Button {
                role: Role::Cancel,
                x: screen.w / 2 - width / 2,
                y: screen.h / 2 + scale(48),
                w: width,
                h: height,
                shown: shown_cancel,
                swatch: None,
            });
            return;
        };
        let shown = shown_cancel;
        let mut roles = vec![
            Role::Select,
            Role::Pen,
            Role::Line,
            Role::Arrow,
            Role::Rectangle,
            Role::Marker,
            Role::Text,
            Role::Color,
            Role::Width,
            Role::Undo,
            Role::Redo,
            Role::Delete,
            Role::Copy,
            Role::Save,
            Role::Record,
            Role::Cancel,
        ];
        if !self.dynamic {
            roles.retain(|role| matches!(role, Role::Record | Role::Cancel));
        }
        let tools: Vec<Role> = roles
            .iter()
            .copied()
            .filter(|role| !matches!(role, Role::Copy | Role::Save | Role::Record | Role::Cancel))
            .collect();
        let actions: Vec<Role> = roles
            .iter()
            .copied()
            .filter(|role| matches!(role, Role::Copy | Role::Save | Role::Record | Role::Cancel))
            .collect();
        let gap = scale(2);
        let side = scale(34);
        let action_gap = scale(6);
        let action_h = scale(32);
        let margin = scale(8);
        let extra = if self.palette { 8 } else { 0 };
        let stride = side + gap;
        let rows_fit = ((screen.h - margin * 2 + gap) / stride).max(1);
        let count = tools.len() as i32 + extra;
        let columns = ((count + rows_fit - 1) / rows_fit).max(1);
        let rows = ((count + columns - 1) / columns).max(1);
        let tools_w = columns * side + (columns - 1) * gap;
        let tools_h = rows * side + (rows - 1) * gap;
        let action_widths: Vec<i32> = actions
            .iter()
            .copied()
            .map(|role| action_width(self, role))
            .collect();
        let max_row =
            (screen.w - margin * 2).max(action_widths.iter().copied().max().unwrap_or(side));
        let mut action_rows: Vec<Vec<(Role, i32)>> = Vec::new();
        let mut row: Vec<(Role, i32)> = Vec::new();
        let mut row_w = 0;
        for (role, width) in actions.into_iter().zip(action_widths) {
            let next = if row.is_empty() {
                width
            } else {
                row_w + action_gap + width
            };
            if next > max_row && !row.is_empty() {
                action_rows.push(std::mem::take(&mut row));
                row_w = 0;
            }
            row_w = if row.is_empty() {
                width
            } else {
                row_w + action_gap + width
            };
            row.push((role, width));
        }
        if !row.is_empty() {
            action_rows.push(row);
        }
        let actions_w = action_rows
            .iter()
            .map(|row| {
                row.iter().map(|(_, width)| *width).sum::<i32>()
                    + action_gap * (row.len() as i32 - 1).max(0)
            })
            .max()
            .unwrap_or(0);
        let actions_h = action_rows.len() as i32 * action_h
            + action_gap * (action_rows.len() as i32 - 1).max(0);
        let (tools_rect, actions_rect) = geom::place_panels(
            screen,
            selection.panel(),
            self.dynamic.then_some((tools_w, tools_h.max(side))),
            (actions_w, actions_h),
            margin,
        );
        if let Some(rect) = tools_rect {
            let mut index = 0i32;
            for role in tools {
                let column = index / rows;
                let row_index = index % rows;
                self.buttons.push(Button {
                    role,
                    x: rect.x + column * stride,
                    y: rect.y + row_index * stride,
                    w: side,
                    h: side,
                    shown,
                    swatch: None,
                });
                index += 1;
            }
            if self.palette {
                for swatch in 0..8 {
                    let column = index / rows;
                    let row_index = index % rows;
                    self.buttons.push(Button {
                        role: Role::Swatch,
                        x: rect.x + column * stride,
                        y: rect.y + row_index * stride,
                        w: side,
                        h: side,
                        shown,
                        swatch: Some(swatch as usize),
                    });
                    index += 1;
                }
            }
        }
        for (row_index, row) in action_rows.into_iter().enumerate() {
            let mut x = actions_rect.x;
            let y = actions_rect.y + row_index as i32 * (action_h + action_gap);
            for (role, width) in row {
                self.buttons.push(Button {
                    role,
                    x,
                    y,
                    w: width,
                    h: action_h,
                    shown,
                    swatch: None,
                });
                x += width + action_gap;
            }
        }
    }

    fn redraw(&self) {
        unsafe {
            // SAFETY: The HWND is a window this program created.

            InvalidateRect(self.hwnd, None, false).ok();
        }
    }

    fn ensure_back(&mut self, dc: HDC) {
        if self.back_w == self.frame.width
            && self.back_h == self.frame.height
            && !self.back_dc.is_invalid()
        {
            return;
        }
        unsafe {
            // SAFETY: The GDI object was created by this window and is not selected into a DC.

            if !self.back_bmp.is_invalid() {
                DeleteObject(self.back_bmp);
            }
            if !self.back_dc.is_invalid() {
                DeleteDC(self.back_dc);
            }
            self.back_dc = CreateCompatibleDC(dc);
            self.back_bmp =
                CreateCompatibleBitmap(dc, self.frame.width as i32, self.frame.height as i32);
            SelectObject(self.back_dc, self.back_bmp);
            self.back_w = self.frame.width;
            self.back_h = self.frame.height;
        }
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: These GDI objects were created for this overlay and are no longer selected into a DC.
            if !self.font.is_invalid() {
                DeleteObject(self.font);
            }
            if !self.icon_font.is_invalid() {
                DeleteObject(self.icon_font);
            }
            if !self.edit_font.is_invalid() {
                DeleteObject(self.edit_font);
            }
            if !self.font_resource.is_invalid() {
                RemoveFontMemResourceEx(self.font_resource);
            }
            if !self.back_bmp.is_invalid() {
                DeleteObject(self.back_bmp);
            }
            if !self.back_dc.is_invalid() {
                DeleteDC(self.back_dc);
            }
        }
    }
}

fn region<'a>() -> &'a mut Region {
    unsafe {
        // SAFETY: The POINT is a writable local, and any cursor position is a valid monitor query.
        &mut *REGION
    }
}

fn same_rect(a: Rect, b: Rect) -> bool {
    a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h
}

fn point(lparam: LPARAM) -> Point {
    Point {
        x: (lparam.0 as i16) as i32,
        y: ((lparam.0 >> 16) as i16) as i32,
    }
}

fn cursor_screen() -> (i32, i32) {
    unsafe {
        // SAFETY: The POINT is a writable local, and any cursor position is a valid monitor query.

        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor).ok();
        (cursor.x, cursor.y)
    }
}

fn hit_test(selection: Selection, point: Point) -> Option<Handle> {
    if !selection.usable() {
        return None;
    }
    let near = |a: i32, b: i32| (a - b).abs() <= 8;
    if point.x < selection.x - 8
        || point.x > selection.right() + 8
        || point.y < selection.y - 8
        || point.y > selection.bottom() + 8
    {
        return None;
    }
    let left = near(point.x, selection.x);
    let right = near(point.x, selection.right());
    let top = near(point.y, selection.y);
    let bottom = near(point.y, selection.bottom());
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => Handle::NW,
        (_, true, true, _) => Handle::NE,
        (true, _, _, true) => Handle::SW,
        (_, true, _, true) => Handle::SE,
        (true, _, _, _) => Handle::W,
        (_, true, _, _) => Handle::E,
        (_, _, true, _) => Handle::N,
        (_, _, _, true) => Handle::S,
        _ if point.x >= selection.x
            && point.x < selection.right()
            && point.y >= selection.y
            && point.y < selection.bottom() =>
        {
            Handle::Move
        }
        _ => return None,
    })
}

fn edit_selection(start: Selection, handle: Handle, origin: Point, point: Point) -> Selection {
    let dx = point.x - origin.x;
    let dy = point.y - origin.y;
    if matches!(handle, Handle::Move) {
        return Selection {
            x: start.x + dx,
            y: start.y + dy,
            ..start
        };
    }
    let mut left = start.x;
    let mut right = start.right();
    let mut top = start.y;
    let mut bottom = start.bottom();
    if matches!(handle, Handle::W | Handle::NW | Handle::SW) {
        left += dx;
    }
    if matches!(handle, Handle::E | Handle::NE | Handle::SE) {
        right += dx;
    }
    if matches!(handle, Handle::N | Handle::NW | Handle::NE) {
        top += dy;
    }
    if matches!(handle, Handle::S | Handle::SW | Handle::SE) {
        bottom += dy;
    }
    Selection::new(
        Point { x: left, y: top },
        Point {
            x: right,
            y: bottom,
        },
    )
}

fn monitor_rects() -> Vec<Rect> {
    // SAFETY: The enumerator calls this once per monitor. The Vec behind data stays alive until EnumDisplayMonitors returns.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe {
            // SAFETY: The callback's Vec stays alive until the enumeration returns.
            GetMonitorInfoW(monitor, &mut info)
        }
        .as_bool()
        {
            let rect = info.rcMonitor;
            unsafe {
                // SAFETY: The callback's Vec stays alive until the enumeration returns.
                &mut *(data.0 as *mut Vec<Rect>)
            }
            .push(Rect {
                x: rect.left,
                y: rect.top,
                w: (rect.right - rect.left) as u32,
                h: (rect.bottom - rect.top) as u32,
            });
        }
        true.into()
    }
    let mut result = Vec::new();
    unsafe {
        // SAFETY: The callback's Vec stays alive until the enumeration returns.

        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut result as *mut _ as isize),
        )
    }
    .ok();
    result
}

fn save_dialog() -> Option<PathBuf> {
    let mut path = vec![0u16; 32768];
    let filter = "PNG image\0*.png\0WebP image\0*.webp\0\0"
        .encode_utf16()
        .collect::<Vec<_>>();
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: region().hwnd,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        lpstrDefExt: w!("png"),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe {
        // SAFETY: The path buffer and filter outlive the call, and lStructSize matches the struct.
        GetSaveFileNameW(&mut dialog)
    }
    .as_bool()
    {
        let len = path
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(path.len());
        Some(PathBuf::from(String::from_utf16_lossy(&path[..len])))
    } else {
        None
    }
}

fn text_commit_box(edit_pos: Point, edit_w: i32, format: RECT, margins: i32) -> (Point, i32) {
    let left_margin = margins & 0xffff;
    let right_margin = (margins as u32 >> 16) as i32;
    let left = format.left.max(left_margin);
    let right_limit = if right_margin > 0 {
        edit_w - right_margin
    } else {
        format.right
    };
    let right = format.right.min(right_limit).max(left + 1);
    (
        Point {
            x: edit_pos.x + left,
            y: edit_pos.y + format.top,
        },
        right - left,
    )
}

fn message(text: &str) {
    unsafe {
        // SAFETY: The caller passes the editor paint DC. The fill uses the process-owned chroma brush.

        MessageBoxW(
            region().hwnd,
            PCWSTR(crate::wide(text).as_ptr()),
            w!("ezcap"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn chroma_brush() -> HBRUSH {
    static BRUSH: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
    HBRUSH(*BRUSH.get_or_init(|| unsafe {
        // SAFETY: The caller passes the editor paint DC. The fill uses the process-owned chroma brush.
        CreateSolidBrush(theme::CHROMA).0 as isize
    }) as *mut std::ffi::c_void)
}

// SAFETY: The caller passes the editor paint DC. The fill uses the process-owned chroma brush.
unsafe fn erase_edit_background(dc: HDC) -> LRESULT {
    let region = region();
    let rect = RECT {
        left: 0,
        top: 0,
        right: region.edit_w.max(1),
        bottom: region.edit_h.max(1),
    };
    FillRect(dc, &rect, chroma_brush());
    LRESULT(1)
}

fn edit_edge_handle(point: Point, width: i32, height: i32) -> Option<Handle> {
    let near = |value: i32, edge: i32| (value - edge).abs() <= 8;
    let left = near(point.x, 0);
    let right = near(point.x, width);
    let top = near(point.y, 0);
    let bottom = near(point.y, height);
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => Handle::NW,
        (_, true, true, _) => Handle::NE,
        (true, _, _, true) => Handle::SW,
        (_, true, _, true) => Handle::SE,
        (true, _, _, _) => Handle::W,
        (_, true, _, _) => Handle::E,
        (_, _, true, _) => Handle::N,
        (_, _, _, true) => Handle::S,
        _ => return None,
    })
}

fn move_edit(current: Point) {
    let Some(drag) = region().edit_drag else {
        return;
    };
    let delta = geom::drag_delta(
        drag.start_screen,
        cursor_screen(),
        (drag.start_client.x, drag.start_client.y),
        (current.x, current.y),
    );
    let region = region();
    region.edit_pos = Point {
        x: drag.origin.x + delta.0,
        y: drag.origin.y + delta.1,
    };
    region.place_edit();
}

fn draw_edit_chrome(dc: HDC, width: i32, height: i32) {
    unsafe {
        // SAFETY: The color is a plain COLORREF. The brush is stored for the process or deleted in this block.

        let pen = CreatePen(PS_SOLID, 3, theme::ACCENT);
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        RoundRect(dc, 1, 1, width - 1, height - 1, 10, 10);
        let fill = CreateSolidBrush(theme::WHITE);
        let edge = CreatePen(PS_SOLID, 1, theme::ACCENT);
        SelectObject(dc, fill);
        SelectObject(dc, edge);
        for (x, y) in [
            (0, 0),
            (width, 0),
            (0, height),
            (width, height),
            (width / 2, 0),
            (width / 2, height),
            (0, height / 2),
            (width, height / 2),
        ] {
            Ellipse(dc, x - 5, y - 5, x + 5, y + 5);
        }
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen);
        DeleteObject(fill);
        DeleteObject(edge);
    }
}

// SAFETY: Windows calls this for the text editor this program created. The saved procedure is called only while it is still installed.
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if REGION.is_null() || EDIT_PROC == 0 {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_ERASEBKGND => erase_edit_background(HDC(wparam.0 as *mut std::ffi::c_void)),
        WM_PAINT | 0x0317 | 0x0318 => {
            let result = CallWindowProcW(
                std::mem::transmute::<isize, WNDPROC>(EDIT_PROC),
                hwnd,
                message,
                wparam,
                lparam,
            );
            let dc = if message == WM_PAINT {
                GetDC(hwnd)
            } else {
                HDC(wparam.0 as *mut std::ffi::c_void)
            };
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).ok();
            draw_edit_chrome(dc, rect.right, rect.bottom);
            if message == WM_PAINT {
                ReleaseDC(hwnd, dc);
            }
            result
        }
        WM_MBUTTONDOWN => {
            let local = point(lparam);
            let mut parent_point = POINT {
                x: local.x,
                y: local.y,
            };
            ClientToScreen(hwnd, &mut parent_point).ok();
            ScreenToClient(region().hwnd, &mut parent_point).ok();
            region().begin_text_move(Point {
                x: parent_point.x,
                y: parent_point.y,
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let local = point(lparam);
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).ok();
            if message == WM_LBUTTONDOWN {
                if let Some(handle) = edit_edge_handle(local, rect.right, rect.bottom) {
                    let mut parent_point = POINT {
                        x: local.x,
                        y: local.y,
                    };
                    ClientToScreen(hwnd, &mut parent_point).ok();
                    ScreenToClient(region().hwnd, &mut parent_point).ok();
                    let region = region();
                    region.drag = Some(Drag::TextSize(
                        handle,
                        Point {
                            x: parent_point.x,
                            y: parent_point.y,
                        },
                        region.edit_pos,
                        region.edit_w,
                        region.edit_h,
                    ));
                    SetCapture(region.hwnd);
                    return LRESULT(0);
                }
            }
            CallWindowProcW(
                std::mem::transmute::<isize, WNDPROC>(EDIT_PROC),
                hwnd,
                message,
                wparam,
                lparam,
            )
        }
        WM_MOUSEMOVE => {
            if region().edit_drag.is_some() {
                move_edit(point(lparam));
                LRESULT(0)
            } else {
                CallWindowProcW(
                    std::mem::transmute::<isize, WNDPROC>(EDIT_PROC),
                    hwnd,
                    message,
                    wparam,
                    lparam,
                )
            }
        }
        WM_LBUTTONUP => {
            if region().edit_drag.is_some() {
                move_edit(point(lparam));
                region().edit_drag = None;
                ReleaseCapture().ok();
                LRESULT(0)
            } else {
                CallWindowProcW(
                    std::mem::transmute::<isize, WNDPROC>(EDIT_PROC),
                    hwnd,
                    message,
                    wparam,
                    lparam,
                )
            }
        }
        WM_KEYDOWN if wparam.0 == VK_RETURN.0 as usize && GetKeyState(VK_SHIFT.0 as i32) >= 0 => {
            region().commit_text();
            SetFocus(region().hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
            region().cancel_text();
            SetFocus(region().hwnd);
            LRESULT(0)
        }
        _ => CallWindowProcW(
            std::mem::transmute::<isize, WNDPROC>(EDIT_PROC),
            hwnd,
            message,
            wparam,
            lparam,
        ),
    }
}

// SAFETY: Windows calls this with a window of this class. Paint uses the DC from BeginPaint, and every other message is forwarded to DefWindowProc.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if REGION.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CTLCOLOREDIT => {
            let dc = HDC(wparam.0 as *mut std::ffi::c_void);
            SetBkColor(dc, theme::CHROMA);
            SetTextColor(dc, region().color);
            LRESULT(chroma_brush().0 as isize)
        }
        WM_KEYDOWN => {
            region().key_down(wparam.0 as u32);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            region().mouse_down(point(lparam), false);
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            region().mouse_down(point(lparam), true);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            region().mouse_move(point(lparam));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            region().mouse_up(point(lparam));
            LRESULT(0)
        }
        WM_MBUTTONDOWN => {
            let cursor = point(lparam);
            if region().over_text_box(cursor) {
                region().begin_text_move(cursor);
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            if matches!(region().drag, Some(Drag::TextMove(_, _))) {
                region().mouse_up(point(lparam));
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            region().finish(Outcome::Cancel);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn paint(hwnd: HWND) {
    let region = region();
    let mut ps = PAINTSTRUCT::default();
    unsafe {
        // SAFETY: Windows is painting this window. EndPaint receives the same paint struct, and brushes created here are deleted.
        let dc = BeginPaint(hwnd, &mut ps);
        region.ensure_back(dc);
        let memory = region.back_dc;
        blit_frame(memory, &region.frame);
        draw_dim(memory, region);
        for shape in region.shapes.iter().chain(region.current.iter()) {
            draw_shape(memory, shape, Point { x: 0, y: 0 });
        }
        if let Some(selection) = region.selected() {
            let pen = CreatePen(PS_SOLID, 2, theme::ACCENT);
            let old_pen = SelectObject(memory, pen);
            let old_brush = SelectObject(memory, GetStockObject(NULL_BRUSH));
            Rectangle(
                memory,
                selection.x,
                selection.y,
                selection.right(),
                selection.bottom(),
            );
            SelectObject(memory, old_pen);
            SelectObject(memory, old_brush);
            DeleteObject(pen);
            if region.tool == Tool::Select {
                let brush = CreateSolidBrush(theme::ACCENT);
                for (x, y) in handles(selection) {
                    let rect = RECT {
                        left: x - 5,
                        top: y - 5,
                        right: x + 5,
                        bottom: y + 5,
                    };
                    FillRect(memory, &rect, brush);
                }
                DeleteObject(brush);
            }
            draw_badge(memory, region, selection);
        } else if region.buttons.iter().all(|button| !button.shown) || region.drag.is_none() {
            draw_hint(memory, region);
        }
        for button in &region.buttons {
            if button.shown {
                draw_button(memory, region, button);
            }
        }
        if region.editing {
            draw_text_frame(memory, region);
        }
        BitBlt(
            dc,
            0,
            0,
            region.frame.width as i32,
            region.frame.height as i32,
            memory,
            0,
            0,
            SRCCOPY,
        )
        .ok();
        EndPaint(hwnd, &ps);
    }
}

fn draw_text_frame(dc: HDC, region: &Region) {
    let frame = region.text_outer();
    unsafe {
        // SAFETY: The color is a plain COLORREF. The brush is stored for the process or deleted in this block.

        let pen = CreatePen(PS_SOLID, 2, theme::ACCENT);
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        RoundRect(dc, frame.x, frame.y, frame.right(), frame.bottom(), 12, 12);
        let fill = CreateSolidBrush(theme::WHITE);
        let edge = CreatePen(PS_SOLID, 1, theme::ACCENT);
        SelectObject(dc, fill);
        SelectObject(dc, edge);
        for (x, y) in [
            (frame.x, frame.y),
            (frame.right(), frame.y),
            (frame.x, frame.bottom()),
            (frame.right(), frame.bottom()),
            (frame.x + frame.w / 2, frame.y),
            (frame.x + frame.w / 2, frame.bottom()),
            (frame.x, frame.y + frame.h / 2),
            (frame.right(), frame.y + frame.h / 2),
        ] {
            Rectangle(dc, x - 4, y - 4, x + 4, y + 4);
        }
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen);
        DeleteObject(fill);
        DeleteObject(edge);
    }
}

fn blit_frame(dc: HDC, frame: &Frame) {
    unsafe {
        // SAFETY: Every field the API reads, including the size, is written before the struct is used.

        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = frame.width as i32;
        info.bmiHeader.biHeight = -(frame.height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        SetDIBitsToDevice(
            dc,
            0,
            0,
            frame.width,
            frame.height,
            0,
            0,
            0,
            frame.height,
            frame.data.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
        );
    }
}

fn draw_dim(dc: HDC, region: &Region) {
    let width = region.frame.width as i32;
    let height = region.frame.height as i32;
    let selection = region.selection.unwrap_or(Selection {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    });
    let rects = [
        (0, 0, width, selection.y),
        (0, selection.bottom(), width, height - selection.bottom()),
        (0, selection.y, selection.x, selection.h),
        (
            selection.right(),
            selection.y,
            width - selection.right(),
            selection.h,
        ),
    ];
    unsafe {
        // SAFETY: dc is the overlay paint DC. The temporary DC and bitmap are deleted before return.
        let dim = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, 1, 1);
        let old = SelectObject(dim, bitmap);
        SetPixelV(dim, 0, 0, theme::BLACK);
        for (x, y, w, h) in rects {
            if w > 0 && h > 0 {
                GdiAlphaBlend(
                    dc,
                    x,
                    y,
                    w,
                    h,
                    dim,
                    0,
                    0,
                    1,
                    1,
                    BLENDFUNCTION {
                        BlendOp: AC_SRC_OVER as u8,
                        BlendFlags: 0,
                        SourceConstantAlpha: 120,
                        AlphaFormat: 0,
                    },
                );
            }
        }
        SelectObject(dim, old);
        DeleteObject(bitmap);
        DeleteDC(dim);
    }
}

fn handles(selection: Selection) -> [(i32, i32); 8] {
    [
        (selection.x, selection.y),
        (selection.right(), selection.y),
        (selection.x, selection.bottom()),
        (selection.right(), selection.bottom()),
        (selection.x + selection.w / 2, selection.y),
        (selection.x + selection.w / 2, selection.bottom()),
        (selection.x, selection.y + selection.h / 2),
        (selection.right(), selection.y + selection.h / 2),
    ]
}

fn draw_badge(dc: HDC, region: &Region, selection: Selection) {
    let text = format!("{} × {}", selection.w, selection.h);
    let wide = crate::wide(&text);
    unsafe {
        // SAFETY: The DC belongs to a window this program is painting, and created GDI objects are deleted before return.

        let old = SelectObject(dc, region.font);
        let mut size = SIZE::default();
        GetTextExtentPoint32W(dc, &wide, &mut size).ok();
        let mut blocked: Vec<Panel> = region
            .buttons
            .iter()
            .filter(|button| button.shown)
            .map(|button| Panel {
                x: button.x,
                y: button.y,
                w: button.w,
                h: button.h,
            })
            .collect();
        if region.editing {
            blocked.push(region.text_outer().panel());
        }
        let screen = Panel {
            x: 0,
            y: 0,
            w: region.frame.width as i32,
            h: region.frame.height as i32,
        };
        let badge = geom::place_badge(
            screen,
            selection.panel(),
            (size.cx + 16, size.cy + 8),
            &blocked,
        );
        let x = badge.x;
        let y = badge.y;
        let rect = RECT {
            left: x,
            top: y,
            right: x + size.cx + 16,
            bottom: y + size.cy + 8,
        };
        let brush = CreateSolidBrush(theme::BADGE);
        let pen = CreatePen(PS_SOLID, 1, theme::BADGE);
        SelectObject(dc, brush);
        SelectObject(dc, pen);
        RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, 12, 12);
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, theme::BADGE_TEXT);
        TextOutW(dc, x + 8, y + 4, &wide);
        SelectObject(dc, old);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

fn draw_hint(dc: HDC, region: &Region) {
    let mut text = crate::wide("Drag to select an area    ·    Double click: full screen    ·    Tab: next display    ·    Esc: cancel");
    unsafe {
        // SAFETY: The DC belongs to a window this program is painting, and created GDI objects are deleted before return.

        let old = SelectObject(dc, region.font);
        SetBkMode(dc, TRANSPARENT);
        let mut rect = RECT {
            left: 40,
            top: 0,
            right: region.frame.width as i32 - 40,
            bottom: region.frame.height as i32,
        };
        SetTextColor(dc, theme::HINT_SHADOW);
        let mut shadow = rect;
        shadow.left += 1;
        shadow.top += 1;
        DrawTextW(
            dc,
            &mut text,
            &mut shadow,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SetTextColor(dc, theme::WHITE);
        DrawTextW(
            dc,
            &mut text,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old);
    }
}

fn measure(dc: HDC, font: HFONT, text: &str) -> i32 {
    unsafe {
        // SAFETY: dc is the overlay paint DC passed in, and the previous font is restored before return.
        let old = SelectObject(dc, font);
        let wide = crate::wide(text);
        let mut size = SIZE::default();
        GetTextExtentPoint32W(dc, &wide, &mut size).ok();
        SelectObject(dc, old);
        size.cx
    }
}

fn action_metrics(region: &Region, role: Role) -> (i32, i32, i32, i32) {
    let pad = (8 * region.scale as i32 / 100).max(6);
    let gap = (8 * region.scale as i32 / 100).max(6);
    let Some(caption) = caption(role) else {
        return (pad, 0, 0, 0);
    };
    let Some(glyph) = icon(role) else {
        return (pad, 0, 0, 0);
    };
    unsafe {
        // SAFETY: This DC was obtained with GetDC in the same call and is released once.

        let dc = GetDC(None);
        let icon_w =
            measure(dc, region.icon_font, glyph).max((16 * region.scale as i32 / 100).max(14));
        let text_w = measure(dc, region.font, caption);
        ReleaseDC(None, dc);
        (pad, icon_w, gap, text_w)
    }
}

fn action_width(region: &Region, role: Role) -> i32 {
    let (pad, icon_w, gap, text_w) = action_metrics(region, role);
    pad + icon_w + gap + text_w + pad
}

/// Phosphor Regular, the same glyphs the original overlay uses.
fn icon(role: Role) -> Option<&'static str> {
    Some(match role {
        Role::Select => "\u{E1DC}",
        Role::Pen => "\u{E3B4}",
        Role::Line => "\u{E6D2}",
        Role::Arrow => "\u{E092}",
        Role::Rectangle => "\u{E3F0}",
        Role::Marker => "\u{EC76}",
        Role::Text => "\u{E48A}",
        Role::Undo => "\u{E038}",
        Role::Redo => "\u{E036}",
        Role::Delete => "\u{E4A6}",
        Role::Copy => "\u{E1CA}",
        Role::Save => "\u{E248}",
        Role::Record => "\u{E69A}",
        Role::Cancel => "\u{E4F6}",
        _ => return None,
    })
}

fn caption(role: Role) -> Option<&'static str> {
    match role {
        Role::Copy => Some("Copy"),
        Role::Save => Some("Save…"),
        Role::Record => Some("Record"),
        Role::Cancel => Some("Cancel"),
        _ => None,
    }
}

fn draw_button(dc: HDC, region: &Region, button: &Button) {
    let selected = matches!(
        (button.role, region.tool),
        (Role::Select, Tool::Select)
            | (Role::Pen, Tool::Pen)
            | (Role::Line, Tool::Line)
            | (Role::Arrow, Tool::Arrow)
            | (Role::Rectangle, Tool::Rectangle)
            | (Role::Marker, Tool::Marker)
            | (Role::Text, Tool::Text)
    );
    let enabled = region.role_enabled(button.role);
    let primary = selected || button.role == Role::Record;
    let fill = if let Some(swatch) = button.swatch {
        theme::SWATCHES[swatch]
    } else if !enabled {
        theme::OVERLAY_DISABLED
    } else if primary {
        theme::ACCENT
    } else {
        theme::OVERLAY
    };
    let ink = if !enabled {
        theme::OVERLAY_DISABLED_INK
    } else if primary {
        theme::ON_ACCENT
    } else {
        theme::OVERLAY_INK
    };
    unsafe {
        // SAFETY: The color is a plain COLORREF. The brush is stored for the process or deleted in this block.

        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(
            PS_SOLID,
            1,
            if selected {
                theme::ACCENT_DEEP
            } else {
                theme::OVERLAY_EDGE
            },
        );
        let old_brush = SelectObject(dc, brush);
        let old_pen = SelectObject(dc, pen);
        let radius = (button.h / 4).max(6);
        RoundRect(
            dc,
            button.x,
            button.y,
            button.x + button.w,
            button.y + button.h,
            radius,
            radius,
        );
        if button.role == Role::Color {
            let dot = CreateSolidBrush(region.color);
            let ring = CreatePen(PS_SOLID, 2, theme::WHITE);
            SelectObject(dc, dot);
            SelectObject(dc, ring);
            let cx = button.x + button.w / 2;
            let cy = button.y + button.h / 2;
            Ellipse(dc, cx - 8, cy - 8, cx + 8, cy + 8);
            DeleteObject(dot);
            DeleteObject(ring);
        } else if button.role == Role::Width {
            let stroke = CreatePen(PS_SOLID, region.width.max(1), ink);
            SelectObject(dc, stroke);
            MoveToEx(dc, button.x + 8, button.y + button.h / 2, None).ok();
            LineTo(dc, button.x + button.w - 8, button.y + button.h / 2).ok();
            DeleteObject(stroke);
        } else if let Some(swatch) = button.swatch {
            if theme::SWATCHES[swatch] == region.color {
                let ring = CreatePen(PS_SOLID, 2, theme::WHITE);
                SelectObject(dc, ring);
                SelectObject(dc, GetStockObject(NULL_BRUSH));
                RoundRect(
                    dc,
                    button.x + 3,
                    button.y + 2,
                    button.x + button.w - 3,
                    button.y + button.h - 2,
                    8,
                    8,
                );
                DeleteObject(ring);
            }
        } else if let Some(glyph) = icon(button.role) {
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, ink);
            let old = SelectObject(dc, region.icon_font);
            let mut glyph = crate::wide(glyph);
            if let Some(caption) = caption(button.role) {
                let (pad, icon_w, gap, _) = action_metrics(region, button.role);
                let mut icon_rect = RECT {
                    left: button.x + pad,
                    top: button.y,
                    right: button.x + pad + icon_w,
                    bottom: button.y + button.h,
                };
                DrawTextW(
                    dc,
                    &mut glyph,
                    &mut icon_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(dc, region.font);
                let mut text = crate::wide(caption);
                let mut text_rect = RECT {
                    left: icon_rect.right + gap,
                    top: button.y,
                    right: button.x + button.w - pad,
                    bottom: button.y + button.h,
                };
                DrawTextW(
                    dc,
                    &mut text,
                    &mut text_rect,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            } else {
                let mut rect = RECT {
                    left: button.x,
                    top: button.y,
                    right: button.x + button.w,
                    bottom: button.y + button.h,
                };
                DrawTextW(
                    dc,
                    &mut glyph,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            SelectObject(dc, old);
        }
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

fn draw_annotations(frame: &mut Frame, shapes: &[Shape], offset: Point) {
    unsafe {
        // SAFETY: Every field the API reads, including the size, is written before the struct is used.

        let screen = GetDC(None);
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = frame.width as i32;
        info.bmiHeader.biHeight = -(frame.height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let old = SelectObject(dc, bitmap);
        std::ptr::copy_nonoverlapping(frame.data.as_ptr(), bits.cast::<u8>(), frame.data.len());
        for shape in shapes {
            draw_shape(dc, shape, offset);
        }
        GdiFlush();
        std::ptr::copy_nonoverlapping(bits.cast::<u8>(), frame.data.as_mut_ptr(), frame.data.len());
        // GDI writes the stroke color and clears the alpha byte, so a red line becomes a transparent hole in PNG.
        for pixel in frame.data.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(None, screen);
    }
}

fn draw_shape(dc: HDC, shape: &Shape, offset: Point) {
    unsafe {
        // SAFETY: The color is a plain COLORREF. The brush is stored for the process or deleted in this block.

        let color = if matches!(shape.kind, ShapeKind::Marker(_)) {
            theme::MARKER
        } else {
            shape.color
        };
        let width = if matches!(shape.kind, ShapeKind::Marker(_)) {
            shape.width * 4
        } else {
            shape.width
        };
        SetBkMode(dc, TRANSPARENT);
        let pen = CreatePen(PS_SOLID, width, color);
        let old_pen = SelectObject(dc, pen);
        let brush = CreateSolidBrush(color);
        let old_brush = SelectObject(dc, brush);
        let p = |point: Point| POINT {
            x: point.x + offset.x,
            y: point.y + offset.y,
        };
        match &shape.kind {
            ShapeKind::Pen(points) | ShapeKind::Marker(points) => {
                if points.len() == 1 {
                    let point = p(points[0]);
                    Ellipse(
                        dc,
                        point.x - width / 2,
                        point.y - width / 2,
                        point.x + width / 2,
                        point.y + width / 2,
                    );
                }
                let points: Vec<POINT> = points.iter().copied().map(p).collect();
                Polyline(dc, &points).ok();
            }
            ShapeKind::Line(from, to) => {
                let a = p(*from);
                let b = p(*to);
                MoveToEx(dc, a.x, a.y, None).ok();
                LineTo(dc, b.x, b.y).ok();
            }
            ShapeKind::Arrow(from, to) => {
                let a = p(*from);
                let b = p(*to);
                MoveToEx(dc, a.x, a.y, None).ok();
                LineTo(dc, b.x, b.y).ok();
                let dx = (b.x - a.x) as f32;
                let dy = (b.y - a.y) as f32;
                let length = dx.hypot(dy).max(1.0);
                let ux = dx / length;
                let uy = dy / length;
                let size = (shape.width * 4).max(12) as f32;
                let left = POINT {
                    x: b.x - (ux * size - uy * size * 0.55) as i32,
                    y: b.y - (uy * size + ux * size * 0.55) as i32,
                };
                let right = POINT {
                    x: b.x - (ux * size + uy * size * 0.55) as i32,
                    y: b.y - (uy * size - ux * size * 0.55) as i32,
                };
                Polygon(dc, &[left, b, right]);
            }
            ShapeKind::Rectangle(from, to) => {
                SelectObject(dc, GetStockObject(NULL_BRUSH));
                let a = p(*from);
                let b = p(*to);
                Rectangle(dc, a.x, a.y, b.x, b.y);
            }
            ShapeKind::Text(pos, wrap, text) => {
                let pos = p(*pos);
                let font = CreateFontW(
                    -shape.font_size.max(18),
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
                    0,
                    PCWSTR(crate::wide(&shape.font_name).as_ptr()),
                );
                let old_font = SelectObject(dc, font);
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, shape.color);
                let mut wide = crate::wide(text);
                let mut rect = RECT {
                    left: pos.x,
                    top: pos.y,
                    right: pos.x + (*wrap).max(1),
                    bottom: pos.y + 4000,
                };
                DrawTextW(dc, &mut wide, &mut rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
                SelectObject(dc, old_font);
                DeleteObject(font);
            }
        }
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

pub fn register(instance: HINSTANCE) {
    unsafe {
        // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
    }
}

pub fn open(profile: Profile, dynamic: bool, allow_record: bool) -> Result<(), String> {
    unsafe {
        // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

        if !REGION.is_null() {
            return Err("Area selection is already active".into());
        }
        let monitor = media::cursor_monitor_rect();
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("ezcap area"),
            WS_POPUP,
            monitor.x,
            monitor.y,
            monitor.w as i32,
            monitor.h as i32,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .map_err(|error| error.to_string())?;
        REGION = Box::into_raw(Region::new(hwnd, profile, dynamic, allow_record)?);
        region().show();
        Ok(())
    }
}

#[cfg(test)]
pub fn open_with_frame(frame: Frame, profile: Profile, scale: u32) -> Result<(), String> {
    unsafe {
        // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

        if !REGION.is_null() {
            shutdown();
        }
        let monitor = Rect {
            x: 0,
            y: 0,
            w: frame.width,
            h: frame.height,
        };
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("ezcap area"),
            WS_POPUP,
            0,
            0,
            frame.width as i32,
            frame.height as i32,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .map_err(|error| error.to_string())?;
        REGION = Box::into_raw(Region::with_frame(
            hwnd, profile, true, true, frame, monitor,
        ));
        region().scale = scale;
        region().selection = Some(Selection {
            x: 96,
            y: 72,
            w: 360,
            h: 260,
        });
        region().shapes.push(Shape {
            kind: ShapeKind::Arrow(Point { x: 120, y: 100 }, Point { x: 300, y: 220 }),
            color: theme::SWATCHES[0],
            width: 6,
            font_size: 24,
            font_name: "Segoe UI".into(),
        });
        region().layout();
        region().show();
        Ok(())
    }
}

#[cfg(test)]
pub fn test_hwnd() -> HWND {
    region().hwnd
}

#[cfg(test)]
pub fn text_edit() -> HWND {
    region().edit
}

#[cfg(test)]
pub fn selection_screen_rect() -> RECT {
    let region = region();
    let selection = region.selected().expect("selection");
    let mut origin = POINT {
        x: selection.x,
        y: selection.y,
    };
    unsafe {
        // SAFETY: The POINT is in client coordinates of a window this program created.

        ClientToScreen(region.hwnd, &mut origin).ok();
    }
    RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + selection.w,
        bottom: origin.y + selection.h,
    }
}

#[cfg(test)]
pub fn composited() -> Option<Frame> {
    region().crop()
}

#[cfg(test)]
pub fn demonstrate_tools() {
    let region = region();
    let ink = |kind| Shape {
        kind,
        color: theme::SWATCHES[0],
        width: 5,
        font_size: 32,
        font_name: "Segoe UI".into(),
    };
    region.shapes.push(ink(ShapeKind::Pen(vec![
        Point { x: 130, y: 110 },
        Point { x: 190, y: 160 },
        Point { x: 170, y: 210 },
    ])));
    region.shapes.push(ink(ShapeKind::Line(
        Point { x: 150, y: 280 },
        Point { x: 420, y: 110 },
    )));
    region.shapes.push(ink(ShapeKind::Rectangle(
        Point { x: 220, y: 100 },
        Point { x: 360, y: 200 },
    )));
    region.shapes.push(Shape {
        kind: ShapeKind::Marker(vec![Point { x: 160, y: 250 }, Point { x: 400, y: 250 }]),
        color: theme::MARKER,
        width: 6,
        font_size: 32,
        font_name: "Segoe UI".into(),
    });
}

#[cfg(test)]
pub fn chrome_screen_rect(role: Role) -> Option<RECT> {
    let region = region();
    let button = region
        .buttons
        .iter()
        .find(|button| button.role == role && button.shown)?;
    let mut point = POINT {
        x: button.x,
        y: button.y,
    };
    unsafe {
        // SAFETY: The POINT is in client coordinates of a window this program created.

        ClientToScreen(region.hwnd, &mut point).ok();
    }
    Some(RECT {
        left: point.x,
        top: point.y,
        right: point.x + button.w,
        bottom: point.y + button.h,
    })
}

#[cfg(test)]
pub fn select_tool(role: Role) {
    region().select_role(role);
}

pub fn active() -> bool {
    // SAFETY: This only reads the pointer. The region is used only when it is non-null.
    unsafe { !REGION.is_null() }
}

#[cfg(test)]
pub fn select_sample() {
    region().selection = Some(Selection {
        x: 40,
        y: 40,
        w: 220,
        h: 140,
    });
    region().layout();
}

#[cfg(test)]
pub fn set_record_allowed(allowed: bool) {
    region().allow_record = allowed;
    region().redraw();
}

#[cfg(test)]
pub fn press_role(role: Role) {
    let Some(index) = region()
        .buttons
        .iter()
        .position(|button| button.role == role && button.shown)
    else {
        return;
    };
    region().press(index, Point { x: 0, y: 0 });
}

#[cfg(test)]
pub fn has_outcome() -> bool {
    region().outcome.is_some()
}

pub fn take_outcome() -> Option<Outcome> {
    let outcome = region().outcome.take();
    if outcome.is_some() {
        shutdown();
    }
    outcome
}

pub fn cancel() {
    unsafe {
        // SAFETY: Windows calls this only for the short-lived probe edit. Every message is forwarded to DefWindowProc.

        if !REGION.is_null() {
            region().finish(Outcome::Cancel);
        }
    }
}

pub fn shutdown() {
    unsafe {
        // SAFETY: Windows calls this only for the short-lived probe edit. Every message is forwarded to DefWindowProc.

        if !REGION.is_null() {
            let region = Box::from_raw(REGION);
            if !region.edit.is_invalid() {
                DestroyWindow(region.edit).ok();
            }
            DestroyWindow(region.hwnd).ok();
            REGION = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAFETY: Windows calls this only for the short-lived probe edit. Every message is forwarded to DefWindowProc.
    unsafe extern "system" fn probe_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    #[test]
    fn edit_text_inset_is_stable() {
        unsafe {
            // SAFETY: The class name lives for the process. The new HWND is stored and destroyed with its owner.

            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("ezcap.edit.probe");
            let window = WNDCLASSW {
                lpfnWndProc: Some(probe_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&window);
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("probe"),
                WS_POPUP,
                0,
                0,
                400,
                300,
                None,
                None,
                instance,
                None,
            )
            .unwrap();
            let style = WS_CHILD | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32);
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!("Hello"),
                style,
                10,
                20,
                240,
                80,
                parent,
                None,
                instance,
                None,
            )
            .unwrap();
            let mut before = RECT::default();
            SendMessageW(
                edit,
                0x00B2,
                WPARAM(0),
                LPARAM(&mut before as *mut RECT as isize),
            );
            let font = CreateFontW(
                -28,
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
                0,
                w!("Segoe UI"),
            );
            SendMessageW(edit, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            ShowWindow(parent, SW_SHOWNA);
            ShowWindow(edit, SW_SHOW);
            UpdateWindow(edit);
            let dc = GetDC(edit);
            let mut ink = None;
            for y in 0..40 {
                for x in 0..80 {
                    let pixel = GetPixel(dc, x, y);
                    let r = pixel.0 & 255;
                    if r < 40 {
                        ink = Some((x, y));
                        break;
                    }
                }
                if ink.is_some() {
                    break;
                }
            }
            let mem = CreateCompatibleDC(dc);
            let bmp = CreateCompatibleBitmap(dc, 240, 80);
            SelectObject(mem, bmp);
            let white = CreateSolidBrush(theme::WHITE);
            let fill = RECT {
                left: 0,
                top: 0,
                right: 240,
                bottom: 80,
            };
            FillRect(mem, &fill, white);
            DeleteObject(white);
            let old = SelectObject(mem, font);
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, theme::BLACK);
            let mut drawn = RECT {
                left: 0,
                top: 0,
                right: 240,
                bottom: 80,
            };
            let mut wide = crate::wide("Hello");
            DrawTextW(mem, &mut wide, &mut drawn, DT_LEFT | DT_TOP);
            let mut drawn_ink = None;
            for y in 0..40 {
                for x in 0..80 {
                    let pixel = GetPixel(mem, x, y);
                    if pixel.0 & 255 < 40 {
                        drawn_ink = Some((x, y));
                        break;
                    }
                }
                if drawn_ink.is_some() {
                    break;
                }
            }
            let margins = SendMessageW(edit, 0x00D4, WPARAM(0), LPARAM(0)).0 as i32;
            let (origin, wrap) = text_commit_box(Point { x: 10, y: 20 }, 240, before, margins);
            SelectObject(mem, old);
            DeleteObject(bmp);
            DeleteDC(mem);
            ReleaseDC(edit, dc);
            DeleteObject(font);
            DestroyWindow(edit).ok();
            DestroyWindow(parent).ok();
            let (edit_at, drawn_at) = (ink.expect("edit glyph"), drawn_ink.expect("drawn glyph"));
            assert_eq!(
                edit_at.0 - drawn_at.0,
                margins & 0xffff,
                "the edit's left margin is the horizontal jump"
            );
            assert_eq!(edit_at.1, drawn_at.1, "the edit and DrawText share a top");
            assert_eq!(
                origin,
                Point {
                    x: 10 + (margins & 0xffff),
                    y: 20
                }
            );
            assert_eq!(wrap, 240 - (margins & 0xffff) - (margins >> 16));
        }
    }

    #[test]
    fn annotations_are_composited_into_the_frame() {
        let mut frame = Frame {
            width: 8,
            height: 8,
            data: vec![255; 8 * 8 * 4],
        };
        let shape = Shape {
            kind: ShapeKind::Line(Point { x: 0, y: 0 }, Point { x: 7, y: 7 }),
            color: theme::SWATCHES[0],
            width: 2,
            font_size: 12,
            font_name: "Segoe UI".into(),
        };
        draw_annotations(&mut frame, &[shape], Point { x: 0, y: 0 });
        let pixel = &frame.data[(3 * 8 + 3) * 4..(3 * 8 + 3) * 4 + 4];
        assert!(
            pixel[0] < 80 && pixel[1] < 80 && pixel[2] > 200,
            "{pixel:?}"
        );
        assert_eq!(
            pixel[3], 255,
            "the stroke was stored with zero alpha and would punch a hole: {pixel:?}"
        );
    }
}
