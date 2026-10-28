//! ROOM SELECT: the pack's rooms as a scrollable grid of pictures, one
//! stage per tab: RE1's seven stage digits always, then every further stage
//! the pack carries (RE2's letter stages `A`-`G`).
//!
//! Each cell shows the room's first camera cut, its `room3` id and its name;
//! the grid takes as many columns as the window's width allows. The arrows
//! move the cursor (up/down by a row), the page keys change stage and confirm
//! jumps through the session's `debug_jump`, in the RDT variant of the
//! pending character when the pack carries it.

use std::collections::BTreeMap;

use super::widgets::{self, Grid, Scroll, grid_step};
use super::{Cx, Hit, Step, fit};
use crate::gui::canvas::{Color, Rect};
use crate::gui::text::Align;
use crate::gui::theme::{Role, palette};
use crate::gui::widgets::Painter;
use crate::pack::Pack;
use crate::state::RoomId;
use crate::ui::UiInput;

/// Thumbnails loaded per painted frame, so the grid stays responsive.
const LOADS_PER_FRAME: usize = 4;

/// A stage tab's label: the language's `UI_DEBUG_STAGE_<digit>`, else the
/// stage's digit (`Stage A`).
fn stage_label(cx: &Cx<'_>, stage: u8) -> String {
    let digit = RoomId {
        stage,
        ..RoomId::default()
    }
    .stage_char()
    .to_ascii_uppercase();
    let label = cx.label(&format!("UI_DEBUG_STAGE_{digit}"));
    if label.is_empty() {
        format!("Stage {digit}")
    } else {
        label
    }
}

/// One stage/room the pack carries and the RDT player variants it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomVariants {
    /// Stage digit (1-7).
    pub stage: u8,
    /// Room index.
    pub room: u8,
    /// The `player_flag` digits present, sorted.
    pub players: Vec<u8>,
}

impl RoomVariants {
    /// The room id in `preferred`'s variant when the pack carries it, else the
    /// lowest digit.
    pub fn id(&self, preferred: u8) -> RoomId {
        let player_flag = if self.players.contains(&preferred) {
            preferred
        } else {
            self.players[0]
        };
        RoomId {
            stage: self.stage,
            room: self.room,
            player_flag,
        }
    }
}

/// Enumerate the pack's rooms (`room/<id>/room.xml` or `room.rdt`), sorted by
/// stage/room with every room's player variants collected.
pub fn room_variants(pack: &Pack) -> Vec<RoomVariants> {
    let mut rooms: BTreeMap<(u8, u8), Vec<u8>> = BTreeMap::new();
    for id in crate::room::ids(pack) {
        let players = rooms.entry((id.stage, id.room)).or_default();
        if !players.contains(&id.player_flag) {
            players.push(id.player_flag);
        }
    }
    rooms
        .into_iter()
        .map(|((stage, room), mut players)| {
            players.sort_unstable();
            RoomVariants {
                stage,
                room,
                players,
            }
        })
        .collect()
}

/// Enumerate the pack's rooms deduped to one [`RoomId`] per room, picking
/// `preferred_player`'s RDT variant when the pack carries it and the lowest
/// digit otherwise.
pub fn room_entries(pack: &Pack, preferred_player: u8) -> Vec<RoomId> {
    room_variants(pack)
        .iter()
        .map(|room| room.id(preferred_player))
        .collect()
}

/// The ROOM SELECT tool's state.
pub struct RoomSelect {
    /// Each tab's stage number.
    stage_ids: Vec<u8>,
    /// Each tab's rooms.
    stages: Vec<Vec<RoomVariants>>,
    /// The session's room as the menu last saw it.
    current: (u8, u8),
    stage: usize,
    cells: Vec<usize>,
    /// Each stage's grid scroll, in rows.
    scroll: Vec<Scroll>,
    /// The grid's columns as last laid out.
    columns: usize,
}

impl RoomSelect {
    /// Split the pack's rooms into stages and put the cursor on `current`.
    pub fn new(pack: &Pack, current: RoomId) -> Self {
        let rooms = room_variants(pack);
        let mut stage_ids: Vec<u8> = (1..=RoomId::MAX_STAGE).collect();
        for room in &rooms {
            if !stage_ids.contains(&room.stage) {
                stage_ids.push(room.stage);
            }
        }
        let mut stages = vec![Vec::new(); stage_ids.len()];
        for room in rooms {
            if let Some(tab) = stage_ids.iter().position(|&stage| stage == room.stage) {
                stages[tab].push(room);
            }
        }
        let tabs = stage_ids.len();
        let mut select = Self {
            stage_ids,
            stages,
            current: (0, 0),
            stage: 0,
            cells: vec![0; tabs],
            scroll: vec![Scroll::default(); tabs],
            columns: 4,
        };
        select.refresh(current);
        select
    }

    /// The cursor's stage tab.
    pub fn column(&self) -> usize {
        self.stage
    }

    /// The cursor's cell inside its stage.
    pub fn row(&self) -> usize {
        self.cells[self.stage]
    }

    /// The room under the cursor (`None` in an empty stage).
    pub fn selected(&self) -> Option<&RoomVariants> {
        self.stages[self.stage].get(self.cells[self.stage])
    }

    /// Follow the session after the menu was closed: when `current` changed
    /// (a debug jump, a door), re-mark it and put the cursor on it. A reopen
    /// with the same current room keeps the cursor where it was.
    pub fn refresh(&mut self, current: RoomId) {
        let room = (current.stage, current.room);
        if self.current == room {
            return;
        }
        self.current = room;
        let Some(stage) = self.stage_ids.iter().position(|&stage| stage == room.0) else {
            return;
        };
        if let Some(cell) = self.stages[stage]
            .iter()
            .position(|entry| entry.room == room.1)
        {
            self.stage = stage;
            self.cells[stage] = cell;
            self.scroll[stage].follow();
        }
    }

    /// Put the cursor on cell `index`.
    pub(super) fn point(&mut self, index: usize) {
        if index < self.stages[self.stage].len() {
            self.cells[self.stage] = index;
        }
    }

    /// Show stage tab `stage`.
    pub(super) fn set_stage(&mut self, stage: usize) {
        self.stage = stage.min(self.stages.len() - 1);
    }

    /// Scroll the grid by `rows` without moving the cursor.
    pub(super) fn scroll_by(&mut self, rows: isize) {
        self.scroll[self.stage].by(rows);
    }

    /// One input edge: the jump target in `character`'s variant on confirm.
    pub(super) fn handle_input(&mut self, input: UiInput, character: u8) -> Step {
        if input.cancel {
            return Step::Back;
        }
        if input.page_left {
            let tabs = self.stages.len();
            self.stage = (self.stage + tabs - 1) % tabs;
            return Step::Stay;
        }
        if input.page_right {
            self.stage = (self.stage + 1) % self.stages.len();
            return Step::Stay;
        }
        if input.confirm {
            return match self.selected() {
                Some(room) => Step::Event(super::DebugMenuEvent::Jump(room.id(character))),
                None => Step::Stay,
            };
        }
        let count = self.stages[self.stage].len();
        let cell = &mut self.cells[self.stage];
        if let Some(next) = grid_step(*cell, count, self.columns, input) {
            *cell = next;
            self.scroll[self.stage].follow();
        }
        Step::Stay
    }

    /// Draw the stage tabs and the visible grid into `area`.
    pub(super) fn draw(&mut self, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let labels: Vec<String> = self
            .stage_ids
            .iter()
            .map(|&stage| stage_label(cx, stage))
            .collect();
        let empty: Vec<bool> = self.stages.iter().map(Vec::is_empty).collect();
        let tab_h = widgets::tabs(p, cx, area, &labels, self.stage, &empty);
        let body = Rect::new(area.x, area.y + tab_h, area.w, area.h - tab_h);
        let rooms = &self.stages[self.stage];
        if rooms.is_empty() {
            let none = cx.label("UI_DEBUG_NO_ROOMS");
            p.text(
                Role::Row,
                body.center_x(),
                body.y + u(60.0),
                Align::Center,
                palette::TEXT_DIM,
                &none,
            );
            return;
        }
        let grid = Grid::new(p, body, 300.0, true);
        let columns = grid.columns;
        self.columns = columns;
        let cell = self.cells[self.stage].min(rooms.len() - 1);
        let rows = rooms.len().div_ceil(columns);
        let top = self.scroll[self.stage].layout(cell / columns, grid.rows, rows);
        let mut loads = 0;
        for (index, room) in rooms
            .iter()
            .enumerate()
            .skip(top * columns)
            .take(grid.rows * columns)
        {
            let (rect, thumb) = grid.cell(index - top * columns);
            let selected = index == cell;
            let id = room.id(0);
            if !cx.art.has_thumbnail(id) {
                if loads < LOADS_PER_FRAME {
                    cx.art.load_thumbnail(cx.pack, id);
                    loads += 1;
                } else {
                    cx.more = true;
                }
            }
            let picture = cx.art.thumbnail(id);
            widgets::grid_picture(p, thumb, picture.as_ref(), selected);
            if (room.stage, room.room) == self.current {
                let tag = cx.gui("debug.you_are_here");
                let tag_w = p.measure(Role::Body, &tag) + u(20.0);
                let tag_rect = Rect::new(
                    thumb.x + u(8.0),
                    thumb.y + u(8.0),
                    tag_w,
                    p.line_height(Role::Body) * 1.1,
                );
                p.canvas
                    .fill_round_rect(tag_rect, u(6.0), Color::rgba(0x20, 0x7A, 0x3A, 0xE0));
                p.text(
                    Role::Body,
                    tag_rect.center_x(),
                    tag_rect.center_y(),
                    Align::Center,
                    palette::TEXT,
                    &tag,
                );
            }
            let room3 = id.room3();
            let label_y = (thumb.bottom() + rect.bottom()) / 2.0;
            let color = if selected {
                palette::TEXT
            } else {
                palette::TEXT_DIM
            };
            let id_w = p.text(Role::Body, rect.x, label_y, Align::Left, color, &room3);
            let name = cx.room_name(&room3);
            let name_x = rect.x + id_w + u(12.0);
            let name = fit(p, Role::Body, &name, rect.right() - name_x);
            p.text(Role::Body, name_x, label_y, Align::Left, color, &name);
            cx.hits.push((rect, Hit::Item(index)));
        }
        grid.scrollbar(p, top, rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(rooms: &[&str]) -> Pack {
        let mut writer = crate::pack::PackWriter::new();
        for room in rooms {
            writer
                .add(&format!("room/{room}/room.rdt"), vec![0])
                .unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn letter_stages_get_their_own_tabs() {
        let pack = pack(&["1000", "1001", "a000", "g010"]);
        let mut select = RoomSelect::new(&pack, RoomId::parse("G010").unwrap());
        assert_eq!(select.stage_ids, [1, 2, 3, 4, 5, 6, 7, 10, 16]);
        assert_eq!(select.column(), 8, "the cursor follows the current room");
        assert_eq!(select.selected().map(|room| room.room), Some(0x01));
        let right = UiInput {
            page_right: true,
            ..UiInput::default()
        };
        select.handle_input(right, 0);
        assert_eq!(select.column(), 0, "the tabs wrap");
    }

    #[test]
    fn an_re1_pack_keeps_seven_tabs() {
        let pack = pack(&["1000", "7001"]);
        let select = RoomSelect::new(&pack, RoomId::parse("1000").unwrap());
        assert_eq!(select.stage_ids, [1, 2, 3, 4, 5, 6, 7]);
    }
}
