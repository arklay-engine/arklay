//! The game side of the RE2 script VM: the flag banks and script variables,
//! and the instructions that build the room's interaction areas.

use super::opcode::Op;
use super::vm::{Host, Step};
use crate::game::{Door, GameState, ROOM_ACTION_SLOTS, RoomAction, RoomActionKind};
use crate::state::Zone;

/// The flag banks `ck` and `set` index (`Flg_addr`).
const BANKS: usize = 32;

/// RE2's script state, which has no RE1 counterpart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Re2State {
    /// `fg_room_enemy`'s low byte: what the room's dogs told each other.
    pub room_enemy: u8,
    /// The flag banks; a bit is `0x80000000 >> (index & 31)` of word
    /// `(index >> 5) & 7`.
    pub banks: [[u32; 8]; BANKS],
    /// The script variables (`scd_variables`).
    pub vars: [u16; 256],
    /// The interaction areas' gates, by action slot.
    pub gates: [ActionGate; ROOM_ACTION_SLOTS],
    /// `door_locks`: the unlocked doors' bits.
    pub door_locks: [u32; 2],
    /// `sce_rnd`'s state.
    pub random: u32,
    /// `Xa_control`'s voice line.
    pub voice: VoiceLine,
    /// The camera `cut_old` returns to.
    pub cut_old: u8,
    /// Interaction-area events waiting for the VM (`Sce_event`): task, event.
    pub pending_events: Vec<(u8, u8)>,
    /// `Sce_Damage` areas the player's position probe hit this tick: the box,
    /// the kind and the damage. The tick applies them to the player.
    pub pending_damage: Vec<([i16; 4], u8, i16)>,
    /// Camera-table edits waiting for the room (`cut_be_set`, `cut_replace`).
    pub pending_cuts: Vec<CutEdit>,
    /// Collision-record edits waiting for the room (`sca_id_set`).
    pub pending_sca: Vec<(u8, u16)>,
    /// A door has just handed back, and `fg_stop` hasn't come down yet.
    pub door_settling: bool,
    /// The same tick, as the enemy slots see it.
    pub door_stop: bool,
    /// What each task's `work_set` selected.
    pub work: [Work; TASK_SLOTS],
    /// Each task's `Speed_set`s: speeds of position and rotation, then what
    /// `Add_aspeed` adds to them. `Work_set` clears them.
    pub speeds: [[i16; 12]; TASK_SLOTS],
    /// A scripted move of the player, applied at the next tick.
    pub pending_player: PendingPlayer,
    /// `Key`, the pad as the scripts read it.
    pub key: u16,
    /// `Key_trg`, the keys that rose this tick.
    pub key_trg: u16,
    /// A stop was masking `key` last tick.
    pub key_was_stopped: bool,
    /// Characters in the enemy slots.
    pub characters: Characters,
    /// Room flags of `plc_dest`s on works that aren't modelled, not yet raised.
    pub unmodelled_arrivals: Vec<u8>,
    /// A room's load is running its init scripts.
    pub room_loading: bool,
    /// The player's `PLAYER_WORK`.
    pub player: crate::player2::Work,
    /// The room's running scripts.
    pub vm: Option<Box<super::vm::Vm>>,
    /// Scenario B (`FG_STATUS_SCENARIO`).
    pub scenario_b: bool,
    /// The room music table (`bgm_tbl`).
    pub bgm: crate::bgm2::BgmTable,
    /// The main music playing.
    pub main_music: Option<u8>,
    /// The music and ambience's channels.
    pub music: crate::bgm2::Music,
    /// `Esp_`: the effect sprites.
    pub effects: crate::esp2::Effects,
    /// `se_on`'s sounds, not yet played.
    pub sounds: Vec<SoundRequest>,
    /// `sce_scr_move`: how far the background has scrolled down.
    pub scroll: i16,
    /// `Rnd`, for what's ported to its stream. A save doesn't hold it: the
    /// room's load seeds it.
    pub rnd: crate::zombie2::Rnd,
    /// `DWORD_006894f8`: the `Work_no` of the licker whose tongue is out.
    pub tongue_owner: u8,
    /// `BYTE_00989eeb`: the room's enemies so far.
    pub enemies: u8,
    /// `Se_tmp0`: the sound types of the room's enemies, as `sce_em_set` saw them.
    pub sound_types: [u8; 2],
    /// `BYTE_0098e543`: the enemy sound bank `Sce_se_set` chose for the room.
    pub enemy_bank: Option<u8>,
    /// The room's load has yet to finish: its last pass runs `Esp_move`.
    pub load_pass: bool,
    /// `Sce_espr_on`s whose `Get_matrix` is someone's, for the characters' models
    /// to answer.
    pub pending_esprs: Vec<(EsprCall, EsprMatrix)>,
    /// `Sce_fade_set`s and `Sce_fade_adjust`s to fade 0, the session's own,
    /// for the end of the tick.
    pub screen_fades: Vec<FadeWrite>,
    /// Fades 1 to 3, over fade 0, and what each drew this tick.
    pub fades: [crate::transition::Fade; 3],
    pub fades_drawn: [Option<crate::transition::Overlay>; 3],
}

/// What a script did to a fade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FadeWrite {
    Set { kind: u8, add: i16 },
    Adjust(i16),
}

impl FadeWrite {
    pub fn apply(self, fade: &mut crate::transition::Fade) {
        match self {
            Self::Set { kind, add } => fade.set_scripted(kind, add),
            Self::Adjust(state) => fade.set_state(state),
        }
    }
}

/// One sound `se_on` asked for, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundRequest {
    pub id: crate::se2::SoundId,
    pub pos: [i32; 3],
}

/// `pos_set` and `dir_set` on the player.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PendingPlayer {
    pub pos: Option<[i32; 3]>,
    pub angle: Option<u16>,
}

/// An edit of the room's camera switch zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutEdit {
    /// `obj_model_set`'s attribute 2: the object's model blends.
    BlendObject { slot: u8 },
    /// `cut_be_set`: entry `index` of camera `cut`'s zones gets gate `value`.
    SetGate { cut: u8, index: u8, value: u8 },
    /// `cut_replace`: swap two values across every zone's gate and floor.
    Replace { a: u8, b: u8 },
    /// `light_*_set`: light `index` of camera `cut`'s record.
    Light {
        cut: u8,
        index: u8,
        change: LightChange,
    },
}

/// What `light_pos_set`, `light_kido_set` and `light_color_set` write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightChange {
    Pos { axis: u8, value: i16 },
    Radius(u16),
    Color([u8; 3]),
}

/// A `work_set` target: the player, or a character in an enemy slot.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    #[default]
    Player,
    Character(u8),
    /// `pOm`'s slot.
    Object(u8),
    Other,
}

/// What `kage_set` asks of a body's `KAGE_WORK`. A script makes a corpse's pool
/// of blood with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kage {
    pub color: u32,
    pub half: (i32, i32),
    /// `Off_x` and `Off_z`, which `MulKage` turns with the body.
    pub off: (i32, i32),
}

/// `_Spl`'s character: a body and a `PLAYER_WORK`, driven by the room's scripts.
#[derive(Debug, Clone)]
pub struct Character {
    pub id: u8,
    /// `Be_flg`, whose first bit is whether it's in play.
    pub be_flg: u32,
    pub work: crate::player2::Work,
    pub body: crate::player::PlayerState,
    /// `Type`, whose `0x400` gives it to `Spl_Auto` rather than the scripts.
    pub kind: u16,
    /// `sce_em_set`'s last two words: `Em_init_move`'s second `_Spl`.
    pub start: (u16, u16),
    /// `+0x1fa`: its sound type, which chooses the room's enemy bank.
    pub sound_type: u8,
    /// The room's load places it, rather than a script during play.
    pub with_room: bool,
    /// `+0x1F0`: how far it was from the player when it last ran.
    pub distance: u32,
    /// `pSub1_kan_t_ptr` once a zombie has hold of it: that zombie's own.
    pub held_bank: Option<std::sync::Arc<crate::game::PlayerDamageBank>>,
    /// What it needs if `Em_Zombie` runs it.
    pub zombie: Option<crate::zombie2::Zombie>,
    /// The last `kage_set`, in place of `Em_init`'s shadow.
    pub kage: Option<Kage>,
}

/// `xa_on`'s line, which `wsleep` waits out. A save doesn't hold it, so a line
/// doesn't outlast the load.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceLine {
    /// The pack entry `xa_on` asked for, until the engine has its length.
    pub request: Option<String>,
    /// Ticks of `Xa_control` left: its two steps either side of the count.
    pub ticks: u32,
}

/// The enemy slots' characters. A save doesn't hold them: the room's scripts
/// make them again.
#[derive(Debug, Clone, Default)]
pub struct Characters(pub Vec<Option<Box<Character>>>);

impl PartialEq for Characters {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Characters {}

/// The bytes of [`Re2State::to_save_bytes`].
const SAVE_LEN: usize = (BANKS * 8 + 2) * 4 + 512 + crate::bgm2::ROOMS * 2 + 5;

/// The script's task slots.
const TASK_SLOTS: usize = super::vm::TASKS;
/// `set 2, 7`: `FG_STOP_DISABLE_INPUT`, a cutscene holding the player's input.
const STOP_BANK: u8 = 2;
/// `Pri_be_flg`: a bank per camera, 13 onwards, with a bit per mask group.
const MASK_BANK: usize = 13;
/// How many cameras `Pri_be_flg` has a bank for.
const MASK_BANKS: usize = 16;
const STOP_INPUT: u8 = 7;
/// `Spl_Init`'s `Atd[0].W`, by id from 0x40.
const CHARACTER_SIZES: [u16; 28] = [
    500, 350, 500, 350, 86, 250, 86, 350, 450, 350, 450, 350, 0, 0, 0, 250, 450, 350, 450, 350,
    450, 350, 450, 350, 450, 350, 450, 450,
];
/// `parts_set`'s case for a part's `Be_flg`.
const PARTS_SET_FLAGS: i64 = 2;
/// `fg_status`'s bank, whose bit picks the second `fg_enemy`.
pub const STATUS_BANK: u8 = 1;
const STATUS_ENEMY_FLAGS: u8 = 6;
/// `fg_status`'s bit for the character whose voices play: 1 is Claire.
pub const STATUS_PLAYER: u8 = 0;
/// `fg_status`'s `0x20`: a voice line is playing, and `wsleep` waits.
pub const STATUS_VOICE: u8 = 26;
/// A scene is running: the bars come down and the menus stay shut.
pub const STATUS_SCENE: u8 = 27;
/// `fg_enemy_0`'s bank: stages below this keep theirs there, `Sce_em_set` says.
const ENEMY_BANK: u8 = 6;
const CAMPAIGN_ENEMY_STAGES: u8 = 3;
/// `Sce_em_set`'s flag of someone who can't die.
pub const NO_ENEMY_FLAG: u8 = 0xFF;
/// `splayer_work`'s place among the characters, after `enemy_work`'s 16.
pub const SPLAYER_SLOT: usize = 16;
const CHARACTER_SLOTS: usize = SPLAYER_SLOT + 1;
/// `sce_em_set`'s slot for `splayer_work`.
const SPLAYER_OPERAND: i64 = 255;
/// `FlagGroup::Message`, `fg_message`: bit 31 is a question's answer, `no`.
const MESSAGE_BANK: u8 = 11;
/// The item-collected flag bank (`FlagGroup::Item`).
pub const ITEM_BANK: u8 = 8;
/// The ink ribbon's item id.
pub const ITEM_INK_RIBBON: u8 = 0x1E;

/// Whether picked-up `item` joins a stack: the ammunition and ink ribbons.
pub fn item_stacks(item: u8) -> bool {
    (0x14..=ITEM_INK_RIBBON).contains(&item)
}

/// `Sce_at`'s door lock key values.
pub const KEY_UNLOCK: u8 = 0xFE;
pub const KEY_LOCKED: u8 = 0xFF;

/// The messages the executable draws itself; RE2's own are not read yet, so
/// the pack's language file spells them (`MESSAGE_<nn>`).
pub const MESSAGE_PICKED_UP: u8 = 0;
pub const MESSAGE_LOCKED: u8 = 1;
pub const MESSAGE_LOCKED_OTHER_SIDE: u8 = 2;
pub const MESSAGE_UNLOCKED: u8 = 3;
pub const MESSAGE_USED_KEY: u8 = 4;
pub const MESSAGE_FULL: u8 = 5;

/// The script variables the engine owns (`scd_var_*`).
pub const VAR_FORWARD_AREA: usize = 0;
pub const VAR_BODY_AREA: usize = 1;
const VAR_STAGE: u8 = 24;
const VAR_ROOM: u8 = 25;
const VAR_CUT: u8 = 26;

/// RE2's `Sce_at_jump_tbl` indices (`SceKind`).
pub const SCE_AUTO: u8 = 0;
const SCE_DOOR: u8 = 1;
const SCE_ITEM: u8 = 2;
const SCE_MESSAGE: u8 = 4;
const SCE_EVENT: u8 = 5;
const SCE_SAVE: u8 = 9;
const SCE_ITEMBOX: u8 = 10;
pub const SCE_DAMAGE: u8 = 11;

impl Default for Re2State {
    fn default() -> Self {
        Self {
            room_enemy: 0,
            banks: {
                let mut banks = [[0; 8]; BANKS];
                banks[MASK_BANK..MASK_BANK + MASK_BANKS].fill([u32::MAX; 8]);
                banks
            },
            vars: [0; 256],
            gates: [ActionGate::default(); ROOM_ACTION_SLOTS],
            door_locks: [0; 2],
            random: 0x1382_01C3,
            cut_old: 0,
            voice: VoiceLine::default(),
            pending_events: Vec::new(),
            pending_damage: Vec::new(),
            pending_cuts: Vec::new(),
            pending_sca: Vec::new(),
            door_settling: false,
            door_stop: false,
            work: [Work::Player; TASK_SLOTS],
            speeds: [[0; 12]; TASK_SLOTS],
            pending_player: PendingPlayer::default(),
            key: 0,
            key_trg: 0,
            key_was_stopped: false,
            characters: Characters::default(),
            unmodelled_arrivals: Vec::new(),
            room_loading: false,
            player: crate::player2::Work::default(),
            vm: None,
            scenario_b: false,
            bgm: Default::default(),
            main_music: None,
            music: Default::default(),
            effects: Default::default(),
            sounds: Vec::new(),
            scroll: 0,
            rnd: Default::default(),
            tongue_owner: 0,
            enemies: 0,
            sound_types: [0; 2],
            enemy_bank: None,
            load_pass: false,
            pending_esprs: Vec::new(),
            screen_fades: Vec::new(),
            fades: [crate::transition::Fade::inactive(); 3],
            fades_drawn: [None; 3],
        }
    }
}

impl Re2State {
    /// The state a save keeps: the flags, variables, door locks and music
    /// table. The rest belongs to the room, and the room's init rebuilds it.
    pub fn to_save_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SAVE_LEN);
        for word in self.banks.iter().flatten().chain(&self.door_locks) {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for var in &self.vars {
            out.extend_from_slice(&var.to_le_bytes());
        }
        for room in self.bgm.words() {
            out.extend_from_slice(&room.to_le_bytes());
        }
        out.extend_from_slice(&self.random.to_le_bytes());
        out.push(u8::from(self.scenario_b));
        out
    }

    /// `Sce_rnd_set`. Extreme Battle's `Rnd` branch isn't ported.
    pub fn sce_rnd_set(&mut self) {
        let base = self.random;
        let twice = base.wrapping_mul(2);
        let sum = ((base & 0x7FFF_FFFF) >> 15).wrapping_add(base);
        self.random = ((sum ^ twice) & 0xFFFF) ^ twice;
        self.vars[usize::from(crate::scd2::vm::RANDOM_VAR)] = self.random as u16;
    }

    /// Restore [`Self::to_save_bytes`]; `None` for bytes of another length.
    pub fn load_save_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        if bytes.len() != SAVE_LEN {
            return None;
        }
        let mut words = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|raw| u32::from_le_bytes(*raw));
        for word in self.banks.iter_mut().flatten().chain(&mut self.door_locks) {
            *word = words.next()?;
        }
        let rest = &bytes[(BANKS * 8 + 2) * 4..];
        let (vars, rest) = rest.split_at(512);
        for (var, raw) in self.vars.iter_mut().zip(vars.as_chunks::<2>().0) {
            *var = u16::from_le_bytes(*raw);
        }
        let (rooms, rest) = rest.split_at(crate::bgm2::ROOMS * 2);
        for (room, raw) in self
            .bgm
            .words_mut()
            .iter_mut()
            .zip(rooms.as_chunks::<2>().0)
        {
            *room = u16::from_le_bytes(*raw);
        }
        self.random = u32::from_le_bytes(rest[..4].try_into().ok()?);
        self.scenario_b = rest[4] != 0;
        Some(())
    }

    /// Whether a script holds the player's input.
    pub fn input_stopped(&self) -> bool {
        self.flag(STOP_BANK, STOP_INPUT)
    }

    /// The `em` and `es` trace fields: a character's position, facing and
    /// motion.
    /// The zombies in play: the oracle's `zm` field.
    pub fn zombie_trace(&self) -> String {
        let rows: Vec<String> = self
            .characters
            .0
            .iter()
            .enumerate()
            .filter_map(|(slot, character)| {
                let character = character.as_deref()?;
                let zombie = character.zombie.as_ref()?;
                if character.be_flg & 1 == 0 {
                    return None;
                }
                let (routine, motion) = character.work.trace_fields();
                let [x, y, z] = character.body.pos;
                Some(format!(
                    "{slot}:{},{x},{y},{z},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    character.id,
                    character.body.angle & 0x0FFF,
                    routine[0],
                    routine[1],
                    motion[0],
                    motion[1],
                    motion[2],
                    motion[3],
                    motion[4],
                    character.work.kind,
                    zombie.life,
                    motion[6],
                    motion[7],
                    motion[8],
                ))
            })
            .collect();
        if rows.is_empty() {
            "-".to_string()
        } else {
            rows.join(";")
        }
    }

    pub fn character_trace(&self, slot: usize) -> Option<[i32; 16]> {
        let character = self.characters.0.get(slot)?.as_deref()?;
        if character.zombie.is_some() {
            return None;
        }
        let (routine, motion) = character.work.trace_fields();
        let [x, y, z] = character.body.pos;
        Some([
            i32::from(character.id),
            x,
            y,
            z,
            i32::from(character.body.angle & 0x0FFF),
            i32::from(routine[0]),
            i32::from(routine[1]),
            motion[0],
            motion[1],
            motion[2],
            motion[3],
            motion[4],
            motion[5],
            motion[6],
            motion[7],
            motion[8],
        ])
    }

    /// `fg_stop`'s second bit keeps `splayer_work` from running, and its third
    /// the enemy slots.
    pub fn character_stopped(&self, slot: usize) -> bool {
        self.flag(STOP_BANK, if slot == SPLAYER_SLOT { 1 } else { 2 })
    }

    pub fn enemies_stopped(&self) -> bool {
        self.flag(STOP_BANK, 2)
    }

    pub fn set_enemies_stopped(&mut self, stopped: bool) {
        self.set_flag(STOP_BANK, 2, u8::from(stopped));
    }

    /// `fg_stop`'s top bit, which keeps `Player_move` from running.
    /// The end of `Pad_set`: a stop leaves only some of the keys, and the
    /// tick it ends on raises none of them.
    pub fn set_key(&mut self, word: u16) {
        let stopped = self.input_stopped();
        let key = if stopped {
            word & crate::player2::KEYS_IN_A_STOP
        } else {
            word
        };
        let previous = if self.key_was_stopped && !stopped {
            key
        } else {
            self.key
        };
        self.key_trg = (previous ^ key) & key;
        self.key = key;
        self.key_was_stopped = stopped;
    }

    pub fn player_stopped(&self) -> bool {
        self.flag(STOP_BANK, 0)
    }

    /// `fg_stop`, for `Die`.
    pub(crate) fn fg_stop_mut(&mut self) -> &mut u32 {
        &mut self.banks[usize::from(STOP_BANK)][0]
    }

    /// `Sce_work_clr`'s `fg_tick`, which is the room bank's second word: the
    /// flags a walk's end raises for one pass of the scripts.
    /// `Sce_se_set`: choose the room's enemy bank from its enemies' sound types,
    /// and mark who plays the bank's second half.
    pub fn sce_se_set(&mut self) {
        let chosen = crate::se2::enemy_bank(self.sound_types);
        self.enemy_bank = chosen.map(|(bank, _)| bank);
        let Some((_, swapped)) = chosen else {
            return;
        };
        for character in self.characters.0.iter_mut().flatten() {
            if character.be_flg & 1 != 0
                && crate::se2::second_half(character.sound_type, self.sound_types, swapped)
            {
                character.be_flg |= 0x2000;
            }
        }
    }

    pub fn clear_tick_flags(&mut self) {
        if let Some(words) = self.banks.get_mut(usize::from(ROOM_BANK)) {
            words[1] = 0;
        }
        for flag in std::mem::take(&mut self.unmodelled_arrivals) {
            self.raise_room_flag(flag);
        }
    }

    /// `Flg_on(&fg_room, ...)`, a finished scripted walk's.
    pub fn raise_room_flag(&mut self, flag: u8) {
        self.set_flag(ROOM_BANK, flag, 1);
    }

    /// `pTbefore_func`, which `Game_loop` calls just before each `Joint_trans2`.
    pub fn tick_before(&mut self) {
        for character in self.characters.0.iter_mut().flatten() {
            if character.be_flg & 1 == 0 {
                continue;
            }
            let Some(zombie) = character.zombie.as_mut() else {
                continue;
            };
            if let Some(licker) = zombie.licker.as_mut() {
                crate::zombie2::licker::before(
                    licker,
                    zombie.life,
                    &mut character.be_flg,
                    &mut self.rnd,
                    &mut self.effects,
                );
            }
        }
    }

    /// `Esp_move`: its draws come from the game's stream, and its sounds join the rest.
    pub fn tick_effects(&mut self, sca: Option<&crate::state::Sca>) {
        // `Joint_trans2` comes just before `Esp_move`, and makes `Parts_down`'s calls.
        for character in self.characters.0.iter_mut().flatten() {
            let Some(zombie) = character.zombie.as_mut() else {
                continue;
            };
            let (rnd, effects) = (&mut self.rnd, &mut self.effects);
            zombie.thrown.retain_mut(|thrown| {
                let falling = thrown.fall(rnd, effects, sca);
                if !falling {
                    // Its children go with it, up to the arm's last.
                    for part in thrown.part..=thrown.part / 3 * 3 + 2 {
                        crate::player2::set_part_flags(&mut character.work, part, 0);
                    }
                }
                falling
            });
        }
        self.effects.tick(&mut self.rnd, sca);
        for (id, pos) in std::mem::take(&mut self.effects.sounds) {
            if let Some(id) = crate::se2::SoundId::decode(id) {
                let pos = pos.map(i32::from);
                self.sounds.push(SoundRequest { id, pos });
            }
        }
    }

    /// The bank `Em_dead_flg_on` raises a dead enemy's flag in, the one
    /// `sce_em_set` checks: by the stage and `fg_status`.
    pub fn enemy_flag_bank(&self, stage: u8) -> u8 {
        if stage.saturating_sub(1) < CAMPAIGN_ENEMY_STAGES
            && !self.flag(STATUS_BANK, STATUS_ENEMY_FLAGS)
        {
            ENEMY_BANK
        } else {
            ENEMY_BANK + 1
        }
    }

    /// `player_work`'s table of necks, by `plc_neck`'s number.
    /// Who `Pl_neck` can look at: the second player, then the enemy slots.
    pub fn neck_targets(&self) -> Vec<crate::player2::NeckTarget> {
        // `fg_status`'s `0x10000000`.
        if self.enemies == 0 && !self.flag(1, 3) {
            return Vec::new();
        }
        let slots = std::iter::once(SPLAYER_SLOT).chain(0..SPLAYER_SLOT);
        slots
            .filter_map(|slot| {
                let character = self.characters.0.get(slot)?.as_deref()?;
                (character.be_flg & 1 != 0).then(|| crate::player2::NeckTarget {
                    index: if slot == SPLAYER_SLOT { 1 } else { slot + 2 },
                    // A zombie's changes as it falls.
                    kind: if character.zombie.is_some() {
                        character.work.kind
                    } else {
                        character.kind
                    },
                    pos: character.body.pos,
                    neck: character.work.neck.position.unwrap_or(character.body.pos),
                    distance: character.distance,
                })
            })
            .collect()
    }

    pub fn necks(&self) -> crate::player2::Necks {
        let character = |slot: usize| {
            let character = self.characters.0.get(slot)?.as_deref()?;
            character.work.neck.position
        };
        std::array::from_fn(|index| match index {
            0 => self.player.neck.position,
            1 => character(SPLAYER_SLOT),
            n => character(n - 2),
        })
    }

    /// `Sce_work_clr_set`: every room starts with all of its mask groups on.
    pub fn reset_mask_groups(&mut self) {
        self.banks[MASK_BANK..MASK_BANK + MASK_BANKS].fill([u32::MAX; 8]);
    }

    /// `Psp_trans`'s `Flg_ck`: which of a camera's mask groups a script has
    /// left on, as bits for [`crate::mask::group_active`].
    pub fn mask_groups(&self, cut: usize) -> u32 {
        if cut >= MASK_BANKS {
            return u32::MAX;
        }
        (1..=32u8).fold(0, |bits, group| {
            if self.flag((MASK_BANK + cut) as u8, group) {
                bits | 1 << (group - 1)
            } else {
                bits
            }
        })
    }

    pub fn flag(&self, bank: u8, index: u8) -> bool {
        let Some(words) = self.banks.get(usize::from(bank)) else {
            return false;
        };
        words[usize::from(index >> 5) & 7] & (0x8000_0000 >> (index & 31)) != 0
    }

    pub fn set_flag(&mut self, bank: u8, index: u8, op: u8) {
        let Some(words) = self.banks.get_mut(usize::from(bank)) else {
            return;
        };
        let word = &mut words[usize::from(index >> 5) & 7];
        let bit = 0x8000_0000 >> (index & 31);
        match op {
            0 => *word &= !bit,
            1 => *word |= bit,
            7 => *word ^= bit,
            _ => {}
        }
    }
}

/// The gate on one interaction area, on top of its [`RoomAction`] box.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActionGate {
    /// The four corners of a `*_4p` area, which replace the box.
    pub quad: Option<[[i16; 2]; 4]>,
    /// The floor the area is on; `None` for every floor.
    pub floor: Option<u8>,
}

impl ActionGate {
    /// Whether `(x, z)` on `floor` is in the area whose box is `zone`, with
    /// RE2's signed coordinates (`Sce_at_check`).
    pub fn contains(&self, zone: [i16; 4], x: i32, z: i32, floor: u8) -> bool {
        if self.floor.is_some_and(|gate| gate != floor) {
            return false;
        }
        match self.quad {
            Some(corners) => Zone {
                corners,
                ..Zone::default()
            }
            .contains_signed(x, z),
            None => {
                let [left, near, width, depth] = zone.map(i32::from);
                let (width, depth) = (width & 0xFFFF, depth & 0xFFFF);
                (0..width).contains(&(x - left)) && (0..depth).contains(&(z - near))
            }
        }
    }
}

/// The RE2 half of a door record.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Re2Door {
    /// The destination's RoomId stage, already 1-based.
    pub stage: u8,
    pub room: u8,
    /// The camera the player arrives under.
    pub cut: u8,
}

/// The probe flags an RE2 `sat` byte means to [`GameState`]'s area scan: its
/// bit `0x10` is the action key, `0x20` probes the reach point, and the low
/// nibble selects who can trigger it.
fn probe_flags(sat: u8) -> u8 {
    let mut flags = sat & 0x0F;
    if sat & 0x10 != 0 {
        flags = 0x81;
    }
    if sat & 0x20 == 0 {
        flags |= 0x40;
    }
    flags
}

pub use crate::esp2::EsprCall;

/// `Get_matrix`: whose, and which of its parts, or the work's own `M`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EsprMatrix {
    /// `None` for the player, else the slot in `Characters`.
    pub slot: Option<usize>,
    pub part: Option<usize>,
}

impl EsprMatrix {
    /// `None` for `GsIdMatrix`, and for an object's, which still spawns at the
    /// origin.
    fn of(matrix: i8, id: u8) -> Option<Self> {
        let slot = |who| match who {
            0 => Some(None),
            1 => Some(Some(SPLAYER_SLOT)),
            2 => Some(Some(usize::from(id))),
            _ => None,
        };
        if matrix < 0 {
            let part = Some((matrix & 0x1F) as usize);
            return Some(Self {
                slot: slot((matrix >> 5) & 3)?,
                part,
            });
        }
        match matrix {
            1..=3 => Some(Self {
                slot: slot(matrix - 1)?,
                part: None,
            }),
            _ => None,
        }
    }
}

/// `work_set`'s kind byte for the player (`WK_PLAYER`).
const WK_PLAYER: i64 = 1;
const WK_SPLAYER: i64 = 2;
const WK_ENEMY: i64 = 3;
const WK_OBJECT: i64 = 4;

/// The work members holding the position (`Member_set_branch`).
const MEMBER_POS_X: usize = 0x0B;
/// The member that reads and writes `cdir.y`.
const MEMBER_ANGLE: u8 = 0x0F;
/// The word at `+0x21a`, which on a zombie is [`crate::zombie2::Zombie::flags`].
const MEMBER_ZOMBIE_FLAGS: u8 = 0x2A;
/// The room's flag bank, which `plc_dest` raises its arrival flag in.
const ROOM_BANK: u8 = 5;

/// `Member_cmp`'s comparison.
fn compare_member(op: u8, value: i32, operand: i32) -> bool {
    match op {
        0 => value == operand,
        1 => operand < value,
        2 => operand <= value,
        3 => value < operand,
        4 => value <= operand,
        5 => value != operand,
        6 => value & operand != 0,
        _ => false,
    }
}

/// An area operand block's box and, for a `*_4p` area, its corners.
fn area_shape(ops: &[i64], four_points: bool) -> ([i16; 4], Option<[[i16; 2]; 4]>) {
    let at = |index: usize| ops.get(index).copied().unwrap_or(0);
    if four_points {
        let corner = |index: usize| [at(5 + index * 2) as i16, at(6 + index * 2) as i16];
        ([0; 4], Some([corner(0), corner(1), corner(2), corner(3)]))
    } else {
        (
            [at(5) as i16, at(6) as i16, at(7) as i16, at(8) as i16],
            None,
        )
    }
}

/// An interaction area's table entry. Every kind but `SCE_AUTO` records the
/// probe's hit in the script variables, so the others stay live even with
/// nothing to do.
fn area_action(slot: u8, sce: u8, sat: u8, zone: [i16; 4], params: [u8; 8]) -> RoomAction {
    let kind = match sce {
        SCE_DOOR => RoomActionKind::Door,
        SCE_ITEM => RoomActionKind::Item,
        SCE_MESSAGE => RoomActionKind::Message,
        SCE_EVENT => RoomActionKind::Event,
        SCE_SAVE => RoomActionKind::Typewriter,
        SCE_ITEMBOX => RoomActionKind::ItemBox,
        _ => RoomActionKind::Other,
    };
    RoomAction {
        slot,
        kind,
        zone,
        sce,
        handler: u8::from(sce != SCE_AUTO),
        flags: probe_flags(sat),
        params,
        item_data: None,
        room_items_flag: 0xFF,
        reach_animation: false,
    }
}

/// Note a script instruction with no game side yet, once per run.
fn log_unmodelled(mnemonic: &'static str) {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    static SEEN: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    if SEEN.lock().is_ok_and(|mut seen| seen.insert(mnemonic)) {
        crate::logging::write(&format!("RE2 script: {mnemonic} is not modelled"));
    }
}

/// The script host over the game state.
pub struct GameHost<'a> {
    pub game: &'a mut GameState,
}

impl GameHost<'_> {
    /// `aot_set` and `aot_set_4p`: an interaction area with no door record.
    fn set_area(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let rest = if four_points { 13 } else { 9 };
        let (zone, quad) = area_shape(ops, four_points);
        let data: [u8; 6] = std::array::from_fn(|index| at(rest + index) as u8);
        let mut params = [0; 8];
        params[..6].copy_from_slice(&data);
        self.write_area(
            at(0) as usize,
            at(1) as u8,
            at(2) as u8,
            at(3) as u8,
            zone,
            quad,
            params,
        );
    }

    /// `item_aot_set` and `item_aot_set_4p`. A collected item's area is inert
    /// (`Sce` zero), as its collected flag says.
    fn set_item_area(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let rest = if four_points { 13 } else { 9 };
        let (zone, quad) = area_shape(ops, four_points);
        let (item, amount, flag, model, action) = (
            at(rest) as u8,
            at(rest + 1) as u8,
            at(rest + 2) as u8,
            at(rest + 3) as u8,
            at(rest + 4) as u8,
        );
        let sce = if self.game.re2.flag(ITEM_BANK, flag) {
            SCE_AUTO
        } else {
            at(1) as u8
        };
        let params = [item, amount, flag, model, action, 0, 0, 0];
        self.write_area(
            at(0) as usize,
            sce,
            at(2) as u8,
            at(3) as u8,
            zone,
            quad,
            params,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_area(
        &mut self,
        slot: usize,
        sce: u8,
        sat: u8,
        floor: u8,
        zone: [i16; 4],
        quad: Option<[[i16; 2]; 4]>,
        params: [u8; 8],
    ) {
        if slot >= ROOM_ACTION_SLOTS {
            eprintln!("warning: RE2 area {slot} is past the action slots");
            return;
        }
        self.game.re2.gates[slot] = ActionGate {
            quad,
            floor: (floor < 0x80).then_some(floor),
        };
        self.game.doors[slot] = None;
        self.game.room_actions[slot] = Some(area_action(slot as u8, sce, sat, zone, params));
    }

    /// `Aot_reset`: an existing area's kind, trigger and data change.
    fn reset_area(&mut self, ops: &[i64]) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let slot = at(0) as usize;
        let Some(Some(old)) = self.game.room_actions.get(slot).copied() else {
            return;
        };
        let data: [u8; 6] = std::array::from_fn(|index| at(3 + index) as u8);
        let mut params = [0; 8];
        params[..6].copy_from_slice(&data);
        self.game.room_actions[slot] = Some(area_action(
            slot as u8,
            at(1) as u8,
            at(2) as u8,
            old.zone,
            params,
        ));
    }

    /// Queue a light's change, on `cut` or the camera in use (`scd_var_cut`).
    fn light_edit(&mut self, cut: Option<i64>, index: i64, change: LightChange) {
        let cut = cut.map_or(self.game.camera.current_cut as u8, |cut| cut as u8);
        let index = index as u8;
        self.game
            .re2
            .pending_cuts
            .push(CutEdit::Light { cut, index, change });
    }

    /// `cut_chg`: a scripted camera, which stays until a `cut_auto` or
    /// `cut_old`.
    fn change_cut(&mut self, cut: u8) {
        self.game.re2.cut_old = self.game.camera.current_cut as u8;
        self.game.camera.locked = true;
        self.game.set_camera_cut(usize::from(cut & 0x7F));
    }

    fn character_mut(&mut self, slot: u8) -> Option<&mut Character> {
        self.game
            .re2
            .characters
            .0
            .get_mut(usize::from(slot))?
            .as_deref_mut()
    }

    /// The character `task` is working on.
    fn task_character(&mut self, task: usize) -> Option<&mut Character> {
        match self.game.re2.work.get(task) {
            Some(&Work::Character(slot)) => self.character_mut(slot),
            _ => None,
        }
    }

    /// `Sce_em_set`, for the characters `_Spl` runs.
    fn sce_em_set(&mut self, at: impl Fn(usize) -> i64) {
        let id = at(2) as u8;
        let slot = match at(1) {
            SPLAYER_OPERAND => SPLAYER_SLOT,
            slot => slot as usize,
        };
        if slot >= CHARACTER_SLOTS {
            return;
        }
        // Someone already dead stays so.
        let dead = at(8) as u8;
        let bank = self.game.re2.enemy_flag_bank(self.game.id.stage);
        if dead != NO_ENEMY_FLAG && self.game.re2.flag(bank, dead) {
            return;
        }
        if slot != SPLAYER_SLOT {
            self.game.re2.enemies = self.game.re2.enemies.wrapping_add(1);
        }
        let sound_type = at(6) as u8;
        let types = &mut self.game.re2.sound_types;
        if types[0] == 0 {
            types[0] = sound_type;
        } else if types[0] != sound_type {
            types[1] = sound_type;
        }
        // A licker on the floor: one that starts on a wall or the ceiling isn't
        // ported.
        let type_word = (at(3) as u16) | (at(4) as u16) << 8;
        let licker = id == 0x22 && type_word & 0x4F0F == 0;
        let zombie = (0x10..=0x20).contains(&id) || licker;
        // Any other enemy takes the slot, and isn't modelled yet.
        if !zombie && !(0x40..0x5C).contains(&id) {
            if let Some(character) = self.game.re2.characters.0.get_mut(slot) {
                *character = None;
            }
            return;
        }
        let mut body = crate::player::spawn(self.game.id, &Default::default());
        body.pos = [at(9) as i32, at(10) as i32, at(11) as i32];
        body.angle = at(12) as u16;
        body.collision_mirror = [body.pos[0], body.pos[2]];
        body.radius = if zombie {
            0x1C2
        } else {
            i32::from(CHARACTER_SIZES[usize::from(id - 0x40)])
        };
        let mut work = crate::player2::Work::default();
        work.routine = [0; 4];
        let kind = (at(3) as u16) | (at(4) as u16) << 8;
        if zombie {
            work.kind = kind;
        }
        let characters = &mut self.game.re2.characters.0;
        characters.resize_with(CHARACTER_SLOTS, || None);
        characters[slot] = Some(Box::new(Character {
            id,
            be_flg: 1,
            work,
            body,
            kind,
            start: (at(13) as u16, at(14) as u16),
            sound_type,
            distance: 0,
            with_room: self.game.re2.room_loading,
            held_bank: None,
            kage: None,
            zombie: zombie.then(|| {
                let mut zombie = crate::zombie2::spawned(slot as u8, at(5) as u8, at(7) as u8);
                zombie.dead_flag = dead;
                if licker {
                    zombie.licker = Some(Box::default());
                }
                zombie
            }),
        }));
    }

    /// Set the player's position or facing for `pos_set` and `dir_set`.
    /// `Sce_se_on`: its position is its offset from the body it names.
    /// A script's write to a fade: fade 0 is the session's, for the door's own
    /// to share.
    fn fade_write(&mut self, fade: u8, write: FadeWrite) {
        match fade {
            0 => self.game.re2.screen_fades.push(write),
            _ => {
                if let Some(fade) = self.game.re2.fades.get_mut(usize::from(fade) - 1) {
                    write.apply(fade);
                }
            }
        }
    }

    fn se_on(&mut self, at: impl Fn(usize) -> i64) {
        let (slot, positional) = (at(1) as u16 & 0xff, at(1) as u16 >> 8);
        let work = at(2) as u16;
        let id = (at(0) as u32 & 0xff) << 24 | u32::from(slot) << 16 | u32::from(positional);
        let Some(id) = crate::se2::SoundId::decode(id) else {
            return;
        };
        let player = self.game.entities[0].pos;
        let character = |slot: usize| {
            self.game
                .re2
                .characters
                .0
                .get(slot)?
                .as_deref()
                .map(|character| character.body.pos)
        };
        let origin = match work & 0xff {
            0 => [0; 3],
            1 => player,
            2 => character(SPLAYER_SLOT).unwrap_or(player),
            3 => character(usize::from(work >> 8)).unwrap_or(player),
            // Objects have no position here yet.
            _ => player,
        };
        let offset = [at(3) as i32, at(4) as i32, at(5) as i32];
        self.game.re2.sounds.push(SoundRequest {
            id,
            pos: std::array::from_fn(|axis| origin[axis] + offset[axis]),
        });
    }

    fn work_is_player(&self, task: usize) -> bool {
        self.game.re2.work.get(task) == Some(&Work::Player)
    }

    /// `Load_member_branch` for what the engine models: the player.
    fn member(&mut self, task: usize, member: u8) -> Option<i32> {
        if let Some(character) = self.task_character(task) {
            return character_member(character, member);
        }
        if let Some(object) = self.task_object(task) {
            return object_member(object, member);
        }
        if !self.work_is_player(task) {
            return None;
        }
        if let Some(axis) = usize::from(member)
            .checked_sub(MEMBER_POS_X)
            .filter(|axis| *axis < 3)
        {
            return self.game.entities[0].pos.get(axis).copied();
        }
        match member {
            MEMBER_ANGLE => Some(i32::from(self.game.entities[0].angle & 0x0FFF)),
            _ => crate::player2::member(&self.game.re2.player, member),
        }
    }

    /// `Member_set_branch` on the player.
    fn set_player_member(&mut self, member: u8, value: i32) {
        if let Some(axis) = usize::from(member)
            .checked_sub(MEMBER_POS_X)
            .filter(|axis| *axis < 3)
        {
            let mut pos = self
                .game
                .re2
                .pending_player
                .pos
                .unwrap_or(self.game.entities[0].pos);
            pos[axis] = value;
            self.game.re2.pending_player.pos = Some(pos);
        } else if member == MEMBER_ANGLE {
            self.game.re2.pending_player.angle = Some(value as u16 & 0x0FFF);
        } else {
            crate::player2::set_member(&mut self.game.re2.player, member, value);
        }
    }

    /// `Member_set_branch` on whichever work `task` has, if it's modelled.
    fn set_member(&mut self, task: usize, member: u8, value: i32) {
        if let Some(character) = self.task_character(task) {
            set_character_member(character, member, value);
        } else if let Some(object) = self.task_object(task) {
            set_object_member(object, member, value);
        } else if self.work_is_player(task) {
            self.set_player_member(member, value);
        }
    }

    /// The object `task`'s work is, if it is one.
    fn task_object(&mut self, task: usize) -> Option<&mut crate::objects::ObjectRecord> {
        match self.game.re2.work.get(task) {
            Some(Work::Object(slot)) => self.game.objects.record_mut(usize::from(*slot)),
            _ => None,
        }
    }

    /// `Add_speed`: the task's speeds, onto its work's position and rotation.
    fn add_speed(&mut self, task: usize) {
        let Some(speeds) = self.game.re2.speeds.get(task).copied() else {
            return;
        };
        if let Some(object) = self.task_object(task) {
            for axis in 0..3 {
                object.pos[axis] = object.pos[axis].wrapping_add(i32::from(speeds[axis]));
                object.rotation[axis] = object.rotation[axis].wrapping_add(speeds[axis + 3]);
            }
        } else if let Some(character) = self.task_character(task) {
            for (pos, speed) in character.body.pos.iter_mut().zip(speeds) {
                *pos = pos.wrapping_add(i32::from(speed));
            }
            character.body.angle = character.body.angle.wrapping_add(speeds[4] as u16);
        } else if self.work_is_player(task) {
            let mut pos = self
                .game
                .re2
                .pending_player
                .pos
                .unwrap_or(self.game.entities[0].pos);
            for (pos, speed) in pos.iter_mut().zip(speeds) {
                *pos = pos.wrapping_add(i32::from(speed));
            }
            self.game.re2.pending_player.pos = Some(pos);
            if speeds[4] != 0 {
                let angle = self
                    .game
                    .re2
                    .pending_player
                    .angle
                    .unwrap_or(self.game.entities[0].angle);
                self.game.re2.pending_player.angle =
                    Some(angle.wrapping_add(speeds[4] as u16) & 0x0FFF);
            }
        }
    }

    /// `Obj_model_set`: a second call on a slot in use takes it away.
    fn obj_model_set(&mut self, at: impl Fn(usize) -> i64) {
        let slot = at(0) as usize;
        let stage_room = (self.game.id.stage, self.game.id.room);
        let attr = at(8) as u16;
        if attr & 2 != 0 {
            self.game
                .re2
                .pending_cuts
                .push(CutEdit::BlendObject { slot: slot as u8 });
        }
        let Some(record) = self.game.objects.record_mut(slot) else {
            return;
        };
        let mut be_flg = if record.be_flg == 0 {
            (at(7) as i16 as i32 | 1) as u32
        } else {
            0
        };
        if attr & 0x20 != 0 {
            be_flg &= !1;
        }
        let selector = (at(5) as u16 >> 8) as u8;
        let parent = match selector & 0xC0 {
            0x40 => 0xFE,
            0xC0 => selector & 0x3F,
            _ => 0xFF,
        };
        let mut y = at(10) as i32;
        // `Obj_model_set` and `Om_trans`' own room: a floor of its first four.
        if stage_room == (2, 3) && slot < 4 {
            y -= 0x28;
        }
        *record = crate::objects::ObjectRecord {
            flag: if be_flg & 1 != 0 {
                crate::objects::OBJECT_FLAG_ACTIVE
            } else {
                0
            },
            model: slot as u8,
            parent,
            pos: [at(9) as i32, y, at(11) as i32],
            rotation: [at(12) as i16, at(13) as i16, at(14) as i16],
            asset: Some(slot as u8),
            be_flg,
            attr,
            draw_lift: if at(1) == 2 { 900 } else { 0 },
            ..crate::objects::ObjectRecord::default()
        };
    }

    /// `door_aot_set` and `door_aot_set_4p`: the area and its door record.
    fn set_door(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let slot = at(0) as usize;
        if slot >= ROOM_ACTION_SLOTS {
            eprintln!("warning: RE2 door area {slot} is past the action slots");
            return;
        }
        let sat = at(2) as u8;
        let floor = at(3) as u8;
        let (zone, quad, rest) = if four_points {
            let corner = |index: usize| [at(5 + index * 2) as i16, at(6 + index * 2) as i16];
            (
                [0; 4],
                Some([corner(0), corner(1), corner(2), corner(3)]),
                13,
            )
        } else {
            (
                [at(5) as i16, at(6) as i16, at(7) as i16, at(8) as i16],
                None,
                9,
            )
        };
        let next = |index: usize| at(rest + index);
        let flags = probe_flags(sat);
        self.game.re2.gates[slot] = ActionGate {
            quad,
            floor: (floor < 0x80).then_some(floor),
        };
        self.game.room_actions[slot] = Some(RoomAction {
            slot: slot as u8,
            kind: RoomActionKind::Door,
            zone,
            sce: at(1) as u8,
            handler: 1,
            flags,
            params: [0; 8],
            item_data: None,
            room_items_flag: 0xFF,
            reach_animation: false,
        });
        self.game.doors[slot] = Some(Door {
            slot: slot as u8,
            zone,
            // The door type, which `Door_load` hands the DO2 script.
            direction: next(9) as u8,
            sfx: next(10) as u8,
            // The DO2 file.
            door_type: next(8) as u8,
            camera: next(6) as u8 & 0x3F,
            lock: next(11) as u8,
            next_room: next(5) as u8,
            next_pos: [next(0) as i32, next(1) as i32, next(2) as i32],
            next_angle: next(3) as i16,
            key: next(12) as u8,
            sub_type: flags,
            re2: Some(Re2Door {
                stage: next(4) as u8 + 1,
                room: next(5) as u8,
                cut: next(6) as u8,
            }),
        });
    }
}

impl Host for GameHost<'_> {
    fn flag(&self, bank: u8, index: u8) -> bool {
        if bank == MESSAGE_BANK
            && u32::from(self.game.message.menu_choice_id()) & (0x8000_0000 >> (index & 31)) != 0
        {
            return true;
        }
        self.game.re2.flag(bank, index)
    }

    fn set_flag(&mut self, bank: u8, index: u8, op: u8) {
        self.game.re2.set_flag(bank, index, op);
    }

    fn var(&self, index: u8) -> u16 {
        match index {
            VAR_CUT => self.game.camera.current_cut as u16,
            VAR_STAGE => u16::from(self.game.id.stage.saturating_sub(1)),
            VAR_ROOM => u16::from(self.game.id.room),
            _ => self.game.re2.vars[usize::from(index)],
        }
    }

    fn set_var(&mut self, index: u8, value: u16) {
        self.game.re2.vars[usize::from(index)] = value;
    }

    fn waiting(&self) -> bool {
        self.game.message.active || self.game.re2.flag(STATUS_BANK, STATUS_VOICE)
    }

    fn random(&mut self) -> u16 {
        self.game.re2.sce_rnd_set();
        self.game.re2.random as u16
    }

    fn exec(&mut self, task: usize, op: &'static Op, operands: &[i64]) -> Step {
        let at = |index: usize| operands.get(index).copied().unwrap_or(0);
        match op.mnemonic {
            "door_aot_set" => self.set_door(operands, false),
            "door_aot_set_4p" => self.set_door(operands, true),
            "aot_set" => self.set_area(operands, false),
            "aot_set_4p" => self.set_area(operands, true),
            "item_aot_set" => self.set_item_area(operands, false),
            "item_aot_set_4p" => self.set_item_area(operands, true),
            "aot_reset" => self.reset_area(operands),
            "aot_on" => {
                if let Some(Some(action)) = self.game.room_actions.get(at(0) as usize).copied() {
                    self.game.fire_re2_area(action);
                }
            }
            "message_on" => {
                self.game.show_re2_message(at(1) as u8);
            }
            "cut_chg" => self.change_cut(at(0) as u8),
            "cut_old" => {
                let cut = self.game.re2.cut_old;
                self.game.camera.locked = false;
                self.game.set_camera_cut(usize::from(cut));
            }
            "cut_auto" => self.game.camera.locked = at(0) == 0,
            "sce_scr_move" => self.game.re2.scroll = at(1) as i16,
            // `Weapon_chg`: `Pl_weapon_ch` follows when the tick next looks.
            "weapon_chg" => self
                .game
                .set_equipped(Some(at(0) as u8).filter(|&id| id != 0)),
            "light_pos_set" => self.light_edit(None, at(1), light_pos(at(2), at(3))),
            "light_kido_set" => self.light_edit(None, at(0), LightChange::Radius(at(1) as u16)),
            "light_color_set" => self.light_edit(
                None,
                at(0),
                LightChange::Color(light_rgb(at(1), at(2), at(3))),
            ),
            "light_pos_set2" => self.light_edit(Some(at(0)), at(1), light_pos(at(2), at(3))),
            "light_kido_set2" => {
                self.light_edit(Some(at(1)), at(2), LightChange::Radius(at(3) as u16))
            }
            "light_color_set2" => self.light_edit(
                Some(at(0)),
                at(1),
                LightChange::Color(light_rgb(at(2), at(3), at(4))),
            ),
            "cut_be_set" => self.game.re2.pending_cuts.push(CutEdit::SetGate {
                cut: at(0) as u8,
                index: at(1) as u8,
                value: at(2) as u8,
            }),
            "cut_replace" => self.game.re2.pending_cuts.push(CutEdit::Replace {
                a: at(0) as u8,
                b: at(1) as u8,
            }),
            "sca_id_set" => self.game.re2.pending_sca.push((at(0) as u8, at(1) as u16)),
            "work_set" => {
                let work = if at(0) == WK_PLAYER {
                    Work::Player
                } else if at(0) == WK_SPLAYER && self.character_mut(SPLAYER_SLOT as u8).is_some() {
                    Work::Character(SPLAYER_SLOT as u8)
                } else if at(0) == WK_ENEMY && self.character_mut(at(1) as u8).is_some() {
                    Work::Character(at(1) as u8)
                } else if at(0) == WK_OBJECT && self.game.objects.record(at(1) as usize).is_some() {
                    Work::Object(at(1) as u8)
                } else {
                    Work::Other
                };
                if let Some(slot) = self.game.re2.work.get_mut(task) {
                    *slot = work;
                }
                if let Some(speeds) = self.game.re2.speeds.get_mut(task) {
                    *speeds = [0; 12];
                }
            }
            "sce_em_set" => self.sce_em_set(at),
            "obj_model_set" => self.obj_model_set(at),
            "speed_set" => {
                if let Some(speed) = self
                    .game
                    .re2
                    .speeds
                    .get_mut(task)
                    .and_then(|speeds| speeds.get_mut(at(0) as usize))
                {
                    *speed = at(1) as i16;
                }
            }
            "add_aspeed" => {
                if let Some(speeds) = self.game.re2.speeds.get_mut(task) {
                    for index in 0..6 {
                        speeds[index] = speeds[index].wrapping_add(speeds[index + 6]);
                    }
                }
            }
            "add_speed" => self.add_speed(task),
            // The game only ever gives this to enemies (kind 3).
            "kage_set" if at(0) == 3 => {
                if let Some(character) = self.character_mut(at(1) as u8) {
                    character.kage = Some(Kage {
                        color: at(2) as u32 | (at(3) as u32) << 8 | (at(4) as u32) << 16,
                        half: (at(5) as i32, at(6) as i32),
                        off: (at(7) as i32, at(8) as i32),
                    });
                }
            }
            "pos_set" if self.task_object(task).is_some() => {
                if let Some(object) = self.task_object(task) {
                    object.pos = [at(1) as i32, at(2) as i32, at(3) as i32];
                }
            }
            "dir_set" if self.task_object(task).is_some() => {
                if let Some(object) = self.task_object(task) {
                    object.rotation = [at(1) as i16, at(2) as i16, at(3) as i16];
                }
            }
            "xa_on" => {
                let stage = if at(0) == 0 { self.game.id.stage } else { 0 };
                let player = u8::from(self.game.re2.flag(STATUS_BANK, STATUS_PLAYER));
                self.game.re2.voice.request =
                    Some(format!("voice/{player}/{stage:02}_{:02x}.wav", at(1)));
                self.game.re2.set_flag(STATUS_BANK, STATUS_VOICE, 1);
            }
            "pos_set" if self.task_character(task).is_some() => {
                let pos = [at(1) as i32, at(2) as i32, at(3) as i32];
                if let Some(character) = self.task_character(task) {
                    character.body.pos = pos;
                }
            }
            "dir_set" if self.task_character(task).is_some() => {
                let angle = at(2) as u16;
                if let Some(character) = self.task_character(task) {
                    character.body.angle = angle;
                }
            }
            "plc_motion" if self.task_character(task).is_some() => {
                if let Some(character) = self.task_character(task) {
                    crate::player2::plc_motion(
                        &mut character.work,
                        at(0) as u8,
                        at(1) as u8,
                        at(2) as u8,
                    );
                }
            }
            "plc_dest" if self.task_character(task).is_some() => {
                if let Some(character) = self.task_character(task) {
                    crate::player2::plc_dest(
                        &mut character.work,
                        character.id,
                        at(1) as u8,
                        at(2) as u8,
                        at(3) as i16,
                        at(4) as i16,
                    );
                }
            }
            "plc_stop" if self.task_character(task).is_some() => {
                if let Some(character) = self.task_character(task) {
                    crate::player2::plc_stop(&mut character.work);
                }
            }
            "plc_cnt" if self.task_character(task).is_some() => {
                if let Some(character) = self.task_character(task) {
                    character.work.move_cnt = at(0) as u8;
                }
            }
            "pos_set" if self.work_is_player(task) => {
                self.game.re2.pending_player.pos = Some([at(1) as i32, at(2) as i32, at(3) as i32]);
            }
            "dir_set" if self.work_is_player(task) => {
                self.game.re2.pending_player.angle = Some(at(2) as u16);
            }
            // `Sce_espr_on`, on the world's matrix: the id's low byte is the
            // type and its high byte the row. `Sce_espr_on2` takes a group for
            // `Sce_espr_kill2` first, and the 3D ones a turn before the
            // direction.
            "sce_espr_on" | "sce_espr_on2" | "sce_espr3d_on" | "sce_espr3d_on2" => {
                let [kind, row] = (at(1) as u16).to_le_bytes();
                let is_3d = op.mnemonic.starts_with("sce_espr3d");
                let call = EsprCall {
                    kind,
                    row,
                    size: at(3) as u16,
                    dir: at(if is_3d { 10 } else { 7 }) as i16,
                    at: [at(4) as i16, at(5) as i16, at(6) as i16],
                    rotation: if is_3d {
                        [at(7) as i16, at(8) as i16, at(9) as i16]
                    } else {
                        [0; 3]
                    },
                    group: if op.mnemonic.ends_with('2') {
                        at(0) as u8
                    } else {
                        0
                    },
                };
                let [matrix, id] = (at(2) as u16).to_le_bytes();
                match EsprMatrix::of(matrix as i8, id) {
                    Some(matrix) => self.game.re2.pending_esprs.push((call, matrix)),
                    None => {
                        self.game.re2.effects.call_placed(call);
                    }
                }
            }
            "sce_espr_kill2" => self.game.re2.effects.kill_group(at(0) as u8),
            // `Sce_shake_on`: `SetGeomOffset`, which holds until the next one.
            "sce_shake_on" => {
                let shake = [-(at(0) as i8 as i32), -(at(1) as i8 as i32)];
                self.game.screen_shake = shake;
            }
            "sce_fade_set" => self.fade_write(
                at(0) as u8,
                FadeWrite::Set {
                    kind: at(1) as u8,
                    add: at(3) as i16,
                },
            ),
            "sce_fade_adjust" => self.fade_write(at(0) as u8, FadeWrite::Adjust(at(1) as i16)),
            // `Sce_parts_bomb`: the pieces `Parts_bomb` flings about aren't drawn.
            "sce_parts_bomb" => {
                let part = at(1) as u8;
                if let Some(character) = self.task_character(task) {
                    crate::player2::set_part_flags(&mut character.work, part, 0);
                }
            }
            "sce_parts_down" => {
                if let Some(zombie) = self
                    .task_character(task)
                    .and_then(|character| character.zombie.as_mut())
                {
                    zombie.script_drops.push(crate::zombie2::ScriptDrop {
                        part: at(0) as u8,
                        angle: at(1) as i16,
                        along: at(2) as i16,
                        up: at(3) as i16,
                        gravity: at(4) as u8,
                        rest: [at(6) as i16, at(7) as i16, at(8) as i16],
                    });
                }
            }
            "sce_espr_kill" => {
                self.game.re2.effects.kill(at(0) as u8, at(1) as u8);
            }
            "sce_bgm_control" => {
                let place = self.game.re2_music_place();
                self.game.re2.music.control(
                    [
                        at(0) as u8,
                        at(1) as u8,
                        at(2) as u8,
                        at(3) as u8,
                        at(4) as u8,
                    ],
                    place,
                );
            }
            "se_on" => self.se_on(at),
            "sce_bgmtbl_set" => {
                self.game
                    .re2
                    .bgm
                    .set(at(1) as u8, at(2) as u8, at(3) as u16, at(4) as u16);
            }
            "plc_motion" if self.work_is_player(task) => {
                crate::player2::plc_motion(
                    &mut self.game.re2.player,
                    at(0) as u8,
                    at(1) as u8,
                    at(2) as u8,
                );
            }
            "plc_dest" if self.work_is_player(task) => {
                let id = self.game.id.player_flag;
                crate::player2::plc_dest(
                    &mut self.game.re2.player,
                    id,
                    at(1) as u8,
                    at(2) as u8,
                    at(3) as i16,
                    at(4) as i16,
                );
            }
            // A character that isn't modelled is there as soon as it's told
            // to go: the destination's flag goes up for the next pass, whose
            // `edwhile` on it would miss one that this pass's end clears.
            "plc_dest" => {
                let flag = at(2) as u8;
                if flag != 0 {
                    self.game.re2.unmodelled_arrivals.push(flag);
                }
            }
            "parts_set" if at(2) == PARTS_SET_FLAGS => {
                let (part, flags) = (at(1) as u8, at(3) as u16);
                if let Some(character) = self.task_character(task) {
                    crate::player2::set_part_flags(&mut character.work, part, flags);
                } else if self.work_is_player(task) {
                    crate::player2::set_part_flags(&mut self.game.re2.player, part, flags);
                }
            }
            "plc_neck" if self.task_character(task).is_some() => {
                let words = [at(1) as i16, at(2) as i16, at(3) as i16];
                let rates = [at(4) as u8, at(5) as u8];
                if let Some(character) = self.task_character(task) {
                    crate::player2::plc_neck(&mut character.work, at(0) as u8, words, rates);
                }
            }
            "plc_neck" if self.work_is_player(task) => {
                crate::player2::plc_neck(
                    &mut self.game.re2.player,
                    at(0) as u8,
                    [at(1) as i16, at(2) as i16, at(3) as i16],
                    [at(4) as u8, at(5) as u8],
                );
            }
            "plc_ret" if self.work_is_player(task) => {
                crate::player2::plc_ret(&mut self.game.re2.player);
            }
            "plc_stop" if self.work_is_player(task) => {
                crate::player2::plc_stop(&mut self.game.re2.player);
            }
            "plc_rot" if self.task_character(task).is_some() => {
                let by = at(1) as i16;
                let which = at(0);
                if let Some(character) = self.task_character(task) {
                    match which {
                        0 => character.work.sce_free[0] = by,
                        1 => character.work.sce_free[1] = by,
                        _ => {}
                    }
                }
            }
            "plc_flg" if self.task_character(task).is_some() => {
                let value = at(1) as u16;
                let how = at(0);
                if let Some(character) = self.task_character(task) {
                    let work = &mut character.work;
                    match how {
                        0 => work.sce_flg |= value,
                        1 => work.sce_flg = value,
                        2 => work.sce_flg ^= value,
                        _ => {}
                    }
                }
            }
            "plc_ret" if self.task_character(task).is_some() => {
                if let Some(character) = self.task_character(task) {
                    crate::player2::plc_ret(&mut character.work);
                }
            }
            "plc_rot" if self.work_is_player(task) => {
                let by = at(1) as i16;
                match at(0) {
                    0 => self.game.re2.player.sce_free[0] = by,
                    1 => self.game.re2.player.sce_free[1] = by,
                    _ => {}
                }
            }
            "plc_flg" if self.work_is_player(task) => {
                let work = &mut self.game.re2.player;
                let value = at(1) as u16;
                match at(0) {
                    0 => work.sce_flg |= value,
                    1 => work.sce_flg = value,
                    2 => work.sce_flg ^= value,
                    _ => {}
                }
            }
            "plc_cnt" if self.work_is_player(task) => {
                self.game.re2.player.move_cnt = at(0) as u8;
            }
            // `Dir_ck`: a target within `range` of the way the work faces.
            "dir_ck" => {
                let member = |host: &mut Self, member| host.member(task, member);
                if let (Some(x), Some(z), Some(angle)) = (
                    member(self, MEMBER_POS_X as u8),
                    member(self, MEMBER_POS_X as u8 + 2),
                    member(self, MEMBER_ANGLE),
                ) {
                    let to = crate::player2::direction_ck(x, z, at(1) as i32, at(2) as i32);
                    let range = at(3) as i32;
                    let off = (i32::from(to) - angle + range) & 0xFFF;
                    if off >= range * 2 {
                        return Step::False;
                    }
                }
            }
            "member_copy" => {
                let value = self.member(task, at(1) as u8).unwrap_or(0);
                self.set_var(at(0) as u8, value as u16);
            }
            // A member with no state behind it ends the loops waiting on it, as
            // a cutscene's moving object has already arrived.
            "member_cmp" => {
                let Some(value) = self.member(task, at(1) as u8) else {
                    return Step::False;
                };
                if !compare_member(at(2) as u8, value, at(3) as i32) {
                    return Step::False;
                }
            }
            "member_set" => self.set_member(task, at(0) as u8, at(1) as i32),
            "member_set2" => {
                let value = i32::from(self.var(at(1) as u8));
                self.set_member(task, at(0) as u8, value);
            }
            "sce_key_ck" | "sce_trg_ck" => {
                let keys = if op.mnemonic == "sce_key_ck" {
                    self.game.re2.key
                } else {
                    self.game.re2.key_trg
                };
                if (keys & at(1) as u16 != 0) != (at(0) != 0) {
                    return Step::False;
                }
            }
            "keep_item_ck" => {
                if !self.game.has_item(at(0) as u8) {
                    return Step::False;
                }
            }
            "sce_item_get" => self.game.add_item(at(0) as u8, at(1) as u8),
            "sce_item_lost" => {
                self.game.remove_item(at(0) as u8);
            }
            _ => log_unmodelled(op.mnemonic),
        }
        Step::Next
    }
}

/// A `light_pos_set`'s axis is the member number: `0x0B` x, `0x0C` y, `0x0D` z.
fn light_pos(member: i64, value: i64) -> LightChange {
    LightChange::Pos {
        axis: (member as u8).wrapping_sub(0x0B),
        value: value as i16,
    }
}

fn light_rgb(red: i64, green: i64, blue: i64) -> [u8; 3] {
    [red as u8, green as u8, blue as u8]
}

/// `Load_member_branch` on a character.
/// `Load_member_branch` on an object: `Be_flg`, its position and its rotation.
fn object_member(object: &crate::objects::ObjectRecord, member: u8) -> Option<i32> {
    match member {
        0 => Some(i32::from(object.be_flg as u16)),
        1 => Some(i32::from((object.be_flg >> 16) as u16)),
        0x0B..=0x0D => Some(object.pos[usize::from(member) - 0x0B]),
        0x0E..=0x10 => Some(i32::from(object.rotation[usize::from(member) - 0x0E])),
        _ => None,
    }
}

/// `Member_set_branch` on an object. `Be_flg`'s bit 19 is `Om_trans`' not drawn.
fn set_object_member(object: &mut crate::objects::ObjectRecord, member: u8, value: i32) {
    match member {
        0 => object.be_flg = object.be_flg & 0xFFFF_0000 | u32::from(value as u16),
        1 => object.be_flg = object.be_flg & 0xFFFF | u32::from(value as u16) << 16,
        0x0B..=0x0D => object.pos[usize::from(member) - 0x0B] = value,
        0x0E..=0x10 => object.rotation[usize::from(member) - 0x0E] = value as i16,
        _ => return,
    }
    object.flag = if object.be_flg & 1 != 0 && object.be_flg & 0x8_0000 == 0 {
        crate::objects::OBJECT_FLAG_ACTIVE
    } else {
        0
    };
}

fn character_member(character: &Character, member: u8) -> Option<i32> {
    if let Some(axis) = usize::from(member)
        .checked_sub(MEMBER_POS_X)
        .filter(|axis| *axis < 3)
    {
        return Some(character.body.pos[axis]);
    }
    match member {
        0 => Some(character.be_flg as i32),
        MEMBER_ANGLE => Some(i32::from(character.body.angle & 0x0FFF)),
        MEMBER_ZOMBIE_FLAGS => character
            .zombie
            .as_ref()
            .map(|zombie| i32::from(zombie.flags)),
        _ => crate::player2::member(&character.work, member),
    }
}

/// `Member_set_branch` on a character.
fn set_character_member(character: &mut Character, member: u8, value: i32) {
    if let Some(axis) = usize::from(member)
        .checked_sub(MEMBER_POS_X)
        .filter(|axis| *axis < 3)
    {
        character.body.pos[axis] = value;
        return;
    }
    match member {
        0 => character.be_flg = value as u32,
        MEMBER_ANGLE => character.body.angle = value as u16,
        MEMBER_ZOMBIE_FLAGS => {
            if let Some(zombie) = character.zombie.as_mut() {
                zombie.flags = value as u16;
            }
        }
        _ => {
            crate::player2::set_member(&mut character.work, member, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(game: &mut GameState, mnemonic: &str, operands: &[i64]) {
        let op = crate::scd2::opcode::by_mnemonic(mnemonic).unwrap();
        GameHost { game }.exec(0, op, operands);
    }

    /// `sce_em_set` of id `id` in slot 0, and `work_set` on it.
    fn spawn_character(game: &mut GameState, id: i64, flag: i64) {
        exec(
            game,
            "sce_em_set",
            &[0, 0, id, 0, 0, 0, 0, 0, flag, -100, 0, -200, 3950, 266, 4],
        );
        exec(game, "work_set", &[WK_ENEMY, 0]);
    }

    #[test]
    fn obj_model_set_places_an_object_and_a_second_call_clears_it() {
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let call = [
            4, 2, 0, 0, 0, 0x4000, 0, 0x10, 0, 100, -200, 300, 1, 2, 3, 0, 0, 0, 0, 0, 0,
        ];
        exec(&mut game, "obj_model_set", &call);
        let object = game.objects.record_mut(4).unwrap();
        assert_eq!(object.be_flg, 0x11);
        assert_eq!(object.attr, 0);
        assert!(game.re2.pending_cuts.is_empty());
        assert_eq!(object.pos, [100, -200, 300]);
        assert_eq!(object.rotation, [1, 2, 3]);
        assert_eq!(object.parent, 0xFE, "0x40 follows the player");
        assert_eq!(object.draw_lift, 900, "Id 2 draws higher");
        assert_ne!(object.flag, 0);

        exec(&mut game, "obj_model_set", &call);
        let object = game.objects.record_mut(4).unwrap();
        assert_eq!(object.be_flg, 0);
        assert_eq!(object.flag, 0);
    }

    #[test]
    fn an_objects_attribute_asks_for_a_blended_model() {
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let call = [3, 0, 0, 0, 0, 0, 0, 0, 0x12, 0, 0, 0, 0, 0, 0];
        exec(&mut game, "obj_model_set", &call);
        assert_eq!(game.objects.record_mut(3).unwrap().attr, 0x12);
        assert_eq!(
            game.re2.pending_cuts,
            vec![CutEdit::BlendObject { slot: 3 }]
        );
    }

    #[test]
    fn a_script_moves_its_object_through_the_work() {
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let call = [4, 0, 0, 0, 0, 0, 0, 0, 0, 10, 20, 30, 0, 0, 0];
        exec(&mut game, "obj_model_set", &call);
        exec(&mut game, "work_set", &[WK_OBJECT, 4]);
        exec(&mut game, "pos_set", &[0, 5, 6, 7]);
        let object = game.objects.record_mut(4).unwrap();
        assert_eq!(object.pos, [5, 6, 7]);
        assert_eq!(object_member(object, 0x0C), Some(6));
        set_object_member(object, 0x0F, 99);
        assert_eq!(object.rotation[1], 99);
    }

    #[test]
    fn the_light_ops_name_the_camera_in_use_or_their_own() {
        let mut game = GameState::default();
        game.camera.current_cut = 3;
        exec(&mut game, "light_pos_set", &[0, 1, 0x0C, -900]);
        exec(&mut game, "light_kido_set", &[2, 20000]);
        exec(&mut game, "light_color_set", &[0, 10, 20, 30, 0]);
        exec(&mut game, "light_pos_set2", &[5, 1, 0x0D, 7]);
        exec(&mut game, "light_kido_set2", &[0, 5, 2, 3000]);
        exec(&mut game, "light_color_set2", &[5, 0, 1, 2, 3]);
        let light = |cut, index, change| CutEdit::Light { cut, index, change };
        assert_eq!(
            game.re2.pending_cuts,
            vec![
                light(
                    3,
                    1,
                    LightChange::Pos {
                        axis: 1,
                        value: -900
                    }
                ),
                light(3, 2, LightChange::Radius(20000)),
                light(3, 0, LightChange::Color([10, 20, 30])),
                light(5, 1, LightChange::Pos { axis: 2, value: 7 }),
                light(5, 2, LightChange::Radius(3000)),
                light(5, 0, LightChange::Color([1, 2, 3])),
            ]
        );
    }

    #[test]
    fn kage_set_makes_a_body_s_pool() {
        let mut game = GameState::default();
        spawn_character(&mut game, 31, 255);
        exec(
            &mut game,
            "kage_set",
            &[3, 0, 16, 191, 191, 2000, 1500, 7, 9],
        );
        let character = game.re2.characters.0[0].as_deref().unwrap();
        assert_eq!(
            character.kage,
            Some(Kage {
                color: crate::zombie2::POOL_COLOR,
                half: (2000, 1500),
                off: (7, 9),
            })
        );
    }

    #[test]
    fn a_script_s_character_takes_its_slot_and_its_commands() {
        let mut game = GameState::default();
        spawn_character(&mut game, 72, 255);
        let character = game.re2.characters.0[0].as_deref().unwrap();
        assert_eq!(character.id, 72);
        assert_eq!(character.be_flg, 1);
        assert_eq!(character.body.pos, [-100, 0, -200]);
        assert_eq!(character.body.angle, 3950);
        assert_eq!(character.body.radius, 450);
        assert_eq!(character.start, (266, 4));
        assert!(matches!(game.re2.work[0], Work::Character(0)));

        exec(&mut game, "plc_dest", &[0, 4, 33, -15372, -14079]);
        exec(&mut game, "pos_set", &[0, 5, 6, 7]);
        exec(&mut game, "member_set", &[0, 5]);
        let character = game.re2.characters.0[0].as_deref().unwrap();
        assert_eq!(character.work.routine[..2], [4, 4]);
        assert_eq!(character.work.sce_flg_no, 33);
        assert_eq!(character.work.dest, [-15372, -14079]);
        // `Plc_dest` takes the walk's turn from the character's table.
        assert_eq!(character.work.sce_free[0], 48);
        assert_eq!(character.body.pos, [5, 6, 7]);
        assert_eq!(character.be_flg, 5);
        assert!(!game.re2.flag(ROOM_BANK, 33));
    }

    #[test]
    fn the_scripts_random_steps_as_sce_rnd_set_does() {
        let mut state = Re2State::default();
        state.sce_rnd_set();
        let base = 0x1382_01C3u32;
        let expected = ((((base >> 15) + base) ^ (base * 2)) & 0xFFFF) ^ (base * 2);
        assert_eq!(state.random, expected);
        assert_eq!(state.vars[29], expected as u16);
    }

    #[test]
    fn get_matrix_reads_its_word() {
        let of = |word: u16| {
            let [matrix, id] = word.to_le_bytes();
            EsprMatrix::of(matrix as i8, id)
        };
        assert_eq!(of(0), None);
        assert_eq!(of(0x300), None);
        assert_eq!(of(0x304), None);
        let enemy = EsprMatrix {
            slot: Some(1),
            part: Some(11),
        };
        assert_eq!(of(459), Some(enemy));
        let splayer = EsprMatrix {
            slot: Some(SPLAYER_SLOT),
            part: Some(11),
        };
        assert_eq!(of(171), Some(splayer));
        let root = EsprMatrix {
            slot: Some(0),
            part: None,
        };
        assert_eq!(of(3), Some(root));
    }

    #[test]
    fn dir_ck_asks_whether_the_work_faces_the_point() {
        let mut game = GameState::default();
        spawn_character(&mut game, 72, 0);
        let op = crate::scd2::opcode::by_mnemonic("dir_ck").unwrap();
        let mut host = GameHost { game: &mut game };
        let angle = host.member(0, MEMBER_ANGLE).unwrap();
        let (x, z) = (
            host.member(0, MEMBER_POS_X as u8).unwrap(),
            host.member(0, MEMBER_POS_X as u8 + 2).unwrap(),
        );
        let ahead = (0..0x1000)
            .step_by(16)
            .map(|n| n as f64 * std::f64::consts::TAU / 4096.0)
            .map(|a| (x + (a.cos() * 3000.0) as i32, z + (a.sin() * 3000.0) as i32))
            .min_by_key(|(px, pz)| {
                let to = i32::from(crate::player2::direction_ck(x, z, *px, *pz));
                ((to - angle + 0x800) & 0xFFF).abs_diff(0x800)
            })
            .unwrap();
        let behind = (2 * x - ahead.0, 2 * z - ahead.1);
        let check = |host: &mut GameHost<'_>, point: (i32, i32)| {
            host.exec(0, op, &[0, i64::from(point.0), i64::from(point.1), 350])
        };
        assert!(check(&mut host, ahead) != Step::False);
        assert!(check(&mut host, behind) == Step::False);
    }

    #[test]
    fn a_voice_line_holds_wsleep_until_its_bit_clears() {
        let mut game = GameState::default();
        game.id.stage = 1;
        game.re2.set_flag(STATUS_BANK, STATUS_PLAYER, 1);
        exec(&mut game, "xa_on", &[0, 41]);
        assert_eq!(game.re2.voice.request.as_deref(), Some("voice/1/01_29.wav"));
        assert!(GameHost { game: &mut game }.waiting());
        game.re2.set_flag(STATUS_BANK, STATUS_VOICE, 0);
        assert!(!GameHost { game: &mut game }.waiting());
        exec(&mut game, "xa_on", &[1, 6]);
        assert_eq!(game.re2.voice.request.as_deref(), Some("voice/1/00_06.wav"));
    }

    #[test]
    fn a_script_changes_the_players_weapon() {
        let mut game = GameState::default();
        exec(&mut game, "weapon_chg", &[7]);
        assert_eq!(game.equipped_weapon_id, 7);
        exec(&mut game, "weapon_chg", &[0]);
        assert_eq!(game.equipped_weapon_id, 0);
    }

    #[test]
    fn the_second_player_has_its_own_slot_and_stop_bit() {
        let mut game = GameState::default();
        exec(
            &mut game,
            "sce_em_set",
            &[
                0, 255, 69, 0, 72, 0, 0, 0, 255, -26700, 0, -22840, 2800, 0, 0,
            ],
        );
        let character = game.re2.characters.0[SPLAYER_SLOT].as_deref().unwrap();
        assert_eq!(character.id, 69);
        assert_eq!(character.kind, 0x4800);
        assert_eq!(character.body.radius, 250);
        exec(&mut game, "work_set", &[WK_SPLAYER, 0]);
        assert!(matches!(
            game.re2.work[0],
            Work::Character(slot) if usize::from(slot) == SPLAYER_SLOT
        ));

        game.re2.set_flag(STOP_BANK, 1, 1);
        assert!(game.re2.character_stopped(SPLAYER_SLOT));
        assert!(!game.re2.character_stopped(0));
        game.re2.set_flag(STOP_BANK, 1, 0);
        game.re2.set_flag(STOP_BANK, 2, 1);
        assert!(!game.re2.character_stopped(SPLAYER_SLOT));
        assert!(game.re2.character_stopped(0));
    }

    #[test]
    fn someone_dead_stays_dead_and_an_enemy_is_not_a_character() {
        let mut game = GameState::default();
        game.re2.set_flag(ENEMY_BANK, 28, 1);
        spawn_character(&mut game, 74, 28);
        assert!(game.re2.characters.0.is_empty());
        // A crow, which isn't modelled.
        spawn_character(&mut game, 0x21, 255);
        assert!(game.re2.characters.0.is_empty());
        assert!(matches!(game.re2.work[0], Work::Other));
        // One that isn't modelled is there as soon as it's told to go.
        exec(&mut game, "plc_dest", &[0, 4, 33, 0, 0]);
        assert!(!game.re2.flag(ROOM_BANK, 33));
        game.re2.clear_tick_flags();
        assert!(game.re2.flag(ROOM_BANK, 33));
    }

    #[test]
    fn a_walk_s_end_lasts_one_pass_of_the_scripts() {
        let mut state = Re2State::default();
        state.set_flag(ROOM_BANK, 0, 1);
        state.raise_room_flag(33);
        state.clear_tick_flags();
        assert!(!state.flag(ROOM_BANK, 33));
        assert!(state.flag(ROOM_BANK, 0));
    }

    #[test]
    fn a_flag_bit_counts_from_the_top_of_its_word() {
        let mut state = Re2State::default();
        state.set_flag(3, 33, 1);
        assert_eq!(state.banks[3][1], 0x4000_0000);
        assert!(state.flag(3, 33));
        state.set_flag(3, 33, 7);
        assert!(!state.flag(3, 33));
    }

    #[test]
    fn a_stop_leaves_some_keys_and_its_end_raises_none() {
        const CONFIRM: u16 = 0x1000;
        const FORWARD: u16 = 0x0001;
        let mut state = Re2State::default();
        state.set_key(CONFIRM | FORWARD);
        assert_eq!(state.key_trg, CONFIRM | FORWARD);
        state.set_key(CONFIRM | FORWARD);
        assert_eq!(state.key_trg, 0);
        state.set_key(0);

        state.set_flag(STOP_BANK, STOP_INPUT, 1);
        state.set_key(CONFIRM | FORWARD);
        assert_eq!(state.key, CONFIRM);
        assert_eq!(state.key_trg, CONFIRM);
        state.set_flag(STOP_BANK, STOP_INPUT, 0);
        state.set_key(CONFIRM | FORWARD);
        assert_eq!(state.key, CONFIRM | FORWARD);
        assert_eq!(state.key_trg, 0);
    }

    #[test]
    fn a_key_wait_answers_for_its_own_keys() {
        let mut game = GameState::default();
        game.re2.key = 0x20;
        game.re2.key_trg = 0x1000;
        let mut host = GameHost { game: &mut game };
        let key = crate::scd2::opcode::by_mnemonic("sce_key_ck").unwrap();
        let trg = crate::scd2::opcode::by_mnemonic("sce_trg_ck").unwrap();
        assert!(matches!(host.exec(0, key, &[1, 0x20]), Step::Next));
        assert!(matches!(host.exec(0, key, &[1, 0x40]), Step::False));
        assert!(matches!(host.exec(0, key, &[0, 0x40]), Step::Next));
        assert!(matches!(host.exec(0, trg, &[1, 0x20]), Step::False));
        assert!(matches!(host.exec(0, trg, &[0, 0x3000]), Step::False));
        assert!(matches!(host.exec(0, trg, &[1, 0x3000]), Step::Next));
    }

    #[test]
    fn a_question_s_answer_reads_as_the_message_flag() {
        let mut game = GameState::default();
        game.message.set_menu_choice_id(0x81);
        let host = GameHost { game: &mut game };
        assert!(host.flag(MESSAGE_BANK, 31));
        assert!(host.flag(MESSAGE_BANK, 24));
        assert!(!host.flag(MESSAGE_BANK, 30));
    }

    #[test]
    fn a_door_op_builds_its_area_and_record() {
        let mut game = GameState::default();
        let mut host = GameHost { game: &mut game };
        // Room 1000's first door: the action key at the reach point, to room
        // 1010's cut 0 through DO2 file 1.
        let ops = [
            0, 1, 49, 0, 0, -22155, -20156, 1770, 2280, -15332, 0, -11799, 1832, 0, 1, 0, 0, 1, 2,
            3, 4, 5, 6,
        ];
        host.exec(0, crate::scd2::opcode::lookup(0x3B).unwrap(), &ops);
        let action = game.room_actions[0].unwrap();
        assert_eq!(action.flags, 0x81);
        assert!(game.re2.gates[0].contains(action.zone, -21000, -19000, 0));
        assert!(!game.re2.gates[0].contains(action.zone, -21000, -19000, 1));
        assert!(!game.re2.gates[0].contains(action.zone, -23000, -19000, 0));
        let door = game.doors[0].unwrap();
        assert_eq!(door.next_pos, [-15332, 0, -11799]);
        assert_eq!(door.door_type, 1);
        assert_eq!(door.direction, 2);
        assert_eq!(door.lock, 4);
        assert_eq!(door.key, 5);
        assert_eq!(
            door.re2,
            Some(Re2Door {
                stage: 1,
                room: 1,
                cut: 0
            })
        );
    }

    #[test]
    fn the_saved_state_round_trips() {
        let mut state = Re2State::default();
        state.set_flag(8, 15, 1);
        state.vars[29] = 0x1234;
        state.door_locks[1] = 7;
        state.bgm.set(1, 1, 0x0102, 0);
        state.scenario_b = true;
        let bytes = state.to_save_bytes();
        let mut restored = Re2State::default();
        restored.load_save_bytes(&bytes).unwrap();
        assert_eq!(restored, state);
        assert!(restored.load_save_bytes(&bytes[1..]).is_none());
    }

    #[test]
    fn a_picked_up_item_stays_collected() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 15, 3, 1, 0];
        let mut host = GameHost { game: &mut game };
        let item = crate::scd2::opcode::lookup(0x4E).unwrap();
        host.exec(0, item, &ops);
        let action = game.room_actions[8].unwrap();
        assert_eq!(action.kind, RoomActionKind::Item);
        game.fire_re2_area(action);
        assert_eq!(game.item_count(0x14), 15);
        assert!(game.re2.flag(ITEM_BANK, 3));
        assert_eq!(game.room_actions[8].unwrap().handler, 0);
        // The room's next init sees the flag.
        let mut host = GameHost { game: &mut game };
        host.exec(0, item, &ops);
        assert_eq!(game.room_actions[8].unwrap().handler, 0);
    }

    #[test]
    fn a_locked_door_takes_its_key_then_opens() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let door = Door {
            lock: 0x85,
            key: 0x31,
            re2: Some(Re2Door::default()),
            ..Door::default()
        };
        assert!(!game.try_re2_door_for_test(&door));
        assert_eq!(game.re2.door_locks, [0, 0]);
        game.message.active = false;
        game.add_item(0x31, 1);
        assert!(!game.try_re2_door_for_test(&door));
        assert_ne!(game.re2.door_locks, [0, 0]);
    }

    #[test]
    fn a_member_with_no_state_ends_the_wait_on_it() {
        let mut game = GameState::default();
        let mut host = GameHost { game: &mut game };
        let work = crate::scd2::opcode::lookup(0x2E).unwrap();
        host.exec(0, work, &[4, 0]);
        let cmp = crate::scd2::opcode::lookup(0x3E).unwrap();
        assert_eq!(host.exec(0, cmp, &[0, 12, 3, -5600]), Step::False);
        host.exec(0, work, &[1, 0]);
        // The player's height is 0, not below -5600.
        assert_eq!(host.exec(0, cmp, &[0, 12, 3, -5600]), Step::False);
        assert_eq!(host.exec(0, cmp, &[0, 12, 4, 0]), Step::Next);
    }

    #[test]
    fn an_auto_area_probes_the_body_and_an_action_area_the_reach_point() {
        assert_eq!(probe_flags(0x31), 0x81);
        assert_eq!(probe_flags(0x11), 0xC1);
        assert_eq!(probe_flags(0x01), 0x41);
        assert_eq!(probe_flags(0x21), 0x01);
    }
}
