//! The controller diagram on the options' Controller page: the icon type's
//! controller drawn in vectors, each control's actions called out beside it.
//!
//! The drawing is in units of `k`, centred on the controller; the body spans
//! about eight units across.

use sdl3_sys::gamepad::*;

use super::canvas::{Color, Rect};
use super::input::PadStyle;
use super::locale::Locale;
use super::text::{Align, Face, Style};
use super::theme::{Role, palette};
use super::widgets::Painter;
use crate::controls::{ACTIONS, Action, Bindings, Source};

/// One physical control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    LeftTrigger,
    LeftBumper,
    LeftStick,
    DPad,
    Back,
    RightTrigger,
    RightBumper,
    North,
    East,
    West,
    South,
    Start,
    RightStick,
}

/// Every control, the left side first.
const CONTROLS: [Control; 13] = [
    Control::LeftTrigger,
    Control::LeftBumper,
    Control::LeftStick,
    Control::DPad,
    Control::Back,
    Control::RightTrigger,
    Control::RightBumper,
    Control::North,
    Control::East,
    Control::West,
    Control::South,
    Control::Start,
    Control::RightStick,
];

impl Control {
    /// The control `source` reads, if any.
    fn of(source: Source) -> Option<Self> {
        Some(match source {
            Source::PadButton(button) => match SDL_GamepadButton(button) {
                SDL_GAMEPAD_BUTTON_SOUTH => Control::South,
                SDL_GAMEPAD_BUTTON_EAST => Control::East,
                SDL_GAMEPAD_BUTTON_WEST => Control::West,
                SDL_GAMEPAD_BUTTON_NORTH => Control::North,
                SDL_GAMEPAD_BUTTON_BACK => Control::Back,
                SDL_GAMEPAD_BUTTON_START => Control::Start,
                SDL_GAMEPAD_BUTTON_LEFT_STICK => Control::LeftStick,
                SDL_GAMEPAD_BUTTON_RIGHT_STICK => Control::RightStick,
                SDL_GAMEPAD_BUTTON_LEFT_SHOULDER => Control::LeftBumper,
                SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER => Control::RightBumper,
                SDL_GAMEPAD_BUTTON_DPAD_UP
                | SDL_GAMEPAD_BUTTON_DPAD_DOWN
                | SDL_GAMEPAD_BUTTON_DPAD_LEFT
                | SDL_GAMEPAD_BUTTON_DPAD_RIGHT => Control::DPad,
                _ => return None,
            },
            Source::PadAxis(axis, _) => match SDL_GamepadAxis(axis) {
                SDL_GAMEPAD_AXIS_LEFTX | SDL_GAMEPAD_AXIS_LEFTY => Control::LeftStick,
                SDL_GAMEPAD_AXIS_RIGHTX | SDL_GAMEPAD_AXIS_RIGHTY => Control::RightStick,
                SDL_GAMEPAD_AXIS_LEFT_TRIGGER => Control::LeftTrigger,
                SDL_GAMEPAD_AXIS_RIGHT_TRIGGER => Control::RightTrigger,
                _ => return None,
            },
            _ => return None,
        })
    }

    /// Whether the control sits between the sticks, its callout going up.
    fn centered(self) -> bool {
        matches!(self, Control::Back | Control::Start)
    }

    /// Whether the control's callout goes to the left.
    fn on_left(self) -> bool {
        matches!(
            self,
            Control::LeftTrigger
                | Control::LeftBumper
                | Control::LeftStick
                | Control::DPad
                | Control::Back
        )
    }
}

/// The string keys naming `action`, as the key configuration does.
fn action_keys(action: Action) -> &'static [&'static str] {
    match action {
        Action::Up | Action::Down | Action::Left | Action::Right => &["keys.group.move"],
        Action::Confirm => &["keys.interact", "keys.confirm"],
        Action::Cancel => &["keys.cancel"],
        Action::Run => &["keys.run"],
        Action::Aim => &["keys.aim"],
        Action::Fire => &["keys.fire"],
        Action::Start => &["keys.inventory"],
        Action::PageLeft => &["keys.page_left"],
        Action::PageRight => &["keys.page_right"],
        Action::QuickTurn => &["keys.quick_turn"],
        Action::Reload => &["keys.reload"],
        Action::Map => &["keys.map"],
        Action::WeaponPrev => &["keys.weapon_prev"],
        Action::WeaponNext => &["keys.weapon_next"],
        Action::TargetPrev => &["keys.target_prev"],
        Action::TargetNext => &["keys.target_next"],
        Action::EquipSlot1
        | Action::EquipSlot2
        | Action::EquipSlot3
        | Action::EquipSlot4
        | Action::EquipSlot5
        | Action::EquipSlot6
        | Action::EquipSlot7
        | Action::EquipSlot8
        | Action::EquipSlot9
        | Action::EquipSlot10 => &["keys.group.equip"],
    }
}

/// What `control` does under `bindings`: its actions' names joined, empty
/// when it does nothing. Start always opens the menu.
fn callout(control: Control, bindings: &Bindings, text: &Locale) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for action in ACTIONS {
        if bindings
            .get(action)
            .iter()
            .any(|&source| Control::of(source) == Some(control))
        {
            for &key in action_keys(action) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
    }
    if control == Control::Start {
        keys.push("keys.tab.menu");
    }
    keys.iter()
        .map(|&key| text.get(key))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Where a style's controls sit, in units from the controller's centre.
struct Layout {
    left_stick: (f32, f32),
    right_stick: (f32, f32),
    dpad: (f32, f32),
    face: (f32, f32),
    back: (f32, f32),
    start: (f32, f32),
    home: (f32, f32),
}

impl Layout {
    fn of(style: PadStyle) -> Self {
        match style {
            // The sticks offset: the left one above the d-pad.
            PadStyle::Xbox | PadStyle::Nintendo => Self {
                left_stick: (-2.3, -0.9),
                right_stick: (1.2, 0.7),
                dpad: (-1.2, 0.7),
                face: (2.3, -0.9),
                back: (-0.75, -0.9),
                start: (0.75, -0.9),
                home: (0.0, -1.75),
            },
            // The sticks side by side below the d-pad and face buttons.
            PadStyle::PlayStation => Self {
                left_stick: (-1.15, 0.75),
                right_stick: (1.15, 0.75),
                dpad: (-2.3, -0.9),
                face: (2.3, -0.9),
                back: (-1.95, -2.0),
                start: (1.95, -2.0),
                home: (0.0, 0.85),
            },
        }
    }

    /// Where a control's callout line starts.
    fn anchor(&self, control: Control) -> (f32, f32) {
        let (fx, fy) = self.face;
        match control {
            Control::LeftTrigger => (-3.0, -3.1),
            Control::LeftBumper => (-3.35, -2.5),
            Control::RightTrigger => (3.0, -3.1),
            Control::RightBumper => (3.35, -2.5),
            Control::LeftStick => self.left_stick,
            Control::RightStick => self.right_stick,
            Control::DPad => self.dpad,
            Control::Back => self.back,
            Control::Start => self.start,
            Control::North => (fx, fy - FACE_SPREAD),
            Control::East => (fx + FACE_SPREAD, fy),
            Control::West => (fx - FACE_SPREAD, fy),
            Control::South => (fx, fy + FACE_SPREAD),
        }
    }
}

/// How far the face buttons sit from their cluster's centre.
const FACE_SPREAD: f32 = 0.62;
/// The side callouts' end, beyond the body on either side.
const ELBOW: f32 = 4.2;
/// The room beside each elbow for a side callout's text.
const LABEL_ROOM: f32 = 4.1;
/// The centre buttons' callouts sit above the controller, here.
const OVER: f32 = -3.45;
/// The diagram's extent in units.
const TOP: f32 = -3.7;
const BOTTOM: f32 = 3.0;

const BODY: Color = Color::rgb(0x26, 0x26, 0x26);
const RIM: Color = Color::rgb(0x6A, 0x6A, 0x6A);
const PART: Color = Color::rgb(0x3E, 0x3E, 0x3E);
const WELL: Color = Color::rgb(0x12, 0x12, 0x12);
const BUTTON: Color = Color::rgb(0x18, 0x18, 0x18);

/// One callout's text, where it is anchored and how it is aligned.
struct Label {
    lines: Vec<String>,
    style: Style,
    x: f32,
    y: f32,
    align: Align,
}

/// `text` in `style`, broken at its " / " joins into lines no wider than
/// `room` where it can be, and shrunk until the widest line fits.
fn wrap(p: &mut Painter<'_>, mut style: Style, text: &str, room: f32) -> (Vec<String>, Style) {
    let mut lines: Vec<String> = Vec::new();
    for part in text.split(" / ") {
        match lines.last_mut() {
            Some(line) if p.fonts.measure(style, &format!("{line} / {part}")) <= room => {
                line.push_str(" / ");
                line.push_str(part);
            }
            Some(line) => {
                line.push_str(" /");
                lines.push(part.to_string());
            }
            None => lines.push(part.to_string()),
        }
    }
    let widest = lines
        .iter()
        .map(|line| p.fonts.measure(style, line))
        .fold(0.0, f32::max);
    if widest > room && room > 0.0 {
        style.px = ((style.px as f32 * room / widest).floor() as u32).max(8);
    }
    (lines, style)
}

/// A line's height for `style`.
fn line_height(style: Style) -> f32 {
    style.px as f32 * 1.15
}

/// Draw the diagram for `style`'s controller with what `bindings` do,
/// fitted into `rect` and bottom-aligned.
pub fn draw(p: &mut Painter<'_>, rect: Rect, style: PadStyle, bindings: &Bindings, text: &Locale) {
    let k = (rect.w / ((ELBOW + LABEL_ROOM) * 2.0)).min(rect.h / (BOTTOM - TOP));
    let origin = (rect.center_x(), rect.bottom() - BOTTOM * k);
    let at = |(x, y): (f32, f32)| (origin.0 + x * k, origin.1 + y * k);
    let area = |r: Rect| {
        let (x, y) = at((r.x, r.y));
        Rect::new(x, y, r.w * k, r.h * k)
    };
    let layout = Layout::of(style);

    // The shoulders peek out above the body, the triggers behind the
    // bumpers.
    let trigger = Rect::new(-3.1, -3.4, 1.15, 1.0);
    let bumper = Rect::new(-3.45, -2.78, 1.95, 0.6);
    for side in [-1.0f32, 1.0] {
        for (part, radius) in [(trigger, 0.4), (bumper, 0.27)] {
            let part = area(mirror(part, side));
            p.canvas
                .fill_round_rect(part.inset(-k * 0.05, -k * 0.05), (radius + 0.05) * k, RIM);
            p.canvas.fill_round_rect(part, radius * k, PART);
        }
    }
    let names = shoulder_names(style);
    let bold = Style::new(Face::Bold, (k * 0.28).round().max(7.0) as u32);
    // Centred on what shows of each: the trigger above the bumper, the
    // bumper above the body.
    let centers = [(-2.52, -3.09), (-2.47, -2.54), (2.52, -3.09), (2.47, -2.54)];
    for (name, center) in names.into_iter().zip(centers) {
        let (x, y) = at(center);
        p.fonts
            .draw_centered(p.canvas, bold, x, y, Align::Center, palette::TEXT, name);
    }

    // The body: a rounded top over two angled grips, outlined by drawing
    // the pieces grown in the rim colour first.
    let rim = 0.05;
    for (grow, color) in [(rim, RIM), (0.0, BODY)] {
        let top = Rect::new(-3.3, -2.3, 6.6, 3.2).inset(-grow, -grow);
        p.canvas.fill_round_rect(area(top), (1.1 + grow) * k, color);
        for side in [-1.0f32, 1.0] {
            let grip = ellipse(
                at((2.7 * side, 0.9)),
                (1.25 + grow) * k,
                (2.0 + grow) * k,
                -0.35 * side,
            );
            p.canvas.fill_polygon(&grip, color);
        }
        let bridge = ellipse(at((0.0, 0.75)), (2.4 + grow) * k, (0.55 + grow) * k, 0.0);
        p.canvas.fill_polygon(&bridge, color);
    }
    if style == PadStyle::PlayStation {
        let pad = area(Rect::new(-1.4, -2.3, 2.8, 1.55));
        p.canvas.fill_round_rect(pad, 0.3 * k, RIM);
        p.canvas
            .fill_round_rect(pad.inset(k * 0.05, k * 0.05), 0.25 * k, PART);
    }

    // The callouts, drawn under the controls so each line runs out from
    // beneath its button. Controls doing the same thing share one label.
    let line = (k * 0.045).max(1.0);
    let mut lines: Vec<((f32, f32), (f32, f32))> = Vec::new();
    let mut labels = Vec::new();
    let side_room = LABEL_ROOM * k - k * 0.3;
    let base = p.style(Role::Hint);
    let text_style = Style::new(base.face, (k * 0.42).max(p.u(20.0)).round() as u32);
    for left in [true, false] {
        let mut items: Vec<(String, Vec<Control>, f32)> = Vec::new();
        for &control in CONTROLS
            .iter()
            .filter(|c| c.on_left() == left && !c.centered())
        {
            let label = callout(control, bindings, text);
            if label.is_empty() {
                continue;
            }
            match items.iter_mut().find(|item| item.0 == label) {
                Some(item) => item.1.push(control),
                None => items.push((label, vec![control], 0.0)),
            }
        }
        // Each label wants the middle of its controls.
        for item in &mut items {
            item.2 =
                item.1.iter().map(|&c| at(layout.anchor(c)).1).sum::<f32>() / item.1.len() as f32;
        }
        // Top to bottom; level ones from the outside in.
        let outer = |item: &(String, Vec<Control>, f32)| layout.anchor(item.1[0]).0.abs();
        items.sort_by(|a, b| a.2.total_cmp(&b.2).then(outer(b).total_cmp(&outer(a))));
        let wanted: Vec<f32> = items.iter().map(|item| item.2).collect();
        let wrapped: Vec<_> = items
            .iter()
            .map(|item| wrap(p, text_style, &item.0, side_room))
            .collect();
        let heights: Vec<f32> = wrapped
            .iter()
            .map(|(lines, style)| lines.len() as f32 * line_height(*style))
            .collect();
        let gap = k * 0.8 - line_height(text_style);
        let slots = spread(&wanted, &heights, gap, at((0.0, TOP)).1, rect.bottom());
        let side = if left { -1.0 } else { 1.0 };
        for (((_, controls, _), (text, style)), slot) in items.into_iter().zip(wrapped).zip(slots) {
            let bend = (origin.0 + side * (ELBOW - 0.3) * k, slot);
            let end = (origin.0 + side * ELBOW * k, slot);
            for control in controls {
                lines.push((at(layout.anchor(control)), bend));
            }
            lines.push((bend, end));
            labels.push(Label {
                lines: text,
                style,
                x: end.0 + side * k * 0.15,
                y: slot,
                align: if left { Align::Right } else { Align::Left },
            });
        }
    }
    // The centre buttons' callouts go up between the shoulders.
    for control in [Control::Back, Control::Start] {
        let label = callout(control, bindings, text);
        if label.is_empty() {
            continue;
        }
        let anchor = layout.anchor(control);
        let top = (anchor.0.clamp(-0.9, 0.9), OVER + 0.3);
        lines.push((at(anchor), at(top)));
        let (x, y) = at((top.0, OVER));
        let (text, style) = wrap(p, text_style, &label, k * 1.8);
        // Upwards from the line's end.
        let y = y - (text.len() - 1) as f32 * line_height(style);
        labels.push(Label {
            lines: text,
            style,
            x,
            y,
            align: Align::Center,
        });
    }
    for (a, b) in lines {
        p.canvas.fill_polygon(&segment(a, b, line), palette::TEXT);
    }

    // The controls.
    for stick in [layout.left_stick, layout.right_stick] {
        let (x, y) = at(stick);
        circle(p, x, y, 0.62 * k, WELL);
        circle(p, x, y, 0.46 * k, PART);
        circle(p, x, y, 0.3 * k, Color::rgb(0x34, 0x34, 0x34));
    }
    {
        let (x, y) = at(layout.dpad);
        circle(p, x, y, 0.62 * k, WELL);
        let (arm, reach) = (0.34 * k, 0.55 * k);
        let radius = 0.06 * k;
        p.canvas.fill_round_rect(
            Rect::new(x - arm / 2.0, y - reach, arm, reach * 2.0),
            radius,
            PART,
        );
        p.canvas.fill_round_rect(
            Rect::new(x - reach, y - arm / 2.0, reach * 2.0, arm),
            radius,
            PART,
        );
    }
    for control in [Control::North, Control::East, Control::West, Control::South] {
        let (x, y) = at(layout.anchor(control));
        circle(p, x, y, 0.3 * k, RIM);
        circle(p, x, y, 0.26 * k, BUTTON);
        face_mark(p, x, y, k, style, control);
    }
    for (x, y) in [at(layout.back), at(layout.start)] {
        let (w, h) = (0.4 * k, 0.24 * k);
        p.canvas
            .fill_round_rect(Rect::new(x - w / 2.0, y - h / 2.0, w, h), h / 2.0, PART);
    }
    {
        let (x, y) = at(layout.home);
        circle(p, x, y, 0.22 * k, PART);
    }

    // The callouts' text, each centred on its line.
    for label in labels {
        let step = line_height(label.style);
        let first = label.y - (label.lines.len() - 1) as f32 * step / 2.0;
        for (index, line) in label.lines.iter().enumerate() {
            p.fonts.draw_centered(
                p.canvas,
                label.style,
                label.x,
                first + index as f32 * step,
                label.align,
                palette::TEXT,
                line,
            );
        }
    }
}

/// The centres for labels `heights` tall that want to be centred at
/// `wanted` (sorted): where they are, `gap` apart, crowded runs pushed
/// apart around their middle, and the whole column kept within
/// `top..bottom`.
fn spread(wanted: &[f32], heights: &[f32], gap: f32, top: f32, bottom: f32) -> Vec<f32> {
    // Each label's centre below its run's top, the run packed.
    let offsets = |first: usize, count: usize| {
        let mut edge = 0.0;
        heights[first..first + count]
            .iter()
            .map(|&height| {
                let center = edge + height / 2.0;
                edge += height + gap;
                center
            })
            .collect::<Vec<f32>>()
    };
    // The top that puts a run's labels, on average, where they want.
    let top_of = |first: usize, count: usize| {
        let offsets = offsets(first, count);
        (0..count)
            .map(|i| wanted[first + i] - offsets[i])
            .sum::<f32>()
            / count as f32
    };
    let extent = |first: usize, count: usize| {
        heights[first..first + count].iter().sum::<f32>() + gap * (count as f32 - 1.0)
    };
    // Runs of labels: the first index, the count and the top.
    let mut runs: Vec<(usize, usize, f32)> = Vec::new();
    for index in 0..wanted.len() {
        runs.push((index, 1, top_of(index, 1)));
        while let [.., (a_first, a_count, a_top), (_, b_count, b_top)] = runs[..] {
            if b_top - (a_top + extent(a_first, a_count)) >= gap {
                break;
            }
            let count = a_count + b_count;
            runs.truncate(runs.len() - 2);
            runs.push((a_first, count, top_of(a_first, count)));
        }
    }
    let mut slots = Vec::with_capacity(wanted.len());
    for (first, count, run_top) in runs {
        slots.extend(offsets(first, count).into_iter().map(|o| run_top + o));
    }
    // Keep the whole column inside, the top first.
    if let (Some(&first), Some(&last)) = (slots.first(), slots.last()) {
        let (first, last) = (
            first - heights[0] / 2.0,
            last + heights[heights.len() - 1] / 2.0,
        );
        let shift = if first < top {
            top - first
        } else if last > bottom {
            (bottom - last).max(top - first)
        } else {
            0.0
        };
        for slot in &mut slots {
            *slot += shift;
        }
    }
    slots
}

/// `part` as it is on the left, flipped to the right for `side` 1.
fn mirror(part: Rect, side: f32) -> Rect {
    if side < 0.0 {
        part
    } else {
        Rect::new(-part.right(), part.y, part.w, part.h)
    }
}

/// The shoulder names: left trigger, left bumper, right trigger, right
/// bumper.
fn shoulder_names(style: PadStyle) -> [&'static str; 4] {
    match style {
        PadStyle::Xbox => ["LT", "LB", "RT", "RB"],
        PadStyle::PlayStation => ["L2", "L1", "R2", "R1"],
        PadStyle::Nintendo => ["ZL", "L", "ZR", "R"],
    }
}

/// A face button's letter or symbol.
fn face_mark(p: &mut Painter<'_>, x: f32, y: f32, k: f32, style: PadStyle, control: Control) {
    let letter = |p: &mut Painter<'_>, letter: &str, color: Color| {
        let px = (k * 0.34).round().max(7.0) as u32;
        let style = Style::new(Face::Bold, px);
        p.fonts
            .draw_centered(p.canvas, style, x, y, Align::Center, color, letter);
    };
    match style {
        PadStyle::Xbox => {
            let (name, color) = match control {
                Control::North => ("Y", Color::rgb(0xE8, 0xB8, 0x20)),
                Control::East => ("B", Color::rgb(0xE0, 0x4A, 0x40)),
                Control::West => ("X", Color::rgb(0x40, 0x80, 0xE0)),
                _ => ("A", Color::rgb(0x4C, 0xB0, 0x4C)),
            };
            letter(p, name, color);
        }
        // Lettered by position the other way round.
        PadStyle::Nintendo => {
            let name = match control {
                Control::North => "X",
                Control::East => "A",
                Control::West => "Y",
                _ => "B",
            };
            letter(p, name, palette::TEXT);
        }
        PadStyle::PlayStation => {
            let size = k * 0.13;
            let t = (k * 0.045).max(1.0);
            match control {
                Control::North => {
                    let color = Color::rgb(0x40, 0xC0, 0xA0);
                    let corners = |radius: f32| {
                        [-90.0f32, 30.0, 150.0].map(|degrees| {
                            let angle = degrees.to_radians();
                            (x + radius * angle.cos(), y + radius * angle.sin())
                        })
                    };
                    p.canvas.fill_polygon(&corners(size * 1.2), color);
                    p.canvas
                        .fill_polygon(&corners(size * 1.2 - t * 2.0), BUTTON);
                }
                Control::East => {
                    circle(p, x, y, size, Color::rgb(0xE0, 0x50, 0x50));
                    circle(p, x, y, size - t, BUTTON);
                }
                Control::West => {
                    let square = Rect::new(x - size * 0.9, y - size * 0.9, size * 1.8, size * 1.8);
                    p.canvas
                        .stroke_rect(square, t, Color::rgb(0xD0, 0x70, 0xB0));
                }
                _ => {
                    let color = Color::rgb(0x70, 0x90, 0xE0);
                    let reach = size * 0.85;
                    p.canvas.fill_polygon(
                        &segment((x - reach, y - reach), (x + reach, y + reach), t),
                        color,
                    );
                    p.canvas.fill_polygon(
                        &segment((x + reach, y - reach), (x - reach, y + reach), t),
                        color,
                    );
                }
            }
        }
    }
}

fn circle(p: &mut Painter<'_>, x: f32, y: f32, radius: f32, color: Color) {
    p.canvas.fill_round_rect(
        Rect::new(x - radius, y - radius, radius * 2.0, radius * 2.0),
        radius,
        color,
    );
}

/// An ellipse's outline turned `angle` radians clockwise about its centre.
fn ellipse(center: (f32, f32), rx: f32, ry: f32, angle: f32) -> Vec<(f32, f32)> {
    let (sin, cos) = angle.sin_cos();
    (0..48)
        .map(|i| {
            let t = i as f32 / 48.0 * std::f32::consts::TAU;
            let (x, y) = (rx * t.cos(), ry * t.sin());
            (center.0 + x * cos - y * sin, center.1 + x * sin + y * cos)
        })
        .collect()
}

/// A line from `a` to `b`, `thickness` wide, as a quad.
fn segment(a: (f32, f32), b: (f32, f32), thickness: f32) -> [(f32, f32); 4] {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx.hypot(dy).max(f32::EPSILON);
    let (nx, ny) = (
        -dy / length * thickness / 2.0,
        dx / length * thickness / 2.0,
    );
    [
        (a.0 + nx, a.1 + ny),
        (b.0 + nx, b.1 + ny),
        (b.0 - nx, b.1 - ny),
        (a.0 - nx, a.1 - ny),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ControlType;

    #[test]
    fn the_callouts_follow_the_control_type() {
        let text = Locale::new("en-US");
        let a = Bindings::default();
        assert_eq!(callout(Control::RightTrigger, &a, &text), "Fire");
        assert_eq!(
            callout(Control::South, &a, &text),
            "Interact / Confirm / Fire"
        );
        assert_eq!(callout(Control::Back, &a, &text), "Map");
        assert_eq!(
            callout(Control::LeftBumper, &a, &text),
            "Previous Page / Previous Target"
        );
        assert_eq!(callout(Control::LeftStick, &a, &text), "Move");
        assert_eq!(callout(Control::Start, &a, &text), "Menu");
        assert_eq!(callout(Control::RightStick, &a, &text), "");
        let b = Bindings::default().with_control_type(ControlType::B);
        assert_eq!(
            callout(Control::South, &b, &text),
            "Interact / Confirm / Fire"
        );
        assert_eq!(callout(Control::RightBumper, &b, &text), "Aim");
        let c = Bindings::default().with_control_type(ControlType::C);
        assert_eq!(
            callout(Control::East, &c, &text),
            "Interact / Confirm / Fire"
        );
        assert_eq!(callout(Control::South, &c, &text), "Cancel");
    }

    #[test]
    fn spread_labels_keep_apart_and_inside() {
        let flat = [0.0; 3];
        assert_eq!(
            spread(&[0.0, 10.0], &flat[..2], 5.0, -100.0, 100.0),
            [0.0, 10.0]
        );
        assert_eq!(
            spread(&[0.0, 0.0, 3.0], &flat, 3.0, -100.0, 100.0),
            [-2.0, 1.0, 4.0]
        );
        assert_eq!(spread(&[0.0, 0.0], &flat[..2], 4.0, 0.0, 100.0), [0.0, 4.0]);
        assert_eq!(
            spread(&[10.0, 10.0], &flat[..2], 4.0, 0.0, 10.0),
            [6.0, 10.0]
        );
        // A taller label takes more room.
        assert_eq!(
            spread(&[0.0, 0.0], &[2.0, 4.0], 1.0, -100.0, 100.0),
            [-2.0, 2.0]
        );
        assert_eq!(
            spread(&[2.0, 20.0], &[2.0, 4.0], 1.0, 0.0, 100.0),
            [2.0, 20.0]
        );
    }
}
