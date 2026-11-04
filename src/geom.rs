//! Pure layout, capture-policy and codec-graph decisions.
//! Nothing in here opens a device, a window, or a timer.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Selecting,
    Recording,
    Saving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayClick {
    Stop,
    OpenRegion,
    Show,
    Ignore,
}

/// Left click on the tray icon. Recording always stops; nothing else is touched.
pub fn tray_click(activity: Activity, tray_capture: bool) -> TrayClick {
    match activity {
        Activity::Recording => TrayClick::Stop,
        Activity::Idle if tray_capture => TrayClick::OpenRegion,
        Activity::Idle => TrayClick::Show,
        Activity::Saving => TrayClick::Show,
        Activity::Selecting => TrayClick::Ignore,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Panel {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Panel {
    pub fn right(self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub fn overlaps(self, other: Panel) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

fn fits_horizontally(panel: Panel, screen: Panel, gap: i32) -> bool {
    panel.w <= screen.w - gap * 2
        && panel.x >= screen.x + gap
        && panel.right() <= screen.right() - gap
}

fn fits_vertically(panel: Panel, screen: Panel, gap: i32) -> bool {
    panel.h <= screen.h - gap * 2
        && panel.y >= screen.y + gap
        && panel.bottom() <= screen.bottom() - gap
}

fn nudge_into(panel: Panel, screen: Panel, gap: i32) -> Panel {
    let max_x = (screen.right() - gap - panel.w).max(screen.x + gap);
    let max_y = (screen.bottom() - gap - panel.h).max(screen.y + gap);
    Panel {
        x: panel.x.clamp(screen.x + gap, max_x),
        y: panel.y.clamp(screen.y + gap, max_y),
        ..panel
    }
}

fn inside_screen(panel: Panel, screen: Panel, gap: i32) -> bool {
    let wide = panel.w > screen.w - gap * 2;
    let horizontal = if wide {
        panel.x <= screen.x + gap
    } else {
        fits_horizontally(panel, screen, gap)
    };
    horizontal && fits_vertically(panel, screen, gap)
}

fn slide_beside(selection: Panel, y: i32, size: (i32, i32), gap: i32) -> (Panel, Panel) {
    let (w, h) = size;
    (
        Panel {
            x: selection.right() + gap,
            y,
            w,
            h,
        },
        Panel {
            x: selection.x - gap - w,
            y,
            w,
            h,
        },
    )
}

/// Keeps a block on screen without covering `blocked` panels. `prefer_y` is where it would
/// sit beside the selection before it is slid up or down to stay visible.
fn place_block(
    screen: Panel,
    selection: Panel,
    prefer_y: i32,
    size: (i32, i32),
    gap: i32,
    blocked: &[Panel],
) -> Panel {
    let (w, h) = size;
    let (right, left) = slide_beside(selection, prefer_y, size, gap);
    let mut spots = vec![
        nudge_into(right, screen, gap),
        nudge_into(left, screen, gap),
        nudge_into(
            Panel {
                x: selection.x,
                y: selection.y - gap - h,
                w,
                h,
            },
            screen,
            gap,
        ),
        nudge_into(
            Panel {
                x: selection.x,
                y: selection.bottom() + gap,
                w,
                h,
            },
            screen,
            gap,
        ),
        nudge_into(
            Panel {
                x: screen.x + gap,
                y: screen.y + gap,
                w,
                h,
            },
            screen,
            gap,
        ),
        nudge_into(
            Panel {
                x: screen.right() - gap - w,
                y: screen.bottom() - gap - h,
                w,
                h,
            },
            screen,
            gap,
        ),
    ];
    for blocked in blocked {
        spots.push(nudge_into(
            Panel {
                x: blocked.x,
                y: blocked.y - gap - h,
                w,
                h,
            },
            screen,
            gap,
        ));
        spots.push(nudge_into(
            Panel {
                x: blocked.x,
                y: blocked.bottom() + gap,
                w,
                h,
            },
            screen,
            gap,
        ));
        spots.push(nudge_into(
            Panel {
                x: blocked.right() + gap,
                y: blocked.y,
                w,
                h,
            },
            screen,
            gap,
        ));
        spots.push(nudge_into(
            Panel {
                x: blocked.x - gap - w,
                y: blocked.y,
                w,
                h,
            },
            screen,
            gap,
        ));
    }
    // Prefer a spot that misses the selection. A double-click selects the whole screen, and then
    // every on-screen spot covers it: stay inside the screen and only keep the panels apart.
    let clear = |panel: &Panel, avoid_selection: bool| {
        inside_screen(*panel, screen, gap)
            && blocked.iter().all(|other| !panel.overlaps(*other))
            && (!avoid_selection || !panel.overlaps(selection))
    };
    spots
        .iter()
        .copied()
        .find(|panel| clear(panel, true))
        .or_else(|| spots.iter().copied().find(|panel| clear(panel, false)))
        .unwrap_or_else(|| {
            nudge_into(
                Panel {
                    x: screen.x + gap,
                    y: screen.y + gap,
                    w,
                    h,
                },
                screen,
                gap,
            )
        })
}

/// Keeps a candidate on its side of the selection. Nudging it across that edge would cover the area.
fn stay_off_selection(panel: Panel, screen: Panel, selection: Panel, gap: i32) -> Option<Panel> {
    let mut placed = nudge_into(panel, screen, 0);
    if panel.y >= selection.bottom() {
        let min_y = selection.bottom() + gap.min(8);
        if placed.y < min_y {
            if min_y + panel.h <= screen.bottom() {
                placed.y = min_y;
            } else {
                return None;
            }
        }
    } else if panel.bottom() <= selection.y {
        let max_bottom = selection.y - gap.min(8);
        if placed.bottom() > max_bottom {
            let y = max_bottom - panel.h;
            if y >= screen.y {
                placed.y = y;
            } else {
                return None;
            }
        }
    }
    if placed.x < screen.x {
        placed.x = screen.x;
    }
    if placed.right() > screen.right() {
        placed.x = screen.right() - placed.w;
    }
    if placed.y < screen.y {
        placed.y = screen.y;
    }
    if placed.bottom() > screen.bottom() {
        return None;
    }
    if placed.overlaps(selection) {
        None
    } else {
        Some(placed)
    }
}

fn outside_width(panel: Panel, selection: Panel) -> i32 {
    (selection.x - panel.x).max(0) + (panel.right() - selection.right()).max(0)
}

fn gap_from_selection(panel: Panel, selection: Panel, gap: i32) -> i32 {
    if panel.y >= selection.bottom() {
        (panel.y - (selection.bottom() + gap)).abs()
    } else if panel.bottom() <= selection.y {
        (selection.y - gap - panel.bottom()).abs()
    } else {
        100_000
    }
}

fn center_delta(panel: Panel, selection: Panel) -> i32 {
    (panel.x + panel.w / 2 - (selection.x + selection.w / 2)).abs()
}

/// The text buttons sit under the selection, inside its width, when that spot is free.
/// Above it is the next choice. Beside it is only for when the band above and below is full.
fn place_actions(
    screen: Panel,
    selection: Panel,
    size: (i32, i32),
    gap: i32,
    blocked: &[Panel],
) -> Panel {
    let (w, h) = size;
    let centered = selection.x + (selection.w - w) / 2;
    let mut spots = vec![
        Panel {
            x: centered,
            y: selection.bottom() + gap,
            w,
            h,
        },
        Panel {
            x: centered,
            y: selection.y - gap - h,
            w,
            h,
        },
        Panel {
            x: selection.x,
            y: selection.bottom() + gap,
            w,
            h,
        },
        Panel {
            x: selection.right() - w,
            y: selection.bottom() + gap,
            w,
            h,
        },
        Panel {
            x: selection.x,
            y: selection.y - gap - h,
            w,
            h,
        },
        Panel {
            x: selection.right() - w,
            y: selection.y - gap - h,
            w,
            h,
        },
    ];
    for other in blocked {
        spots.push(Panel {
            x: centered,
            y: other.bottom() + gap,
            w,
            h,
        });
        spots.push(Panel {
            x: centered,
            y: other.y - gap - h,
            w,
            h,
        });
        spots.push(Panel {
            x: other.x,
            y: other.bottom() + gap,
            w,
            h,
        });
        spots.push(Panel {
            x: other.x,
            y: other.y - gap - h,
            w,
            h,
        });
        spots.push(Panel {
            x: other.right() + gap,
            y: other.y,
            w,
            h,
        });
        spots.push(Panel {
            x: other.x - gap - w,
            y: other.y,
            w,
            h,
        });
    }
    let (right, left) = slide_beside(selection, selection.y, size, gap);
    spots.push(right);
    spots.push(left);
    spots.push(Panel {
        x: screen.x + gap,
        y: screen.y + gap,
        w,
        h,
    });
    spots.push(Panel {
        x: screen.right() - gap - w,
        y: screen.bottom() - gap - h,
        w,
        h,
    });
    let nudged: Vec<Panel> = spots
        .into_iter()
        .filter_map(|panel| stay_off_selection(panel, screen, selection, gap))
        .collect();
    let usable = |panel: &Panel, avoid_selection: bool, avoid_blocked: bool| {
        inside_screen(*panel, screen, 0)
            && (!avoid_blocked || blocked.iter().all(|other| !panel.overlaps(*other)))
            && (!avoid_selection || !panel.overlaps(selection))
    };
    let edge_miss = |panel: Panel| {
        (screen.y + gap - panel.y).max(0)
            + (panel.bottom() - (screen.bottom() - gap)).max(0)
            + (screen.x + gap - panel.x).max(0)
            + (panel.right() - (screen.right() - gap)).max(0)
    };
    let best = |avoid_selection: bool, avoid_blocked: bool| {
        nudged
            .iter()
            .copied()
            .filter(|panel| usable(panel, avoid_selection, avoid_blocked))
            .min_by_key(|panel| {
                (
                    outside_width(*panel, selection),
                    edge_miss(*panel),
                    gap_from_selection(*panel, selection, gap),
                    center_delta(*panel, selection),
                )
            })
    };
    best(true, true)
        .or_else(|| best(true, false))
        .or_else(|| best(false, true))
        .unwrap_or_else(|| {
            nudge_into(
                Panel {
                    x: screen.x + gap,
                    y: screen.y + gap,
                    w,
                    h,
                },
                screen,
                gap,
            )
        })
}

/// The size label prefers the space above the selection's left edge.
/// If buttons already sit there, it moves to another edge instead of covering them.
pub fn place_badge(screen: Panel, selection: Panel, size: (i32, i32), blocked: &[Panel]) -> Panel {
    let (w, h) = size;
    let gap = 10;
    let inset = 4;
    let spots = [
        Panel {
            x: selection.x,
            y: selection.y - h - gap,
            w,
            h,
        },
        Panel {
            x: selection.right() - w,
            y: selection.y - h - gap,
            w,
            h,
        },
        Panel {
            x: selection.x,
            y: selection.bottom() + gap,
            w,
            h,
        },
        Panel {
            x: selection.right() - w,
            y: selection.bottom() + gap,
            w,
            h,
        },
        Panel {
            x: selection.x + inset,
            y: selection.y + inset,
            w,
            h,
        },
        Panel {
            x: selection.right() - w - inset,
            y: selection.y + inset,
            w,
            h,
        },
        Panel {
            x: selection.x + inset,
            y: selection.bottom() - h - inset,
            w,
            h,
        },
        Panel {
            x: selection.right() - w - inset,
            y: selection.bottom() - h - inset,
            w,
            h,
        },
    ];
    let clamped: Vec<Panel> = spots
        .into_iter()
        .map(|panel| nudge_into(panel, screen, 8))
        .collect();
    let overlap = |panel: Panel| {
        blocked
            .iter()
            .map(|other| {
                let width = (panel.right().min(other.right()) - panel.x.max(other.x)).max(0);
                let height = (panel.bottom().min(other.bottom()) - panel.y.max(other.y)).max(0);
                width * height
            })
            .sum::<i32>()
    };
    clamped
        .iter()
        .copied()
        .enumerate()
        .min_by(|(index_a, a), (index_b, b)| {
            overlap(*a).cmp(&overlap(*b)).then(index_a.cmp(index_b))
        })
        .map(|(_, panel)| panel)
        .unwrap_or(clamped[0])
}

/// Tool column and action row stay on screen, off the selection, and off each other.
/// Tools stay beside the selection. The text buttons prefer the free band under it, then above it.
pub fn place_panels(
    screen: Panel,
    selection: Panel,
    tools: Option<(i32, i32)>,
    actions: (i32, i32),
    gap: i32,
) -> (Option<Panel>, Panel) {
    let tools_rect = tools.map(|size| place_block(screen, selection, selection.y, size, gap, &[]));
    let blocked: Vec<Panel> = tools_rect.into_iter().collect();
    let actions_rect = place_actions(screen, selection, actions, gap, &blocked);
    (tools_rect, actions_rect)
}

/// `0` means Auto: 200% on 4K, 150% on 1440p, 100% otherwise.
pub fn interface_scale(setting: u32, monitor_height: u32) -> u32 {
    if setting != 0 {
        return setting;
    }
    if monitor_height >= 2160 {
        200
    } else if monitor_height >= 1440 {
        150
    } else {
        100
    }
}

/// Screen movement wins. A synthetic message that never moves the cursor falls back to client coordinates.
pub fn drag_delta(
    start_screen: (i32, i32),
    now_screen: (i32, i32),
    start_client: (i32, i32),
    now_client: (i32, i32),
) -> (i32, i32) {
    let screen = (now_screen.0 - start_screen.0, now_screen.1 - start_screen.1);
    let client = (now_client.0 - start_client.0, now_client.1 - start_client.1);
    let span = |delta: (i32, i32)| delta.0.abs() + delta.1.abs();
    // A real drag keeps the cursor still inside the moved box, so the screen delta is the one that grows.
    // A synthetic message moves the client point while the cursor barely moves.
    if span(client) > span(screen) {
        client
    } else {
        screen
    }
}

/// Screen rectangle of the blinking recording outline.
///
/// The stroke sits outside the captured pixels, with a gap so the line is not
/// part of the picture. `dpi` is the monitor DPI (96 is 100%).
pub struct OutlineBox {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub stroke: i32,
}

pub fn recording_outline(x: i32, y: i32, width: u32, height: u32, dpi: u32) -> OutlineBox {
    let dpi = dpi.max(96);
    let stroke = (4 * dpi / 96).clamp(2, 32) as i32;
    let gap = (4 * dpi / 96).max(1) as i32;
    let margin = stroke + gap;
    OutlineBox {
        x: x.saturating_sub(margin),
        y: y.saturating_sub(margin),
        width: width.max(1) as i32 + margin * 2,
        height: height.max(1) as i32 + margin * 2,
        stroke,
    }
}

/// Bright for half a second, dim for the next half.
pub fn outline_blink(elapsed_ms: u128) -> bool {
    (elapsed_ms / 500).is_multiple_of(2)
}

/// Video frame index on a shared recording clock. `elapsed_ns` is time since the recording started.
pub fn frame_slot(elapsed_ns: u128, fps: u64) -> i64 {
    if fps == 0 {
        return 0;
    }
    (elapsed_ns.saturating_mul(fps as u128) / 1_000_000_000) as i64
}

/// Sample index on the same clock as [`frame_slot`].
pub fn sample_index(elapsed_ns: u128, rate: u64) -> u64 {
    if rate == 0 {
        return 0;
    }
    (elapsed_ns.saturating_mul(rate as u128) / 1_000_000_000) as u64
}

/// Interleaved stereo placed on a sample timeline.
///
/// `origin` is the timeline index of `samples[0]`. A microphone that starts later
/// gets a later origin, so its voice stays on the same moment as the picture.
#[derive(Clone, Debug, Default)]
pub struct AudioQueue {
    pub origin: u64,
    pub samples: Vec<f32>,
    started: bool,
}

impl AudioQueue {
    pub fn end(&self) -> u64 {
        self.origin + (self.samples.len() / 2) as u64
    }

    /// Adds the next block from one device.
    ///
    /// The first block is placed at `captured_at`. Every later block continues
    /// from the previous sample. A callback that runs a little late does not
    /// open a hole, and one that runs early does not overwrite the waveform.
    pub fn append(&mut self, captured_at: u64, samples: &[f32]) {
        if samples.len() < 2 {
            return;
        }
        if !self.started {
            self.started = true;
            self.push_at(captured_at, samples);
            return;
        }
        let at = self.end();
        self.push_at(at, samples);
    }

    pub fn push_at(&mut self, frame: u64, samples: &[f32]) {
        let usable = samples.len() & !1;
        if usable == 0 {
            return;
        }
        let samples = &samples[..usable];
        if self.samples.is_empty() {
            self.origin = frame;
            self.samples.extend_from_slice(samples);
            return;
        }
        if frame < self.origin {
            let skip = ((self.origin - frame) as usize).saturating_mul(2);
            if skip >= samples.len() {
                return;
            }
            self.push_at(self.origin, &samples[skip..]);
            return;
        }
        let end = self.end();
        if frame >= end {
            let gap = ((frame - end) as usize).saturating_mul(2);
            self.samples.extend(std::iter::repeat_n(0.0, gap));
            self.samples.extend_from_slice(samples);
            return;
        }
        let offset = ((frame - self.origin) as usize).saturating_mul(2);
        let tail = offset + samples.len();
        if self.samples.len() < tail {
            self.samples.resize(tail, 0.0);
        }
        self.samples[offset..tail].copy_from_slice(samples);
    }

    pub fn drop_before(&mut self, frame: u64) {
        if self.samples.is_empty() || frame <= self.origin {
            return;
        }
        if frame >= self.end() {
            self.origin = self.end();
            self.samples.clear();
            return;
        }
        let skip = ((frame - self.origin) as usize).saturating_mul(2);
        self.samples.drain(..skip);
        self.origin = frame;
    }
}

/// Mixes every queue that already has samples onto `cursor`.
///
/// Empty queues are silence: a microphone that has not delivered yet does not
/// hold the other stream back, and its later samples keep the timestamp they
/// were captured with. Nothing at or after `until` is consumed, so a chunk that
/// is still in flight can still land on its own moment. At most `max_frames`
/// are returned.
pub fn mix_from(queues: &mut [AudioQueue], cursor: u64, until: u64, max_frames: u64) -> Vec<f32> {
    if max_frames == 0 || until <= cursor {
        return Vec::new();
    }
    let Some(end) = queues
        .iter()
        .filter(|queue| !queue.samples.is_empty())
        .map(|queue| queue.end())
        .min()
    else {
        return Vec::new();
    };
    let end = end.min(until).min(cursor.saturating_add(max_frames));
    if end <= cursor {
        return Vec::new();
    }
    let frames = (end - cursor) as usize;
    let mut mixed = vec![0.0f32; frames * 2];
    for queue in queues.iter().filter(|queue| !queue.samples.is_empty()) {
        let first = cursor.max(queue.origin);
        let last = end.min(queue.end());
        if last <= first {
            continue;
        }
        let start_index = ((first - cursor) as usize).saturating_mul(2);
        let offset = ((first - queue.origin) as usize).saturating_mul(2);
        let count = ((last - first) as usize).saturating_mul(2);
        for (target, value) in mixed[start_index..start_index + count]
            .iter_mut()
            .zip(queue.samples[offset..offset + count].iter())
        {
            *target += *value;
        }
    }
    for queue in queues.iter_mut() {
        queue.drop_before(end);
    }
    mixed
}

/// Mixes stereo streams and leaves whatever has not been paired yet.
#[cfg_attr(not(test), allow(dead_code))]
pub fn mix_stereo(pending: &mut [Vec<f32>]) -> Vec<f32> {
    if pending.is_empty() {
        return Vec::new();
    }
    if pending.len() == 1 {
        return std::mem::take(&mut pending[0]);
    }
    let count = pending
        .iter()
        .map(|samples| samples.len())
        .min()
        .unwrap_or(0)
        & !1;
    if count == 0 {
        return Vec::new();
    }
    let mut mixed = vec![0.0f32; count];
    for samples in pending.iter() {
        for (target, value) in mixed.iter_mut().zip(samples.iter().take(count)) {
            *target += *value;
        }
    }
    for samples in pending.iter_mut() {
        samples.drain(..count.min(samples.len()));
    }
    mixed
}

pub fn downmix_stereo(input: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    if channels == 1 {
        let mut output = Vec::with_capacity(input.len() * 2);
        for sample in input {
            output.push(*sample);
            output.push(*sample);
        }
        return output;
    }
    let mut output = Vec::with_capacity(input.len() / channels * 2);
    for frame in input.chunks(channels) {
        if frame.len() < channels {
            break;
        }
        output.push(frame[0]);
        output.push(frame[1]);
    }
    output
}

pub fn resample_stereo(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == 0 || to_rate == 0 || from_rate == to_rate || input.len() < 2 {
        return input.to_vec();
    }
    let frames = input.len() / 2;
    let out_frames = ((frames as u64 * to_rate as u64) / from_rate as u64).max(1) as usize;
    let mut output = Vec::with_capacity(out_frames * 2);
    for index in 0..out_frames {
        let source = index as f64 * from_rate as f64 / to_rate as f64;
        let left = (source.floor() as usize).min(frames - 1);
        let right = (left + 1).min(frames - 1);
        let blend = (source - left as f64) as f32;
        for channel in 0..2 {
            let a = input[left * 2 + channel];
            let b = input[right * 2 + channel];
            output.push(a + (b - a) * blend);
        }
    }
    output
}

pub fn to_stereo_48k(input: &[f32], channels: u16, rate: u32) -> Vec<f32> {
    resample_stereo(&downmix_stereo(input, channels as usize), rate, 48_000)
}

const TWO_PASS_BYTES: u64 = 128 * 1024 * 1024;

pub fn gif_needs_two_pass(width: u32, height: u32, duration_us: u64, fps: u32) -> bool {
    let frames = duration_us.saturating_mul(u64::from(fps.max(1))) / 1_000_000;
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4)
        .saturating_mul(frames)
        > TWO_PASS_BYTES
}

fn scale_step(height: u32) -> String {
    if height == 0 {
        String::new()
    } else {
        format!(",scale=w=-2:h='min({height},ih)':flags=lanczos")
    }
}

pub fn gif_palette_graph(fps: u32, height: u32) -> String {
    format!(
        "fps={fps}{},palettegen=max_colors=256:stats_mode=diff",
        scale_step(height)
    )
}

pub fn gif_use_graph(fps: u32, height: u32) -> String {
    format!(
        "[in]fps={fps}{}[b];[b][pal]paletteuse=dither=sierra2:diff_mode=rectangle[out]",
        scale_step(height)
    )
}

pub fn gif_single_graph(fps: u32, height: u32) -> String {
    format!(
        "fps={fps}{},split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2:diff_mode=rectangle",
        scale_step(height)
    )
}

pub fn webp_graph(fps: u32, height: u32) -> String {
    format!("fps={fps}{},format=yuv420p", scale_step(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_click_stops_audio_and_video_recording() {
        assert_eq!(tray_click(Activity::Recording, false), TrayClick::Stop);
        assert_eq!(tray_click(Activity::Recording, true), TrayClick::Stop);
        assert_eq!(tray_click(Activity::Idle, false), TrayClick::Show);
        assert_eq!(tray_click(Activity::Idle, true), TrayClick::OpenRegion);
        assert_eq!(tray_click(Activity::Saving, true), TrayClick::Show);
        assert_eq!(tray_click(Activity::Selecting, false), TrayClick::Ignore);
    }

    #[test]
    fn text_drag_follows_the_screen_and_synthetic_messages() {
        assert_eq!(drag_delta((10, 10), (50, 30), (0, 0), (1, 1)), (40, 20));
        assert_eq!(drag_delta((10, 10), (10, 10), (10, 10), (50, 30)), (40, 20));
        assert_eq!(drag_delta((10, 10), (12, 5), (10, 10), (50, 30)), (40, 20));
    }

    #[test]
    fn recording_outline_sits_outside_the_captured_pixels() {
        let frame = recording_outline(100, 50, 300, 200, 96);
        assert_eq!(frame.stroke, 4);
        assert_eq!(
            (frame.x, frame.y, frame.width, frame.height),
            (92, 42, 316, 216)
        );
        assert_eq!(
            frame.x + frame.stroke,
            96,
            "the red line ends before the recorded edge"
        );
        let hidpi = recording_outline(0, 0, 10, 10, 192);
        assert_eq!(hidpi.stroke, 8);
        assert_eq!((hidpi.x, hidpi.y), (-16, -16));
    }

    #[test]
    fn outline_blinks_twice_a_second() {
        assert!(outline_blink(0));
        assert!(outline_blink(499));
        assert!(!outline_blink(500));
        assert!(!outline_blink(999));
        assert!(outline_blink(1000));
    }

    #[test]
    fn frame_slot_follows_the_recording_clock() {
        assert_eq!(frame_slot(0, 60), 0);
        assert_eq!(frame_slot(1_000_000_000, 60), 60);
        assert_eq!(frame_slot(500_000_000, 30), 15);
        assert_eq!(sample_index(1_000_000_000, 48_000), 48_000);
        assert_eq!(sample_index(500_000_000, 48_000), 24_000);
    }

    #[test]
    fn one_device_stays_contiguous_when_callbacks_jitter() {
        let mut queue = AudioQueue::default();
        queue.append(10, &[0.1, 0.1, 0.2, 0.2]);
        queue.append(500, &[0.3, 0.3]);
        assert_eq!(queue.origin, 10);
        assert_eq!(queue.samples, vec![0.1, 0.1, 0.2, 0.2, 0.3, 0.3]);
        let end = queue.end();
        queue.drop_before(end);
        assert!(queue.samples.is_empty());
        queue.append(9_000, &[0.4, 0.4]);
        assert_eq!(queue.origin, end);
        assert_eq!(queue.samples, vec![0.4, 0.4]);
    }

    #[test]
    fn microphone_keeps_its_place_on_the_timeline() {
        let mut system = AudioQueue::default();
        let mut microphone = AudioQueue::default();
        system.push_at(0, &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
        microphone.push_at(2, &[0.25, 0.25, 0.25, 0.25]);
        let mut queues = [system, microphone];
        let mixed = mix_from(&mut queues, 0, 100, 100);
        assert_eq!(mixed, vec![0.5, 0.5, 0.5, 0.5, 0.75, 0.75, 0.75, 0.75]);
    }

    #[test]
    fn a_silent_input_does_not_block_the_other() {
        let mut system = AudioQueue::default();
        system.push_at(0, &[0.5, 0.25, 0.5, 0.25]);
        let mut queues = [system, AudioQueue::default()];
        let mixed = mix_from(&mut queues, 0, 100, 100);
        assert_eq!(mixed, vec![0.5, 0.25, 0.5, 0.25]);
        assert!(queues[1].samples.is_empty());
    }

    #[test]
    fn mix_waits_for_audio_that_can_still_arrive() {
        let mut system = AudioQueue::default();
        system.push_at(0, &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
        let mut queues = [system, AudioQueue::default()];
        let held = mix_from(&mut queues, 0, 2, 100);
        assert_eq!(held, vec![0.5, 0.5, 0.5, 0.5]);
        assert_eq!(queues[0].origin, 2);
        queues[1].push_at(2, &[0.25, 0.25, 0.25, 0.25]);
        let rest = mix_from(&mut queues, 2, 100, 100);
        assert_eq!(rest, vec![0.75, 0.75, 0.75, 0.75]);
    }

    #[test]
    fn mix_keeps_the_unpaired_tail() {
        let mut pending = vec![vec![0.5, 0.25, 0.5, 0.25], vec![0.5, 0.25]];
        let mixed = mix_stereo(&mut pending);
        assert_eq!(mixed, vec![1.0, 0.5]);
        assert_eq!(pending[0], vec![0.5, 0.25]);
        assert!(pending[1].is_empty());
        let rest = mix_stereo(&mut pending);
        assert!(rest.is_empty(), "a stream with no partner must wait");
        assert_eq!(pending[0], vec![0.5, 0.25]);
    }

    #[test]
    fn resample_and_downmix_hit_stereo_48k() {
        let mono = to_stereo_48k(&[0.0, 1.0], 1, 24_000);
        assert_eq!(mono.len(), 8);
        assert!((mono[0] - 0.0).abs() < 0.01 && (mono[1] - 0.0).abs() < 0.01);
        assert!((mono[6] - 1.0).abs() < 0.01);
    }

    #[test]
    fn gif_and_webp_graphs_match_the_recording_path() {
        let gif = gif_single_graph(30, 480);
        assert!(gif.starts_with("fps=30,scale=w=-2:h='min(480,ih)':flags=lanczos,split"));
        assert!(gif.contains("palettegen=max_colors=256:stats_mode=diff"));
        assert!(gif.contains("paletteuse=dither=sierra2:diff_mode=rectangle"));
        assert!(gif_palette_graph(15, 0).starts_with("fps=15,palettegen"));
        assert!(gif_use_graph(15, 720).contains("[pal]paletteuse=dither=sierra2"));
        assert_eq!(
            webp_graph(30, 480),
            "fps=30,scale=w=-2:h='min(480,ih)':flags=lanczos,format=yuv420p"
        );
        assert!(!gif_needs_two_pass(64, 64, 200_000, 30));
        assert!(gif_needs_two_pass(1920, 1080, 10_000_000, 30));
    }

    #[test]
    fn auto_scale_follows_the_monitor_height() {
        assert_eq!(interface_scale(0, 1080), 100);
        assert_eq!(interface_scale(0, 1439), 100);
        assert_eq!(interface_scale(0, 1440), 150);
        assert_eq!(interface_scale(0, 2160), 200);
        assert_eq!(interface_scale(125, 2160), 125);
    }

    #[test]
    fn panels_sit_beside_the_selection() {
        let screen = Panel {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        };
        let selection = Panel {
            x: 80,
            y: 60,
            w: 400,
            h: 300,
        };
        let (tools, actions) = place_panels(screen, selection, Some((48, 220)), (180, 40), 10);
        let tools = tools.expect("tools");
        assert!(tools.x >= selection.right());
        assert!(!tools.overlaps(actions));
        assert!(!tools.overlaps(selection));
        assert!(!actions.overlaps(selection));
        assert!(
            actions.y >= selection.bottom(),
            "actions left the free band under the selection: {actions:?}"
        );
        assert!(
            actions.x >= selection.x && actions.right() <= selection.right(),
            "actions slipped outside the selection width: {actions:?}"
        );
    }

    #[test]
    fn size_badge_moves_off_the_buttons() {
        let screen = Panel {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        };
        let selection = Panel {
            x: 100,
            y: 200,
            w: 500,
            h: 300,
        };
        let free = place_badge(screen, selection, (90, 28), &[]);
        assert_eq!((free.x, free.y), (100, 162));
        let covering_the_left = Panel {
            x: 100,
            y: 150,
            w: 200,
            h: 40,
        };
        let shifted = place_badge(screen, selection, (90, 28), &[covering_the_left]);
        assert!(!shifted.overlaps(covering_the_left), "{shifted:?}");
        assert!(
            shifted.bottom() <= selection.y && shifted.x > covering_the_left.right(),
            "badge stayed under the buttons: {shifted:?}"
        );
        let covering_the_top = Panel {
            x: 80,
            y: 150,
            w: 540,
            h: 42,
        };
        let below = place_badge(screen, selection, (90, 28), &[covering_the_top]);
        assert!(!below.overlaps(covering_the_top), "{below:?}");
        assert!(
            below.y >= selection.bottom(),
            "badge did not leave the occupied top band: {below:?}"
        );
    }

    #[test]
    fn actions_sit_above_the_selection_when_the_bottom_is_full() {
        let screen = Panel {
            x: 0,
            y: 0,
            w: 800,
            h: 500,
        };
        let selection = Panel {
            x: 120,
            y: 400,
            w: 280,
            h: 70,
        };
        let (_, actions) = place_panels(screen, selection, None, (220, 36), 10);
        assert!(
            actions.bottom() <= selection.y,
            "actions did not take the space above the selection: {actions:?}"
        );
        assert!(
            actions.x >= selection.x && actions.right() <= selection.right(),
            "{actions:?}"
        );
        assert!(!actions.overlaps(selection));
    }

    #[test]
    fn actions_move_to_the_other_side_instead_of_covering_the_selection() {
        let screen = Panel {
            x: 0,
            y: 0,
            w: 800,
            h: 500,
        };
        let selection = Panel {
            x: 120,
            y: 400,
            w: 280,
            h: 70,
        };
        let (_, actions) = place_panels(screen, selection, None, (220, 36), 10);
        assert!(
            actions.y >= 10 && actions.bottom() <= 490,
            "actions left the screen: {actions:?}"
        );
        assert!(!actions.overlaps(selection), "{actions:?}");
        let crowded = Panel {
            x: 0,
            y: 0,
            w: 640,
            h: 480,
        };
        let area = Panel {
            x: 96,
            y: 72,
            w: 360,
            h: 260,
        };
        let (_, wide) = place_panels(crowded, area, Some((68, 860)), (856, 64), 8);
        assert!(
            !wide.overlaps(area),
            "wide action row covers the selection: {wide:?}"
        );
    }

    #[test]
    fn tools_slide_inside_the_screen_when_the_selection_is_at_the_bottom() {
        let screen = Panel {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let selection = Panel {
            x: 200,
            y: 820,
            w: 640,
            h: 220,
        };
        let (tools, actions) = place_panels(screen, selection, Some((36, 400)), (420, 32), 8);
        let tools = tools.expect("tools");
        assert!(
            tools.y >= 8 && tools.bottom() <= 1072,
            "tools left the screen: {tools:?}"
        );
        assert!(
            actions.y >= 8 && actions.bottom() <= 1072,
            "actions left the screen: {actions:?}"
        );
        assert!(!tools.overlaps(selection), "{tools:?}");
        assert!(!actions.overlaps(selection), "{actions:?}");
        assert!(
            !tools.overlaps(actions),
            "panels overlap: {tools:?} {actions:?}"
        );
    }

    #[test]
    fn full_screen_selection_keeps_panels_inside_and_apart() {
        let screen = Panel {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let (tools, actions) = place_panels(screen, screen, Some((36, 400)), (480, 32), 8);
        let tools = tools.expect("tools");
        assert!(
            tools.y >= 8 && tools.bottom() <= 1072 && tools.x >= 8 && tools.right() <= 1912,
            "{tools:?}"
        );
        assert!(
            actions.y >= 8 && actions.bottom() <= 1072 && actions.x >= 8 && actions.right() <= 1912,
            "{actions:?}"
        );
        assert!(!tools.overlaps(actions), "{tools:?} {actions:?}");
    }
}
