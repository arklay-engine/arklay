//! Oracle trace v1 emission (`oracle/trace-v1.md`).
//!
//! The shared, engine-neutral per-tick trace both the C++ reference oracle and
//! arklay emit: one line per gameplay tick, plus an `oracle-trace v1
//! engine=ark` header and an FNV-1a frame hash per line. [`TraceWriter`] is
//! the streaming file; [`TraceTick`] is the engine state one line carries.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::game::Entity;
use crate::render::Framebuffer;
use crate::state::RoomId;

/// FNV-1a 32-bit offset basis.
const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
/// FNV-1a 32-bit prime.
const FNV_PRIME: u32 = 0x0100_0193;

/// FNV-1a (32-bit) of a rendered frame.
///
/// The width and height go in as little-endian `u32`s, then every RGBA byte
/// row-major with the top row first. `Framebuffer::rgba` is already top-down.
pub fn frame_hash(framebuffer: &Framebuffer) -> u32 {
    let mut hash = FNV_OFFSET_BASIS;
    let mut feed = |byte: u8| {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    };
    for byte in framebuffer.width.to_le_bytes() {
        feed(byte);
    }
    for byte in framebuffer.height.to_le_bytes() {
        feed(byte);
    }
    for &byte in &framebuffer.rgba {
        feed(byte);
    }
    hash
}

/// An eight-word flag bank as the original's recorder writes it: memory's
/// bytes in hex, each word low byte first, so index `i` of the bank is bit
/// `0x80 >> (i & 7)` of byte `4 * (i >> 5) + 3 - ((i & 31) >> 3)`.
fn bank_bytes(words: &[u32; 8]) -> String {
    words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One tick's state for [`TraceWriter::write_tick`].
pub struct TraceTick<'a> {
    /// The live room identity.
    pub room: RoomId,
    /// The platform PRNG state (`GameState::rand_state`).
    pub rng: u32,
    /// The per-frame seed (`GameState::rand_seed`).
    pub rseed: u16,
    /// The room fade step (the reference's `g_fading_counter`).
    pub fade_counter: i16,
    /// The room fade accumulator (the reference's `g_fading_state`).
    pub fade_state: i16,
    /// Current camera cut index.
    pub cam: u8,
    /// RE2's player machine, which isn't in the entity: its `beh` and `as`,
    /// and the `mot` field's values.
    pub re2_player: Option<([u8; 2], [i32; 9])>,
    /// RE2's character in enemy slot 0: the `em` field's values.
    pub re2_character: Option<[i32; 16]>,
    /// RE2's `splayer_work`: the `es` field's values.
    pub re2_splayer: Option<[i32; 16]>,
    /// RE2's partner's own words: the `spl` field
    /// ([`crate::scd2::host::Re2State::partner_trace`]).
    pub re2_partner: Option<String>,
    /// What the partner's `Spl_enemy_ck` left: the `spe` field
    /// ([`crate::scd2::host::Re2State::partner_enemy_trace`]).
    pub re2_partner_enemies: Option<String>,
    /// The partner's climb and water words: the `spc` field
    /// ([`crate::scd2::host::Re2State::partner_climb_trace`]).
    pub re2_partner_climb: Option<String>,
    /// The zombies in play, as the oracle's `zm` field writes them.
    pub re2_zombies: Option<String>,
    /// Their hidden and coloured parts, as the oracle's `zp` field writes them.
    pub re2_zombie_parts: Option<String>,
    /// Where each of them is heading (`Dest`), as the oracle's `zd` field
    /// writes it.
    pub re2_zombie_dests: Option<String>,
    /// RE2's `Esp_` pool, the oracle's `esp` field
    /// ([`crate::esp2::Effects::trace_field`]).
    pub re2_effects: Option<String>,
    /// RE2's music: the `bgm` field.
    pub re2_music: Option<String>,
    /// RE2's game over ([`crate::die2::Die::trace_field`]).
    pub re2_die: Option<String>,
    /// An RE2 player's neck: flags, joint, whether it has a target, its turn.
    pub re2_neck: Option<String>,
    /// Where an RE2 player's head is in the world (the neck joint's `+0x5C`),
    /// or `-` before anything has posed it.
    pub re2_neck_position: Option<String>,
    /// RE2's voice-channel level, `xa_vol`'s (`0x693E88`).
    pub re2_xa_volume: Option<u8>,
    /// RE2's film state, the oracle's `movie`: `fg_system & 0x8200`,
    /// `Movie_idx` after `WinMain`'s decrement and `Movie_r0` (0 or 1).
    pub re2_movie: Option<String>,
    /// RE2's `Item_work`, the oracle's `inv`: the ten slots and the personal
    /// item as `id:count`, then the slot count.
    pub re2_inventory: Option<String>,
    /// RE2's map banks, the oracle's `mapfg`: bank 9's first five words
    /// (`fg_map`), then the first words of banks 32, 33 and 35, in hex.
    pub re2_map_flags: Option<String>,
    /// RE2: the status screen's item page (`ip`), as the end of the
    /// oracle's `gi`.
    pub re2_item_page: Option<String>,
    /// RE2: the status screen's FILE page and the files held (`fl`): the
    /// binder, slot, page, blink, zoom, turn and page x, then
    /// `File_tag_bk` in hex.
    pub re2_file: Option<String>,
    /// RE2's `Pad_set` repeat counter (`prep`): the flag, the counter, the
    /// mask and the delays.
    pub re2_pad_repeat: Option<String>,
    /// RE2: the item box's lid and page and the box's slots, the oracle's
    /// `ib` and `box` fields.
    pub re2_item_box: Option<String>,
    /// RE2: the message state (`fg_message`, the oracle's `msg`) and the
    /// doors' lock bits (`fg_key`, `fgkey`).
    pub re2_doors: Option<(u8, [u32; 2])>,
    /// RE2: whether a scene is up, `fg_status & 0x10` (the bars' bit), the
    /// trace's `scene` field.
    pub re2_scene: Option<bool>,
    /// RE2: the flag banks (`Re2State::banks`), for `fgstat` (bank 1's first
    /// word), `fgscn` (bank 3), `fgroom` (bank 5's first word) and `fgitem`
    /// (bank 8), in the original's recorder's format.
    pub re2_banks: Option<&'a [[u32; 8]; crate::scd2::host::BANKS]>,
    /// RE2's back routine slot 2, its state and the save place, the
    /// oracle's `tw` field ([`crate::game::GameState::re2_typewriter_trace`]).
    pub re2_typewriter: Option<String>,
    /// RE2's game mode (`Game`), the oracle's `gm` (`gi`'s first value).
    /// arklay follows it only on the typewriter's path.
    pub re2_game_mode: Option<u8>,
    /// RE2's save screen, the oracle's `sv` field: `Card_access`'s work
    /// while the typewriter's screen runs, `-` otherwise.
    pub re2_save_screen: Option<String>,
    /// RE2: the common flags (`fgcom`, `fg_common`'s eight words as their
    /// bytes) and the computer's state (`pc`, `-` without one).
    pub re2_computer: Option<String>,
    /// RE2: the drawer's hook, its state and the room objects' `Be_flg &
    /// 0x11`, the oracle's `hk`.
    pub re2_hikidashi: Option<String>,
    /// RE2: each room object's `Be_flg`, position, `Cdir_y`, `Push_cnt`,
    /// `At_sce_no` and `nFloor`, the oracle's `om`.
    pub re2_objects: Option<String>,
    /// RE2: the slot of the room object the player stands on (`pOn_om`),
    /// the oracle's `pon` (`-` for none).
    pub re2_on_object: Option<Option<u8>>,
    /// RE2: the script work `Sce_work_clr` resets, as the oracle's `fguse`
    /// (`fg_use`'s four words as their bytes) and `scv` (variables 0 to 3).
    pub re2_script_work: Option<String>,
    /// An RE2 room's sounds this tick: `type:index:volume:pan`, by `/`.
    pub re2_sounds: Option<String>,
    /// The player's `PlayerEntity.flags` byte (`GameState::player_flags`): the
    /// aim direction bits. The reference reads the player's status byte at
    /// entity +0x00, which the player entity shares with this flags byte, so
    /// slot 0's `status` field is this ORed with the entity's active bit.
    pub player_flags: u8,
    /// Every entity slot; slot 0 is the player and is always emitted.
    pub entities: &'a [Entity],
    /// [`frame_hash`] of the rendered frame.
    pub frame_hash: u32,
}

/// Where a headless oracle run writes its trace and capture frames.
#[derive(Debug, Clone, Copy, Default)]
pub struct TraceOptions<'a> {
    /// The trace file, or `None` for a plain run.
    pub path: Option<&'a Path>,
    /// Write a BMP every N traced ticks; `0` disables the cadence.
    pub capture_every: u32,
    /// Directory the `f%05d.bmp` captures are written to.
    pub capture_dir: Option<&'a Path>,
    /// Fixed ticks to run.
    pub ticks: u32,
    /// Scripted per-tick input playback (`--input`), or `None` for an idle run.
    pub input: Option<&'a crate::engine::InputScript>,
    /// Run as the game does with a sound device (`--sound`), for the
    /// oracle's `--sound` recordings: an RE2 door holds for the music.
    pub sound: bool,
    /// Mix the run's audio and write it here as a WAV (`--sound-wav`).
    pub sound_wav: Option<&'a Path>,
    /// Each RE2 voice line's length in ticks, in order (`--voice-ticks`).
    pub voice_ticks: Option<&'a Path>,
}

/// The streaming oracle trace file.
///
/// The header is written on creation; [`TraceWriter::write_tick`] emits one
/// line per traced gameplay tick with `t` starting at 0.
pub struct TraceWriter {
    out: BufWriter<File>,
    ticks: u32,
}

impl TraceWriter {
    /// Create `path` and write the engine header line.
    pub fn create(path: &Path) -> Result<Self> {
        let file =
            File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
        let mut out = BufWriter::new(file);
        out.write_all(b"oracle-trace v1 engine=ark\n")
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(Self { out, ticks: 0 })
    }

    /// The `t` of the next tick line.
    pub fn tick_index(&self) -> u32 {
        self.ticks
    }

    /// Emit one tick line.
    ///
    /// Slot 0 (the player) is always emitted; every other slot only while the
    /// entity's active status bit is set. `t` increments once per line.
    pub fn write_tick(&mut self, tick: &TraceTick<'_>) -> Result<()> {
        write!(
            self.out,
            "t={} room={}:{} rng={} rseed={} fade={}:{}",
            self.ticks,
            // The reference's own stage numbering: RE1's second mansion
            // visit is its stages 6 and 7. RE2 has one visit, so its rooms
            // that share a number with an RE1 second-visit room (119, 200)
            // keep their stage.
            if tick.re2_player.is_some() {
                usize::from(tick.room.stage)
            } else {
                tick.room.table_row() + 1
            },
            tick.room.room,
            tick.rng,
            tick.rseed,
            tick.fade_counter,
            tick.fade_state
        )?;
        match tick.entities.first() {
            Some(player) => write!(
                self.out,
                " p={},{},{},{},{},{},{}",
                player.pos[0],
                player.pos[1],
                player.pos[2],
                player.angle & 0x0FFF,
                tick.re2_player
                    .map_or(player.action_behavior, |(routines, _)| routines[0]),
                tick.re2_player
                    .map_or(player.action_state, |(routines, _)| routines[1]),
                player.health
            )?,
            None => write!(self.out, " p=")?,
        }
        write!(self.out, " cam={}", tick.cam)?;
        for (slot, entity) in tick.entities.iter().enumerate() {
            if slot > 0 && !entity.active() {
                continue;
            }
            // The reference reads one byte at 0x8A: `isBeingAttackedFlag` for
            // the player and `hit_state` for every other entity. The port
            // keeps the player's flag in its own field, so slot 0 emits that
            // one.
            let hit = if slot == 0 {
                entity.is_being_attacked
            } else {
                entity.hit_state
            };
            write!(
                self.out,
                " e={},{},{},{},{},{},{},{},{},{},{},{},{}",
                slot,
                entity.id,
                entity.pos[0],
                entity.pos[1],
                entity.pos[2],
                entity.angle & 0x0FFF,
                entity.state(),
                entity.action_behavior,
                entity.action_state,
                entity.health,
                hit,
                if slot == 0 {
                    entity.status_flags | tick.player_flags
                } else {
                    entity.status_flags
                },
                entity.action_ticks_counter
            )?;
        }
        write!(self.out, " f={}", tick.frame_hash)?;
        if let Some((_, motion)) = tick.re2_player {
            let motion = motion.map(|value| value.to_string()).join(",");
            write!(self.out, " mot={motion}")?;
        }
        if let Some(character) = tick.re2_character {
            let values = character.map(|value| value.to_string()).join(",");
            write!(self.out, " em={values}")?;
        }
        if let Some(character) = tick.re2_splayer {
            let values = character.map(|value| value.to_string()).join(",");
            write!(self.out, " es={values}")?;
        }
        if let Some(partner) = &tick.re2_partner {
            write!(self.out, " spl={partner}")?;
        }
        if let Some(enemies) = &tick.re2_partner_enemies {
            write!(self.out, " spe={enemies}")?;
        }
        if let Some(climb) = &tick.re2_partner_climb {
            write!(self.out, " spc={climb}")?;
        }
        if let Some(zombies) = &tick.re2_zombies {
            write!(self.out, " zm={zombies}")?;
        }
        if let Some(parts) = &tick.re2_zombie_parts {
            write!(self.out, " zp={parts}")?;
        }
        if let Some(dests) = &tick.re2_zombie_dests {
            write!(self.out, " zd={dests}")?;
        }
        if let Some(effects) = &tick.re2_effects {
            write!(self.out, " esp={effects}")?;
        }
        if let Some(music) = &tick.re2_music {
            write!(self.out, " bgm={music}")?;
        }
        if let Some(die) = &tick.re2_die {
            write!(self.out, " die={die}")?;
        }
        if let Some(neck) = &tick.re2_neck {
            write!(self.out, " neck={neck}")?;
        }
        if let Some(position) = &tick.re2_neck_position {
            write!(self.out, " pneck={position}")?;
        }
        if let Some(level) = tick.re2_xa_volume {
            write!(self.out, " xavol={level}")?;
        }
        if let Some(movie) = &tick.re2_movie {
            write!(self.out, " movie={movie}")?;
        }
        if let Some(inventory) = &tick.re2_inventory {
            write!(self.out, " inv={inventory}")?;
        }
        if let Some(flags) = &tick.re2_map_flags {
            write!(self.out, " mapfg={flags}")?;
        }
        if let Some(page) = &tick.re2_item_page {
            write!(self.out, " ip={page}")?;
        }
        if let Some(file) = &tick.re2_file {
            write!(self.out, " fl={file}")?;
        }
        if let Some(repeat) = &tick.re2_pad_repeat {
            write!(self.out, " prep={repeat}")?;
        }
        if let Some(fields) = &tick.re2_item_box {
            write!(self.out, " {fields}")?;
        }
        if let Some((message, [low, high])) = tick.re2_doors {
            write!(self.out, " msg={message:x} fgkey={low:08x}{high:08x}")?;
        }
        if let Some(scene) = tick.re2_scene {
            write!(self.out, " scene={}", u8::from(scene))?;
        }
        if let Some(banks) = tick.re2_banks {
            write!(
                self.out,
                " fgstat={:08x} fgscn={} fgroom={:08x} fgitem={}",
                banks[1][0],
                bank_bytes(&banks[3]),
                banks[5][0],
                bank_bytes(&banks[8])
            )?;
        }
        if let Some(fields) = &tick.re2_typewriter {
            write!(self.out, " {fields}")?;
        }
        if let Some(mode) = tick.re2_game_mode {
            write!(self.out, " gm={mode}")?;
        }
        if let Some(screen) = &tick.re2_save_screen {
            write!(self.out, " sv={screen}")?;
        }
        if let Some(fields) = &tick.re2_computer {
            write!(self.out, " {fields}")?;
        }
        if let Some(fields) = &tick.re2_hikidashi {
            write!(self.out, " {fields}")?;
        }
        if let Some(fields) = &tick.re2_objects {
            write!(self.out, " {fields}")?;
        }
        match tick.re2_on_object {
            Some(Some(index)) => write!(self.out, " pon={index}")?,
            Some(None) => write!(self.out, " pon=-")?,
            None => {}
        }
        if let Some(fields) = &tick.re2_script_work {
            write!(self.out, " {fields}")?;
        }
        if let Some(sounds) = &tick.re2_sounds {
            write!(
                self.out,
                " snd={}",
                if sounds.is_empty() { "-" } else { sounds }
            )?;
        }
        writeln!(self.out)?;
        self.ticks += 1;
        Ok(())
    }

    /// Flush the buffered lines.
    pub fn flush(&mut self) -> Result<()> {
        self.out.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::game::{ENTITY_STATUS_ACTIVE, GameState};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("arklay-trace-{}-{label}.txt", std::process::id()))
    }

    #[test]
    fn frame_hash_matches_the_reference_vectors() {
        let one = Framebuffer {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
            clip: None,
            view: crate::video::View::default(),
            list: None,
        };
        assert_eq!(frame_hash(&one), 0x6dc1_4b23);

        let two = Framebuffer {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
            clip: None,
            view: crate::video::View::default(),
            list: None,
        };
        assert_eq!(frame_hash(&two), 0xa4af_09a6);

        let blank = Framebuffer {
            width: 320,
            height: 240,
            rgba: vec![0; 320 * 240 * 4],
            clip: None,
            view: crate::video::View::default(),
            list: None,
        };
        assert_eq!(frame_hash(&blank), 0xfae3_db1a);
    }

    #[test]
    fn tick_lines_emit_the_player_and_active_slots_in_order() {
        let mut game = GameState::default();
        game.entities[0].id = 0;
        game.entities[0].pos = [17000, 0, 5000];
        // The yaw is emitted masked to 12 bits: the sign-extended form the
        // reference stores (`0xFC00`) prints as `0x0C00` (3072).
        game.entities[0].angle = 0xFC00;
        game.entities[0].health = 140;
        // Slot 0 is emitted even while its active bit is clear.
        game.entities[0].status_flags = 0;
        game.entities[1].id = 0x24;
        game.entities[1].pos = [1, 2, 3];
        game.entities[1].angle = 0x1C04;
        game.entities[1].health = 5;
        game.entities[1].hit_state = 6;
        game.entities[1].action_behavior = 7;
        game.entities[1].action_state = 8;
        game.entities[1].action_ticks_counter = 9;
        game.entities[1].status_flags = ENTITY_STATUS_ACTIVE;
        game.entities[2].id = 0x21;
        game.entities[2].pos = [10, 11, 12];
        game.entities[2].status_flags = 0;

        let path = temp_path("shape");
        let mut writer = TraceWriter::create(&path).unwrap();
        assert_eq!(writer.tick_index(), 0);
        let tick = TraceTick {
            room: RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 0,
                mansion2: false,
            },
            rng: 1,
            rseed: 2,
            fade_counter: 0,
            fade_state: -1,
            cam: 3,
            player_flags: 0,
            entities: &*game.entities,
            re2_player: None,
            re2_character: None,
            re2_splayer: None,
            re2_partner: None,
            re2_partner_enemies: None,
            re2_partner_climb: None,
            re2_zombies: None,
            re2_zombie_parts: None,
            re2_zombie_dests: None,
            re2_effects: None,
            re2_music: None,
            re2_die: None,
            re2_neck: None,
            re2_neck_position: None,
            re2_xa_volume: None,
            re2_movie: None,
            re2_inventory: None,
            re2_map_flags: None,
            re2_item_page: None,
            re2_file: None,
            re2_pad_repeat: None,
            re2_item_box: None,
            re2_doors: None,
            re2_scene: None,
            re2_banks: None,
            re2_typewriter: None,
            re2_game_mode: None,
            re2_save_screen: None,
            re2_computer: None,
            re2_hikidashi: None,
            re2_objects: None,
            re2_on_object: None,
            re2_script_work: None,
            re2_sounds: None,
            frame_hash: 0xDEAD_BEEF,
        };
        writer.write_tick(&tick).unwrap();
        assert_eq!(writer.tick_index(), 1);
        writer.flush().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            text,
            "oracle-trace v1 engine=ark\n\
             t=0 room=1:6 rng=1 rseed=2 fade=0:-1 p=17000,0,5000,3072,0,0,140 cam=3 \
             e=0,0,17000,0,5000,3072,0,0,0,140,0,0,0 \
             e=1,36,1,2,3,3076,0,7,8,5,6,1,9 f=3735928559\n"
        );
    }

    #[test]
    fn an_re2_room_prints_its_own_stage() {
        // Rooms 119 and 200 share their numbers with RE1 rooms only the
        // second mansion visit has; the reference prints RE2's 1:25 and 2:0.
        let game = GameState::default();
        let path = temp_path("re2-stage");
        let mut writer = TraceWriter::create(&path).unwrap();
        for (stage, room) in [(1, 0x19), (2, 0x00)] {
            let tick = TraceTick {
                room: RoomId {
                    stage,
                    room,
                    player_flag: 0,
                    mansion2: false,
                },
                rng: 0,
                rseed: 0,
                fade_counter: 0,
                fade_state: -1,
                cam: 0,
                player_flags: 0,
                entities: &*game.entities,
                re2_player: Some(([1, 0], [0; 9])),
                re2_character: None,
                re2_splayer: None,
                re2_partner: None,
                re2_partner_enemies: None,
                re2_partner_climb: None,
                re2_zombies: None,
                re2_zombie_parts: None,
                re2_zombie_dests: None,
                re2_effects: None,
                re2_music: None,
                re2_die: None,
                re2_neck: None,
                re2_neck_position: None,
                re2_xa_volume: None,
                re2_movie: None,
                re2_inventory: None,
                re2_map_flags: None,
                re2_item_page: None,
                re2_file: None,
                re2_pad_repeat: None,
                re2_item_box: None,
                re2_doors: None,
                re2_scene: None,
                re2_banks: None,
                re2_typewriter: None,
                re2_game_mode: None,
                re2_save_screen: None,
                re2_computer: None,
                re2_hikidashi: None,
                re2_objects: None,
                re2_on_object: None,
                re2_script_work: None,
                re2_sounds: None,
                frame_hash: 0,
            };
            writer.write_tick(&tick).unwrap();
        }
        writer.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let rooms: Vec<&str> = text
            .lines()
            .skip(1)
            .filter_map(|line| line.split(' ').nth(1))
            .collect();
        assert_eq!(rooms, ["room=1:25", "room=2:0"]);
    }

    #[test]
    fn an_re2_tick_writes_its_effects() {
        let game = GameState::default();
        let path = temp_path("re2-effects");
        let mut writer = TraceWriter::create(&path).unwrap();
        for effects in [Some("1/95:2,0,7,0,a003,0,100,0,0".to_string()), None] {
            let tick = TraceTick {
                room: RoomId {
                    stage: 2,
                    room: 0x00,
                    player_flag: 0,
                    mansion2: false,
                },
                rng: 0,
                rseed: 0,
                fade_counter: 0,
                fade_state: -1,
                cam: 0,
                player_flags: 0,
                entities: &*game.entities,
                re2_player: Some(([1, 0], [0; 9])),
                re2_character: None,
                re2_splayer: None,
                re2_partner: None,
                re2_partner_enemies: None,
                re2_partner_climb: None,
                re2_zombies: None,
                re2_zombie_parts: None,
                re2_zombie_dests: None,
                re2_effects: effects,
                re2_music: None,
                re2_die: None,
                re2_neck: None,
                re2_neck_position: None,
                re2_xa_volume: None,
                re2_movie: None,
                re2_inventory: None,
                re2_map_flags: None,
                re2_item_page: None,
                re2_item_box: None,
                re2_file: None,
                re2_pad_repeat: None,
                re2_doors: None,
                re2_scene: None,
                re2_banks: None,
                re2_typewriter: None,
                re2_game_mode: None,
                re2_save_screen: None,
                re2_computer: None,
                re2_hikidashi: None,
                re2_objects: None,
                re2_on_object: None,
                re2_script_work: None,
                re2_sounds: None,
                frame_hash: 0,
            };
            writer.write_tick(&tick).unwrap();
        }
        writer.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let lines: Vec<&str> = text.lines().skip(1).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(" esp=1/95:2,0,7,0,a003,0,100,0,0"));
        assert!(!lines[1].contains("esp="));
    }

    #[test]
    fn an_re2_tick_writes_the_object_stood_on() {
        let game = GameState::default();
        let path = temp_path("re2-on-object");
        let mut writer = TraceWriter::create(&path).unwrap();
        for on_object in [Some(Some(2)), Some(None), None] {
            let tick = TraceTick {
                room: RoomId {
                    stage: 2,
                    room: 0x00,
                    player_flag: 0,
                    mansion2: false,
                },
                rng: 0,
                rseed: 0,
                fade_counter: 0,
                fade_state: -1,
                cam: 0,
                player_flags: 0,
                entities: &*game.entities,
                re2_player: Some(([1, 0], [0; 9])),
                re2_character: None,
                re2_splayer: None,
                re2_partner: None,
                re2_partner_enemies: None,
                re2_partner_climb: None,
                re2_zombies: None,
                re2_zombie_parts: None,
                re2_zombie_dests: None,
                re2_effects: None,
                re2_music: None,
                re2_die: None,
                re2_neck: None,
                re2_neck_position: None,
                re2_xa_volume: None,
                re2_movie: None,
                re2_inventory: None,
                re2_map_flags: None,
                re2_item_page: None,
                re2_item_box: None,
                re2_file: None,
                re2_pad_repeat: None,
                re2_doors: None,
                re2_scene: None,
                re2_banks: None,
                re2_typewriter: None,
                re2_game_mode: None,
                re2_save_screen: None,
                re2_computer: None,
                re2_hikidashi: None,
                re2_objects: None,
                re2_on_object: on_object,
                re2_script_work: None,
                re2_sounds: None,
                frame_hash: 0,
            };
            writer.write_tick(&tick).unwrap();
        }
        writer.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let lines: Vec<&str> = text.lines().skip(1).collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with(" pon=2"));
        assert!(lines[1].ends_with(" pon=-"));
        assert!(!lines[2].contains("pon="));
    }

    #[test]
    fn tick_index_increments_once_per_line() {
        let game = GameState::default();
        let path = temp_path("counter");
        let mut writer = TraceWriter::create(&path).unwrap();
        let tick = TraceTick {
            room: RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 0,
                mansion2: false,
            },
            rng: 0,
            rseed: 0,
            fade_counter: 0,
            fade_state: -1,
            cam: 0,
            player_flags: 0,
            entities: &*game.entities,
            re2_player: None,
            re2_character: None,
            re2_splayer: None,
            re2_partner: None,
            re2_partner_enemies: None,
            re2_partner_climb: None,
            re2_zombies: None,
            re2_zombie_parts: None,
            re2_zombie_dests: None,
            re2_effects: None,
            re2_music: None,
            re2_die: None,
            re2_neck: None,
            re2_neck_position: None,
            re2_xa_volume: None,
            re2_movie: None,
            re2_inventory: None,
            re2_map_flags: None,
            re2_item_page: None,
            re2_file: None,
            re2_pad_repeat: None,
            re2_item_box: None,
            re2_doors: None,
            re2_scene: None,
            re2_banks: None,
            re2_typewriter: None,
            re2_game_mode: None,
            re2_save_screen: None,
            re2_computer: None,
            re2_hikidashi: None,
            re2_objects: None,
            re2_on_object: None,
            re2_script_work: None,
            re2_sounds: None,
            frame_hash: 0,
        };
        for index in 0..3 {
            assert_eq!(writer.tick_index(), index);
            writer.write_tick(&tick).unwrap();
        }
        writer.flush().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("oracle-trace v1 engine=ark"));
        assert!(lines[1].starts_with("t=0 "));
        assert!(lines[2].starts_with("t=1 "));
        assert!(lines[3].starts_with("t=2 "));
    }

    #[test]
    fn neck_position_follows_the_neck() {
        let game = GameState::default();
        let path = temp_path("pneck");
        let mut writer = TraceWriter::create(&path).unwrap();
        let mut banks = [[0u32; 8]; crate::scd2::host::BANKS];
        banks[1][0] = 0x8000_0010;
        banks[3][3] = 0x0800_0000; // 3:100
        banks[3][4] = 0x8000_0000; // 3:128
        banks[5][0] = 0x0200_2000; // 5:6 and 5:18
        banks[5][1] = 1; // `fg_tick`, which isn't traced
        banks[8][4] = 0x0004_0000; // 8:141
        let mut tick = TraceTick {
            room: RoomId {
                stage: 1,
                room: 0x1B,
                player_flag: 0,
                mansion2: false,
            },
            rng: 0,
            rseed: 0,
            fade_counter: 0,
            fade_state: -1,
            cam: 0,
            player_flags: 0,
            entities: &*game.entities,
            re2_player: None,
            re2_character: None,
            re2_splayer: None,
            re2_partner: None,
            re2_partner_enemies: None,
            re2_partner_climb: None,
            re2_zombies: None,
            re2_zombie_parts: None,
            re2_zombie_dests: None,
            re2_effects: None,
            re2_music: None,
            re2_die: None,
            re2_neck: Some("0,8,0,0,0".to_string()),
            re2_neck_position: Some("-".to_string()),
            re2_xa_volume: None,
            re2_movie: None,
            re2_inventory: None,
            re2_map_flags: None,
            re2_item_page: None,
            re2_file: None,
            re2_pad_repeat: None,
            re2_item_box: None,
            re2_doors: None,
            re2_scene: None,
            re2_banks: None,
            re2_typewriter: None,
            re2_game_mode: None,
            re2_save_screen: None,
            re2_computer: None,
            re2_hikidashi: None,
            re2_objects: None,
            re2_on_object: None,
            re2_script_work: None,
            re2_sounds: Some(String::new()),
            frame_hash: 0,
        };
        writer.write_tick(&tick).unwrap();
        tick.re2_neck_position = Some("-9500,-2514,-20302".to_string());
        writer.write_tick(&tick).unwrap();
        tick.re2_scene = Some(true);
        writer.write_tick(&tick).unwrap();
        tick.re2_banks = Some(&banks);
        writer.write_tick(&tick).unwrap();
        writer.flush().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[1].ends_with(" neck=0,8,0,0,0 pneck=- snd=-"));
        assert!(lines[2].ends_with(" neck=0,8,0,0,0 pneck=-9500,-2514,-20302 snd=-"));
        assert!(!lines[2].contains("scene="));
        assert!(lines[3].ends_with(" pneck=-9500,-2514,-20302 scene=1 snd=-"));
        // An RE1 tick (no banks) writes none of the four.
        for field in [" fgstat=", " fgscn=", " fgroom=", " fgitem="] {
            assert!(!lines[3].contains(field), "{field}");
        }
        // The banks as the original's recorder writes them: one-word banks
        // as a number, eight-word banks as memory's bytes, low byte first.
        let zeros = |n: usize| "00".repeat(n);
        let scn = format!(
            "{}00000008{}",
            zeros(12),
            "00000080".to_owned() + &zeros(12)
        );
        let item = format!("{}00000400{}", zeros(16), zeros(12));
        let expected = format!(" fgstat=80000010 fgscn={scn} fgroom=02002000 fgitem={item}");
        assert!(lines[4].contains(&expected), "{}", lines[4]);
    }

    #[test]
    fn the_typewriter_fields_follow_the_item_box() {
        let game = GameState::default();
        let path = temp_path("tw");
        let mut writer = TraceWriter::create(&path).unwrap();
        let tick = TraceTick {
            room: RoomId {
                stage: 1,
                room: 0,
                player_flag: 0,
                mansion2: false,
            },
            rng: 0,
            rseed: 0,
            fade_counter: 0,
            fade_state: -1,
            cam: 0,
            player_flags: 0,
            entities: &*game.entities,
            re2_player: None,
            re2_character: None,
            re2_splayer: None,
            re2_partner: None,
            re2_partner_enemies: None,
            re2_partner_climb: None,
            re2_zombies: None,
            re2_zombie_parts: None,
            re2_zombie_dests: None,
            re2_effects: None,
            re2_music: None,
            re2_die: None,
            re2_neck: None,
            re2_neck_position: None,
            re2_xa_volume: None,
            re2_movie: None,
            re2_inventory: None,
            re2_map_flags: None,
            re2_item_page: None,
            re2_file: None,
            re2_pad_repeat: None,
            re2_item_box: Some("ib=1 box=-".to_string()),
            re2_doors: Some((0x81, [0, 0])),
            re2_scene: None,
            re2_banks: None,
            re2_typewriter: Some("tw=4e9930,1,1".to_string()),
            re2_game_mode: Some(0),
            re2_save_screen: Some("-".to_string()),
            re2_computer: None,
            re2_hikidashi: None,
            re2_objects: None,
            re2_on_object: None,
            re2_script_work: None,
            re2_sounds: Some(String::new()),
            frame_hash: 0,
        };
        writer.write_tick(&tick).unwrap();
        writer.flush().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[1].ends_with(
                " ib=1 box=- msg=81 fgkey=0000000000000000 tw=4e9930,1,1 gm=0 sv=- snd=-"
            )
        );
    }

    #[test]
    fn the_partner_field_is_written_as_spl() {
        let game = GameState::default();
        let path = temp_path("spl");
        let mut writer = TraceWriter::create(&path).unwrap();
        let tick = TraceTick {
            room: RoomId {
                stage: 2,
                room: 0x1B,
                player_flag: 1,
                mansion2: false,
            },
            rng: 0,
            rseed: 0,
            fade_counter: 0,
            fade_state: -1,
            cam: 0,
            player_flags: 0,
            entities: &*game.entities,
            re2_player: None,
            re2_character: None,
            re2_splayer: Some([69, 0, 0, 0, 0, 1, 0, 0, 0, 2, 1, 6, 0, 0, 0, 0]),
            re2_partner: Some("400,1,2,0,3".to_string()),
            re2_partner_enemies: None,
            re2_partner_climb: None,
            re2_zombies: Some("-".to_string()),
            re2_zombie_parts: None,
            re2_zombie_dests: None,
            re2_effects: None,
            re2_music: None,
            re2_die: None,
            re2_neck: None,
            re2_neck_position: None,
            re2_xa_volume: None,
            re2_movie: None,
            re2_inventory: None,
            re2_map_flags: None,
            re2_item_page: None,
            re2_file: None,
            re2_pad_repeat: None,
            re2_item_box: None,
            re2_doors: None,
            re2_scene: None,
            re2_banks: None,
            re2_typewriter: None,
            re2_game_mode: None,
            re2_save_screen: None,
            re2_computer: None,
            re2_hikidashi: None,
            re2_objects: None,
            re2_on_object: None,
            re2_script_work: None,
            re2_sounds: None,
            frame_hash: 0,
        };
        writer.write_tick(&tick).unwrap();
        writer.flush().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let line = text.lines().nth(1).unwrap();
        assert!(
            line.contains(" es=69,0,0,0,0,1,0,0,0,2,1,6,0,0,0,0 spl=400,1,2,0,3 zm=-"),
            "{line}"
        );
    }
}
