//! The game side of the RE2 script VM: the flag banks and script variables,
//! and the instructions that build the room's interaction areas.

use super::opcode::Op;
use super::vm::{Host, Step};
use crate::game::{Door, GameState, ROOM_ACTION_SLOTS, RoomAction, RoomActionKind};
use crate::state::Zone;

/// The flag banks `ck` and `set` index: every entry of `Flg_addr`
/// (`0x53B218`), 0 to 39. The original's banks overlap in memory (bank 9,
/// `fg_map`, is five words long and bank 35's second word is its first),
/// while each bank here is eight words of its own, so an index past a bank's
/// real size doesn't alias as the original's would. No script uses one.
pub const BANKS: usize = 40;
/// The banks a save from before the map's banks holds, 0 to 31; a save
/// keeps banks 32 to 39 after the rest of its fields.
const SAVE_BANKS: usize = 32;

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
    /// `--voice-ticks`: how long the oracle's recording held each voice line,
    /// in its order. With sound the original ends a line by DirectSound's
    /// clock, so its length in ticks follows the recording's pace.
    pub voice_ticks: std::collections::VecDeque<u32>,
    /// `xa_vol`'s level for the voice channel (`0x693E88`), 0 to 127.
    pub xa_volume: u8,
    /// The camera `cut_old` returns to.
    pub cut_old: u8,
    /// `scd_var_cut` as the tick began: the cut on screen, which
    /// `Swap_Cbuff` loaded at the previous frame's end. A cut `Cut_check`
    /// switched to this tick isn't on it yet.
    pub shown_cut: u8,
    /// `fg_status`'s `0x80`, which the aim routine (`pl_mv_05`) raises each
    /// frame and `Player_move` clears: while it is set at the frame's end,
    /// `Swap_Cbuff` loads no background, so a cut `Cut_check` switched to
    /// waits there and `scd_var_cut`, the cut on screen, stays.
    pub view_held: bool,
    /// `BYTE_0098f07b`, the pending `Bg_loadPC`: `Swap_Cbuff` loads the cut's
    /// view and background at the tick's end and skips two frames
    /// ([`crate::transition::PresentSkip`]). `cut_chg` (unless its bit 7 is
    /// set) and `cut_old` raise it even for the cut already on screen;
    /// `Swap_Cbuff` itself raises it for a zone's change.
    pub bg_load: bool,
    /// A `cut_chg` with bit 7 moved the cut this tick: `Cut_chg` sets
    /// `scd_var_cut` with the live cut and drops the pending load, so
    /// `Swap_Cbuff` sees no change to load.
    pub cut_no_load: bool,
    /// Interaction-area events waiting for the VM (`Sce_event`): task, event.
    pub pending_events: Vec<(u8, u8)>,
    /// `Sce_Damage` areas the player's position probe hit this tick: the box,
    /// the kind and the damage. The tick applies them to the player.
    pub pending_damage: Vec<([i16; 4], u8, i16)>,
    /// `fg_stop & 0x2000000` while `Sce_at` runs: the status screen's open
    /// stops the scheduler. Of the area handlers only `Sce_Event` tests it,
    /// so an event area starts nothing and every other kind acts as it
    /// would (`Sce_Door` has a guard of its own, `0x98A0D4`, which isn't
    /// modelled).
    pub areas_stopped: bool,
    /// `scd_var_cut`, the cut `Snd_se_3D` places every positional sound
    /// from: the tick's cut when it starts, the new room's entry cut at a
    /// load, and moved at once by a script's `cut_chg` or `cut_old`
    /// (`FUN_004e5020`). The player's cut check doesn't move it: `Swap_Cbuff`
    /// brings it up at the frame's end. `None` falls back to the tick's cut.
    pub var_cut: Option<u8>,
    /// Camera-table edits waiting for the room (`cut_be_set`, `cut_replace`).
    pub pending_cuts: Vec<CutEdit>,
    /// Collision-record edits waiting for the room (`sca_id_set`).
    pub pending_sca: Vec<(u8, u16)>,
    /// A door has just handed back, and `fg_stop` hasn't come down yet.
    pub door_settling: bool,
    /// The same tick, as the enemy slots see it.
    pub door_stop: bool,
    /// The tick after: `fg_stop` (`0xEF000000`) comes down only after
    /// `Pad_set` has masked the pad with it.
    pub door_key_stop: bool,
    /// The scripts opened a door this tick (`aot_on` on a door's area). The
    /// original's scripts run after `Player_move` and its cut check, and
    /// `Sce_Door` stops both from the next tick; the port runs the scripts
    /// first, so the player, the cut and the characters sit this tick out.
    pub script_door: bool,
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
    /// This frame's [`Self::set_key`] has run: the room tick's, or the one
    /// the session runs for a frame the status screen, the map, the box or
    /// another screen owns (`Psx_main` runs `Pad_set` on every frame).
    pub key_read: bool,
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
    /// `Item_work`'s eleventh slot (`0x98ED5C`): the lighter or the lockpick,
    /// `[id, count]`, which the eight slots and their count don't see.
    pub personal_item: [u8; 2],
    /// `Item_boxwork` (`0x98ED60`): the item box's 64 slots, `[id, count,
    /// mark]`, the mark 3 for a two-slot weapon.
    pub item_box: [[u8; 3]; crate::game::itembox2::BOX_SLOTS],
    /// `File_tag_bk` (`0x98EE60`): the files held, three binders of eight
    /// (byte `binder * 8 + slot`), [`NO_FILE`] for none.
    pub files: [u8; FILE_SLOTS],
    /// The title's `hard_mode` and `censorship_off` as the new game's setup
    /// keeps them in the saved work (`0x98EF2C`, `0x98EF2D`, `FUN_004b73c0`);
    /// a load puts them back (`Title_game_init`).
    pub hard_mode: bool,
    pub censorship_off: bool,
    /// The room music table (`bgm_tbl`).
    pub bgm: crate::bgm2::BgmTable,
    /// The main music playing.
    pub main_music: Option<u8>,
    /// The music and ambience's channels.
    pub music: crate::bgm2::Music,
    /// A held door's `Set_room` already ran `Snd_bgm_set` for the room it loads.
    pub music_set: bool,
    /// `Esp_`: the effect sprites.
    pub effects: crate::esp2::Effects,
    /// `se_on`'s sounds, not yet played.
    pub sounds: Vec<SoundRequest>,
    /// The effects' sounds, not yet played: `Esp_`'s run after
    /// `Player_move`, so they follow the player's footsteps and weapon.
    pub effect_sounds: Vec<SoundRequest>,
    /// `sce_scr_move`: how far the background has scrolled down.
    pub scroll: i16,
    /// `Rnd`, for what's ported to its stream. A save doesn't hold it: the
    /// room's load seeds it.
    pub rnd: crate::zombie2::Rnd,
    /// `DWORD_006894f8`: the `Work_no` of the licker whose tongue is out.
    pub tongue_owner: u8,
    /// `BYTE_00689610`: `Em_2D`'s wait before the next wake groan.
    pub em2d_groan: u8,
    /// The cockroaches' swarm (`Em_29`): `DWORD_00689590`, the ticks with
    /// more than two on the player, `0x689594`, how many are on the player,
    /// `0x689598`, the attach points taken, and `0x68959C`, the player was
    /// killed. Every roach's init zeroes them; a room's load doesn't.
    pub swarm: [u32; 4],
    /// `DWORD_009888d8`'s low byte: the draw buffer, which `Swap_Cbuff` flips
    /// once a frame.
    pub draw_buffer: u8,
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
    /// The films play: the original's `MovieEnable == 0`. The oracle runs with
    /// the films off (`MovieEnable = 0` in `config.ini`), so a trace does too.
    pub films: bool,
    /// The film `movie_on` asked for, for the engine to play (`Movie_r0 == 1`).
    pub film: Option<u8>,
    /// `Movie_set`'s last write this tick: the frames `WinMain` holds.
    pub movie_set: Option<u8>,
    /// `movie_on`'s `SsClose`: every sound stops. It holds how many of
    /// [`Self::sounds`] were asked for before it, which it stops too.
    pub sound_close: Option<usize>,
    /// `SsInit2`: the sound comes back, and the music that `SsClose` stopped
    /// starts again from its start.
    pub sound_init: bool,
    /// `Spl_pos_ck_timer` (`0x53cdef`): `Sherry_move`'s count to its enemy
    /// check. A static of the original that nothing resets, not a room, a new
    /// game nor a load: only a fresh process starts it at 0x0F.
    pub spl_pos_ck_timer: u8,
    /// The nearest-enemy cache's distances (`0x692fc8`): `Spl_enemy_ck`
    /// rewrites them only in a room with enemies, so a room without any keeps
    /// the last room's, or a fresh process's 0. Another static.
    pub nearest_dist: [u16; 3],
    /// `saved_splayer_health` (`0x98ee7c`): the partner's life, 200 at a new
    /// game.
    pub partner_life: i16,
    /// `BYTE_0098ee7b`'s bit 0: the partner is poisoned.
    pub partner_poisoned: bool,
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
    /// The cut `Snd_se_3D` placed it from (`scd_var_cut`), for a script's
    /// `se_on`, which a later `cut_chg` or `cut_old` in the same pass
    /// mustn't move. `None` is placed from the live cut when the tick's
    /// sounds play ([`Re2State::placement_cut`]): every other sound starts
    /// after the scripts, as in `Game_loop`.
    pub cut: Option<u8>,
}

impl SoundRequest {
    pub fn new(id: crate::se2::SoundId, pos: [i32; 3]) -> Self {
        Self { id, pos, cut: None }
    }
}

/// `pos_set` and `dir_set` on the player.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PendingPlayer {
    pub pos: Option<[i32; 3]>,
    pub angle: Option<u16>,
    /// A warp, which also moves `Old_x` and `Old_z`. A script's `pos_set`
    /// leaves them, so the next push starts from the last tick.
    pub warp: Option<[i32; 3]>,
}

/// An edit of the room's camera switch zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutEdit {
    /// `obj_model_set`'s attribute 2: the object's model blends.
    BlendObject { slot: u8 },
    /// `cut_be_set`: entry `index` of camera `cut`'s zones gets gate `value`.
    SetGate { cut: u8, index: u8, value: u8 },
    /// `cut_replace`: swap two camera ids across every zone's `cam_from` and
    /// `cam_to`.
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
    /// `Id`, which chooses what runs it. A script can change it (`member_set`).
    pub id: u8,
    /// The id the room loaded its model and animations for, which a change of
    /// `Id` keeps: Marvin turns into a zombie in his own body.
    pub model: u8,
    /// `sce_em_set`'s floor, model type and dead flag, which a zombie it
    /// becomes starts with.
    pub em_set: (u8, u8, u8),
    /// Marvin's two sets of arms (`Spl_in4a`, `Spl_chenge00`).
    pub arms: Arms,
    /// `Be_flg`, whose first bit is whether it's in play.
    pub be_flg: u32,
    pub work: crate::player2::Work,
    pub body: crate::player::PlayerState,
    /// `Type`, whose `0x400` gives it to `Spl_Auto` rather than the scripts.
    /// For the partner it is the live `Type`, whose low nibble `Sherry_move`
    /// writes; `work.kind` holds only what stops it being moved.
    pub kind: u16,
    /// `sce_em_set`'s last two words: `Em_init_move`'s second `_Spl`.
    pub start: (u16, u16),
    /// `+0x1fa`: its sound type, which chooses the room's enemy bank.
    pub sound_type: u8,
    /// The room's load places it, rather than a script during play.
    pub with_room: bool,
    /// `+0x1F0`: how far it was from the player when it last ran.
    pub distance: u32,
    /// `+0xB` (member 9): the area `Sce_at_check`'s enemy pass last found it
    /// in, [`NO_AREA`] once the scripts' pass after it has ended.
    pub area: u8,
    /// `pSub1_kan_t_ptr` once a zombie has hold of it: that zombie's own.
    pub held_bank: Option<std::sync::Arc<crate::game::PlayerDamageBank>>,
    /// What it needs if `Em_Zombie` runs it.
    pub zombie: Option<crate::zombie2::Zombie>,
    /// `splayer_work`'s own words, for `Spl_Root_ck_sys` and `Spl_Auto`:
    /// placed with any character in that slot.
    pub partner: Option<Box<crate::partner2::Partner>>,
    /// The last `kage_set`, in place of `Em_init`'s shadow.
    pub kage: Option<Kage>,
}

/// Which of Marvin's arms his joints draw, and the fade between them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Arms {
    /// The joints draw the spare meshes: his arms as a zombie's.
    pub swapped: bool,
    /// Ticks of `Spl_chenge00`'s fade so far: the first part's `Poly_rgb`
    /// climbs to `0x40` and back.
    pub fade: u8,
    /// The fade has run (`Timer0` is 2), and doesn't run again.
    pub faded: bool,
}

/// `xa_vol`'s default, which the room load and a new game set.
pub const XA_VOLUME: u8 = 120;

/// `Snd_xa_vol` (`0x434AB0`, channel 7): DirectSound's millibels for an
/// `xa_vol` level. The voice channel takes the level as it is, without the
/// master volume the other channels are scaled by.
pub fn xa_millibels(level: u8) -> i32 {
    let level = i32::from(level);
    let millibels = if level < 0x20 {
        level * 259 - 10_000
    } else {
        (level * 9 - 0x477) * 2
    };
    millibels.clamp(-10_000, 0)
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

/// `File_tag_bk`'s 24 bytes: three binders of eight files.
pub const FILE_SLOTS: usize = 24;
/// A slot of `File_tag_bk` holding no file.
pub const NO_FILE: u8 = 0xFF;

/// The bytes of [`Re2State::to_save_bytes`].
const SAVE_LEN: usize = SAVE_LEN_NO_PARTNER + 3;
/// A save from before the partner's life and poison.
const SAVE_LEN_NO_PARTNER: usize = SAVE_LEN_NO_FILES + FILE_SLOTS;
/// A save from before the files held.
const SAVE_LEN_NO_FILES: usize = SAVE_LEN_NO_MAP + (BANKS - SAVE_BANKS) * 8 * 4;
/// A save from before banks 32 to 39 (the map's, `fg_map_c`, `fg_map_i`,
/// `fg_item_2` and the floors').
const SAVE_LEN_NO_MAP: usize = SAVE_LEN_NO_CHOICE + 2;
/// A save from before the new-game choice's two bytes.
const SAVE_LEN_NO_CHOICE: usize = SAVE_LEN_NO_BOX + crate::game::itembox2::BOX_SLOTS * 3;
/// A save from before the item box's slots.
const SAVE_LEN_NO_BOX: usize = (SAVE_BANKS * 8 + 2) * 4 + 512 + crate::bgm2::ROOMS * 2 + 7;
/// A save from before the personal item's two bytes.
const SAVE_LEN_NO_PERSONAL: usize = SAVE_LEN_NO_BOX - 2;

/// `fg_system`'s `0x20`: EASY.
const FG_SYSTEM_EASY: u8 = 26;
/// `fg_system`'s `0x40`: `censorship_off`'s bit.
const FG_SYSTEM_CENSORSHIP_OFF: u8 = 25;
/// `fg_system`'s `0x80000`: EASY with `censorship_off`.
const FG_SYSTEM_EASY_CENSORSHIP_OFF: u8 = 12;
/// `fg_system`'s `0x200`: a film plays (`Init_movie_work` raises it, the
/// film's end clears it). `Psx_main`'s pause gate reads it.
pub const FG_SYSTEM_MOVIE: u8 = 22;
/// `fg_system`'s `0x8000`, which `Init_movie_work` clears. No reader is known.
pub const FG_SYSTEM_8000: u8 = 16;
/// `fg_status`'s `0x4000000`: EASY's.
const FG_STATUS_EASY: u8 = 5;

/// The script's task slots.
const TASK_SLOTS: usize = super::vm::TASKS;
/// `set 2, 7`: `FG_STOP_DISABLE_INPUT`, a cutscene holding the player's input.
const STOP_BANK: u8 = 2;
/// `fg_stop`'s `0x10000000`, which holds `Esp_move`.
const STOP_EFFECTS: u8 = 3;
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
/// `fg_status`'s bank, whose bit picks the second `fg_enemy`.
pub const STATUS_BANK: u8 = 1;
const STATUS_ENEMY_FLAGS: u8 = 6;
/// `fg_status`'s bit for the character whose voices play: 1 is Claire.
pub const STATUS_PLAYER: u8 = 0;
/// `fg_status`'s `0x10000000`: the partner is with the player.
const PARTNER_BIT: u8 = 3;
/// `fg_status`'s `0x2000`: `Om_move` pushed an object this tick.
const PUSH_BIT: u8 = 18;
/// `fg_stop`'s `0x8000000`: the objects don't move.
const OBJECTS_STOP_BIT: u8 = 4;
/// The partner's life and `Max_life`: `saved_splayer_health` at a new game,
/// and what `Spl_in45`, `Spcl_ret` and `Sce_heal_partner` give her.
pub const PARTNER_LIFE: i16 = 200;

/// `Plc_ret`'s `Type`: a player's high byte keeps only its low nibble, and
/// everyone loses `0x4000` and `0x1000` (`& 0xAF`), so a scene character goes
/// to `Spl_Auto` as a partner would.
fn plc_ret_kind(id: u8, kind: u16) -> u16 {
    let kind = if id < 0x10 { kind & 0x0FFF } else { kind };
    kind & 0xAFFF
}

/// `Splc_sce` (opcode 0x71, `0x4e8d00`) on the task's work, whose partner
/// bit the caller clears: back to the scripts (`Spl_Scenario`, standing),
/// looking ahead, a scene character's `Type` and no life.
fn splc_sce(character: &mut Character) {
    character.work.routine = [4, 6, 0, 0];
    character.work.neck.target = None;
    character.work.neck.flags = 0x92;
    character.kind = (character.kind & !0x400) | 0x5000;
    if let Some(partner) = character.partner.as_deref_mut() {
        partner.life = -1;
        partner.max_life = -1;
    }
}

/// `Spcl_ret` (opcode 0x70, arklay's `splc_ret`, `0x4e8c70`) on the task's
/// work, whose partner bit the caller raises: back to `Spl_Auto`, looking at
/// the player, the partner's `Type` and her saved life. Sherry is no longer
/// lost.
fn splc_ret(character: &mut Character, life: i16) {
    character.work.routine = [1, 0, 0, 0];
    character.work.neck.target = Some(0);
    character.work.neck.flags = 0;
    character.kind = (character.kind & 0xAFFF) | 0x400;
    let sherry = matches!(character.id, 0x45 | 0x4F);
    if let Some(partner) = character.partner.as_deref_mut() {
        partner.life = life;
        partner.max_life = PARTNER_LIFE;
        if sherry {
            partner.state &= !crate::partner2::STATE_LOST;
        }
    }
}
/// `fg_status`'s `0x20`: a voice line is playing, and `wsleep` waits.
pub const STATUS_VOICE: u8 = 26;
/// A scene is running: the bars come down and the menus stay shut.
pub const STATUS_SCENE: u8 = 27;
/// What `Set_room` keeps of `fg_status` on every load (`0x4DE93E`): indices
/// 0 to 11 and 17. The scene, the voice line, the camera's lock and the
/// quakes go.
pub const SET_ROOM_STATUS_KEEP: u32 = 0xFFF0_4000;
/// `fg_enemy_0`'s bank: stages below this keep theirs there, `Sce_em_set` says.
const ENEMY_BANK: u8 = 6;
const CAMPAIGN_ENEMY_STAGES: u8 = 3;
/// `Sce_em_set`'s flag of someone who can't die.
pub const NO_ENEMY_FLAG: u8 = 0xFF;
/// `splayer_work`'s place among the characters, after `enemy_work`'s 32:
/// `Set_enemy_work` looks through all 32, and rooms of young spiders place 27.
pub const SPLAYER_SLOT: usize = 32;
pub(crate) const CHARACTER_SLOTS: usize = SPLAYER_SLOT + 1;
/// `sce_em_set`'s slot for `splayer_work`.
const SPLAYER_OPERAND: i64 = 255;
/// `FlagGroup::Message`, `fg_message`: bit 31 is a question's answer, `no`.
const MESSAGE_BANK: u8 = 11;
/// The item-collected flag bank (`FlagGroup::Item`).
pub const ITEM_BANK: u8 = 8;
/// `fg_use` (`0x989ED8`): the key items the areas under the player want. A
/// USE area (`Sce_Flg_chg`) raises the item's bit while the player's probe is
/// in it, and `Sce_work_clr` clears the bank's first four words each pass.
pub const FG_USE_BANK: u8 = 10;
/// The ink ribbon's item id.
pub const ITEM_INK_RIBBON: u8 = 0x1E;

/// Whether picked-up `item` joins a stack: the get-item window's check
/// (`0x4FD5C0`) merges the ammunition, the ink ribbon and the small key.
pub fn item_stacks(item: u8) -> bool {
    (0x14..=0x1F).contains(&item)
}

/// HARD's ammunition counts, the table `Sce_Item` (`0x4E96F0`) and
/// `St_get_item` (`0x4FD88A`) both apply: the handgun rounds' 15 becomes 10
/// and any other count 20 (`SUB AX,0xF; NEG; SBB; AND 0xA; ADD 0xA`), and the
/// shells, magnum, grenade, flame, acid rounds and bolts get a fixed count.
/// The rest are left alone.
fn hard_item_count(item: u8, count: u8) -> u8 {
    match item {
        0x14 if count == 15 => 10,
        0x14 => 20,
        0x15 => 5,
        0x16 => 6,
        0x18..=0x1A => 4,
        0x1D => 12,
        _ => count,
    }
}

/// `Sce_Item`'s rewrite of an item area's count (`0x4E96E4`), which it
/// writes back into the area, before the check sees it: ARRANGE MODE's ink
/// ribbons come two at a time, then HARD's table (`hard_item_count`).
///
/// The take ([`st_get_item_count`]) applies HARD's table to the same area
/// again, and the table isn't idempotent for the handgun rounds: an area of
/// 15 is 10 at the check and 20 at the take, the count the inventory gets.
pub fn sce_item_count(item: u8, count: u8, difficulty: crate::zombie2::Difficulty) -> u8 {
    let count = if difficulty.uncensored && item == 0x1E {
        2
    } else {
        count
    };
    if difficulty.hard {
        hard_item_count(item, count)
    } else {
        count
    }
}

/// `St_get_item`'s rewrite at the take (sub 3, `0x4FD88A`): HARD's table
/// alone, on the area [`sce_item_count`] already rewrote.
pub fn st_get_item_count(item: u8, count: u8, difficulty: crate::zombie2::Difficulty) -> u8 {
    if difficulty.hard {
        hard_item_count(item, count)
    } else {
        count
    }
}

/// The most of `item` one slot holds: byte 0 of the executable's item table
/// (`0x53DE28`, eight bytes an id). A weapon's is its magazine.
pub fn item_max(item: u8) -> u8 {
    const MAX: [u8; 0x20] = [
        0, 1, 18, 13, 18, 8, 8, 5, 7, 250, 250, 250, 18, 6, 100, 100, // 0x00
        100, 4, 100, 15, 250, 250, 250, 100, 250, 250, 250, 100, 100, 240, 250, 250, // 0x10
    ];
    // Past 0x1F every id holds one, but for the two the table gives more.
    MAX.get(usize::from(item)).copied().unwrap_or(match item {
        0x65 => 5,
        0x67 => 6,
        _ => 1,
    })
}

/// The get-item window's slot for `amount` of `item` (`0x4FD5C0`): the first
/// stack of the same id with room for all of it, else a new slot, else
/// `None` (the inventory is full). The pick-up never splits.
pub fn item_get_slot(
    inventory: &[crate::game::InventoryItem],
    capacity: usize,
    item: u8,
    amount: u8,
) -> Option<ItemGetSlot> {
    if item_stacks(item) {
        let room = item_max(item).saturating_sub(amount);
        if let Some(index) = inventory
            .iter()
            .position(|stack| stack.id == item && stack.quantity <= room)
        {
            return Some(ItemGetSlot::Merge(index));
        }
    }
    (inventory.len() < capacity).then_some(ItemGetSlot::New)
}

/// Where [`item_get_slot`] puts a pick-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemGetSlot {
    /// Onto the stack in this slot.
    Merge(usize),
    /// Into the first free slot (`0x502820`).
    New,
}

/// `Sce_at`'s door lock key values.
pub const KEY_UNLOCK: u8 = 0xFE;
pub const KEY_LOCKED: u8 = 0xFF;

/// The executable's system messages (its table at `0x5324D8`), by the
/// original's ids; the pack's language file carries message `n` as
/// `MESSAGE_<nn>`.
/// The get-item window's question, "Will you take the item?" with yes/no.
pub const MESSAGE_TAKE: u8 = 0;
/// "You can't carry any more items."
pub const MESSAGE_FULL: u8 = 1;
/// The get-item window's "You've taken the item."
pub const MESSAGE_TAKEN: u8 = 6;
/// The file pick-up's "filed" message (`St_get_file`'s sub 6, `Mess_set(0xAF0010, 0xE400, 10, 0)`).
pub const MESSAGE_FILED: u8 = 10;
/// The first item id that is a file (`St_init02`: an id above 0x67), file `id - 0x68`.
pub const FIRST_FILE_ITEM: u8 = 0x68;

/// The file an item area's id stands for, if it is one.
pub fn file_of_item(id: u8) -> Option<u8> {
    id.checked_sub(FIRST_FILE_ITEM)
}

/// The global message id of door message 0 (the executable's door table at
/// `0x52F9B4`); door message `n` is `DOOR_MESSAGES + n`.
pub const DOOR_MESSAGES: u8 = 0x21;
/// `Sce_Door`'s message for a door a key just opened: "You have used the
/// item."
pub const DOOR_USED_KEY: u8 = 5;
/// "You unlocked it.", for a door that unlocks from this side.
pub const DOOR_UNLOCKED: u8 = 0xA;
/// "It's locked from inside."
pub const DOOR_LOCKED_OTHER_SIDE: u8 = 0xB;
/// "It's locked.", the door messages' plain lock.
pub const DOOR_LOCKED: u8 = 0xC;
/// The door messages' count.
const DOOR_MESSAGE_COUNT: u8 = 0x18;

/// The global message id of door message `door`.
pub const fn door_message(door: u8) -> u8 {
    DOOR_MESSAGES + door
}

/// The door message `Sce_Door` shows for a locked door whose key the player
/// lacks: `key - 0x4C`, the message that names the key's lock. A key the
/// table has no message for shows the plain lock.
pub fn door_locked_message(key: u8) -> u8 {
    match key.checked_sub(0x4C) {
        Some(door) if door < DOOR_MESSAGE_COUNT => door_message(door),
        _ => door_message(DOOR_LOCKED),
    }
}

/// The script variables the engine owns (`scd_var_*`).
pub const VAR_FORWARD_AREA: usize = 0;
pub const VAR_BODY_AREA: usize = 1;
/// `scd_var_used_item` (`0x98EAE8`): the key item USE gave, for one pass.
pub const VAR_USED_ITEM: usize = 2;
const VAR_STAGE: u8 = 24;
const VAR_ROOM: u8 = 25;
const VAR_CUT: u8 = 26;

/// RE2's `Sce_at_jump_tbl` indices (`SceKind`).
pub const SCE_AUTO: u8 = 0;
const SCE_DOOR: u8 = 1;
const SCE_ITEM: u8 = 2;
const SCE_MESSAGE: u8 = 4;
pub const SCE_EVENT: u8 = 5;
/// `Sce_Flg_chg`'s area (`0x4E9830`): sets or clears a flag while the probe
/// is inside it.
pub const SCE_FLAG_CHANGE: u8 = 6;
/// `Sce_Water`'s area: the player's `Water` while inside it.
pub const SCE_WATER: u8 = 7;
pub const SCE_SAVE: u8 = 9;
const SCE_ITEMBOX: u8 = 10;
/// `Sce_Hikidashi`'s area (`0x4E9CB0`): a drawer
/// ([`crate::game::hikidashi2`]).
pub const SCE_HIKIDASHI: u8 = 13;
pub const SCE_DAMAGE: u8 = 11;
/// `Sce_Windows` (`0x4EA0B0`): game mode 7, the computer
/// ([`crate::game::computer2`]).
pub const SCE_WINDOWS: u8 = 14;

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
            shown_cut: 0,
            view_held: false,
            bg_load: false,
            cut_no_load: false,
            voice: VoiceLine::default(),
            voice_ticks: Default::default(),
            xa_volume: XA_VOLUME,
            pending_events: Vec::new(),
            pending_damage: Vec::new(),
            areas_stopped: false,
            var_cut: None,
            pending_cuts: Vec::new(),
            pending_sca: Vec::new(),
            door_settling: false,
            door_stop: false,
            door_key_stop: false,
            script_door: false,
            work: [Work::Player; TASK_SLOTS],
            speeds: [[0; 12]; TASK_SLOTS],
            pending_player: PendingPlayer::default(),
            key: 0,
            key_trg: 0,
            key_was_stopped: false,
            key_read: false,
            characters: Characters::default(),
            unmodelled_arrivals: Vec::new(),
            room_loading: false,
            player: crate::player2::Work::default(),
            vm: None,
            scenario_b: false,
            personal_item: [0; 2],
            item_box: [[0; 3]; crate::game::itembox2::BOX_SLOTS],
            files: [NO_FILE; FILE_SLOTS],
            hard_mode: false,
            censorship_off: false,
            bgm: Default::default(),
            main_music: None,
            music: Default::default(),
            music_set: false,
            effects: Default::default(),
            sounds: Vec::new(),
            effect_sounds: Vec::new(),
            scroll: 0,
            rnd: Default::default(),
            tongue_owner: 0,
            em2d_groan: 0,
            swarm: [0; 4],
            draw_buffer: 0,
            enemies: 0,
            sound_types: [0; 2],
            enemy_bank: None,
            load_pass: false,
            pending_esprs: Vec::new(),
            screen_fades: Vec::new(),
            fades: [crate::transition::Fade::inactive(); 3],
            fades_drawn: [None; 3],
            films: false,
            film: None,
            movie_set: None,
            sound_close: None,
            sound_init: false,
            spl_pos_ck_timer: crate::partner2::SPL_POS_CK_START,
            nearest_dist: [0; 3],
            partner_life: PARTNER_LIFE,
            partner_poisoned: false,
        }
    }
}

impl Re2State {
    /// The state a save keeps: the flags, variables, door locks and music
    /// table. The rest belongs to the room, and the room's init rebuilds it.
    pub fn to_save_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SAVE_LEN);
        for word in self.banks[..SAVE_BANKS]
            .iter()
            .flatten()
            .chain(&self.door_locks)
        {
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
        out.extend_from_slice(&self.personal_item);
        out.extend(self.item_box.iter().flatten());
        out.push(u8::from(self.hard_mode));
        out.push(u8::from(self.censorship_off));
        for word in self.banks[SAVE_BANKS..].iter().flatten() {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&self.files);
        out.extend_from_slice(&self.partner_life.to_le_bytes());
        out.push(u8::from(self.partner_poisoned));
        out
    }

    /// `Sce_heal_partner` (opcode 0x89, `0x4e8d70`): the partner's poison
    /// goes, and her life, saved and in play, is 200 of 200.
    pub fn heal_partner(&mut self) {
        self.partner_poisoned = false;
        self.partner_life = PARTNER_LIFE;
        let partner = self
            .characters
            .0
            .get_mut(SPLAYER_SLOT)
            .and_then(|character| character.as_deref_mut())
            .and_then(|character| character.partner.as_deref_mut());
        if let Some(partner) = partner {
            partner.life = PARTNER_LIFE;
            partner.max_life = PARTNER_LIFE;
        }
    }

    /// `St_file_set` (`0x5023B0`): put `file` in the first free slot of
    /// `File_tag_bk`, so a binder fills from slot 0 and the next starts after
    /// its eight. Returns the slot, or `None` when all 24 are held.
    pub fn file_set(&mut self, file: u8) -> Option<usize> {
        let slot = self.files.iter().position(|&held| held == NO_FILE)?;
        self.files[slot] = file;
        Some(slot)
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
        if bytes.len() != SAVE_LEN
            && bytes.len() != SAVE_LEN_NO_PARTNER
            && bytes.len() != SAVE_LEN_NO_FILES
            && bytes.len() != SAVE_LEN_NO_MAP
            && bytes.len() != SAVE_LEN_NO_CHOICE
            && bytes.len() != SAVE_LEN_NO_BOX
            && bytes.len() != SAVE_LEN_NO_PERSONAL
        {
            return None;
        }
        let mut words = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|raw| u32::from_le_bytes(*raw));
        for word in self.banks[..SAVE_BANKS]
            .iter_mut()
            .flatten()
            .chain(&mut self.door_locks)
        {
            *word = words.next()?;
        }
        // A save from before bank 31 was `fg_key` kept a script's `set 31:n`
        // apart from the locks: merge it into them.
        let old_keys = std::mem::take(&mut self.banks[usize::from(KEY_BANK)]);
        self.door_locks[0] |= old_keys[0];
        self.door_locks[1] |= old_keys[1];
        let rest = &bytes[(SAVE_BANKS * 8 + 2) * 4..];
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
        self.personal_item = [
            rest.get(5).copied().unwrap_or(0),
            rest.get(6).copied().unwrap_or(0),
        ];
        let boxed = rest.get(7..).unwrap_or(&[]);
        for (index, slot) in self.item_box.iter_mut().enumerate() {
            for (byte, value) in slot.iter_mut().enumerate() {
                *value = boxed.get(index * 3 + byte).copied().unwrap_or(0);
            }
        }
        let choice = rest
            .get(7 + crate::game::itembox2::BOX_SLOTS * 3..)
            .unwrap_or(&[]);
        self.hard_mode = choice.first().is_some_and(|&byte| byte != 0);
        self.censorship_off = choice.get(1).is_some_and(|&byte| byte != 0);
        // A save from before them loads with the map's banks clear.
        let mut map = bytes
            .get(SAVE_LEN_NO_MAP..SAVE_LEN_NO_FILES.min(bytes.len()))
            .unwrap_or(&[])
            .as_chunks::<4>()
            .0
            .iter();
        for word in self.banks[SAVE_BANKS..].iter_mut().flatten() {
            *word = map.next().map_or(0, |raw| u32::from_le_bytes(*raw));
        }
        // A save from before the files loads with every slot empty.
        let files = bytes.get(SAVE_LEN_NO_FILES..).unwrap_or(&[]);
        for (index, file) in self.files.iter_mut().enumerate() {
            *file = files.get(index).copied().unwrap_or(NO_FILE);
        }
        // A save from before them has the new game's: 200, not poisoned.
        let partner = bytes.get(SAVE_LEN_NO_PARTNER..).unwrap_or(&[]);
        self.partner_life = match partner {
            [low, high, ..] => i16::from_le_bytes([*low, *high]),
            _ => PARTNER_LIFE,
        };
        self.partner_poisoned = partner.get(2).is_some_and(|&byte| byte & 1 != 0);
        Some(())
    }

    /// The title's new-game choice, as the original's game starts with it:
    /// the difficulty confirm's EASY (`fg_system`'s `0x20`) and `hard_mode`,
    /// ARRANGE MODE's `censorship_off` and the title's `fg_system` `0x40` for
    /// it (`Title_sel_menu`), and `Title_game_init`'s new-game step, which
    /// raises `fg_status`'s `0x4000000` under EASY and `fg_system`'s
    /// `0x80000` under EASY with `censorship_off`.
    pub fn start_new_game(&mut self, easy: bool, hard_mode: bool, censorship_off: bool) {
        self.set_flag(0, FG_SYSTEM_EASY, u8::from(easy));
        self.set_flag(0, FG_SYSTEM_CENSORSHIP_OFF, u8::from(censorship_off));
        self.set_flag(
            0,
            FG_SYSTEM_EASY_CENSORSHIP_OFF,
            u8::from(easy && censorship_off),
        );
        self.set_flag(STATUS_BANK, FG_STATUS_EASY, u8::from(easy));
        self.hard_mode = hard_mode;
        self.censorship_off = censorship_off;
    }

    /// The choice's two bytes, as the routines that read them see them.
    pub fn difficulty(&self) -> crate::zombie2::Difficulty {
        crate::zombie2::Difficulty {
            hard: self.hard_mode,
            uncensored: self.censorship_off,
        }
    }

    /// Whether a script holds the player's input, or a door has just handed
    /// back: the original's first tick of play after a door still has
    /// `fg_stop` at `0xEF000000`, input bit included, when `Pad_set` maps the
    /// pad, so a direction held through the door moves the player a tick
    /// later ([`Self::door_key_stop`]).
    pub fn input_stopped(&self) -> bool {
        self.flag(STOP_BANK, STOP_INPUT) || self.door_key_stop
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

    /// The parts of the zombies in play that are hidden or coloured: the
    /// oracle's `zp` field. Each is the slot, the mask of the parts of
    /// `0..Parts_num` that `Be_flg` doesn't draw, part 0's `Poly_rgb`, and the
    /// wrapping sum of every part's.
    pub fn zombie_parts_trace(&self) -> String {
        let rows: Vec<String> = self
            .characters
            .0
            .iter()
            .enumerate()
            .filter_map(|(slot, character)| {
                let character = character.as_deref()?;
                let zombie = character.zombie.as_ref()?;
                let count = usize::from(zombie.parts_num);
                if character.be_flg & 1 == 0 || count == 0 {
                    return None;
                }
                let mask = if count >= 32 {
                    u32::MAX
                } else {
                    (1 << count) - 1
                };
                let mut hidden = character.work.hidden_parts & mask;
                // The licker's tongue is drawn only while it's out
                // (`tongue_move` sets its `Be_flg`).
                if zombie
                    .licker
                    .as_ref()
                    .is_some_and(|licker| licker.tongue_weights().is_none())
                {
                    hidden |= 1 << crate::zombie2::licker::TONGUE_PART;
                }
                let colours = &character.work.part_rgb[..count];
                if hidden == 0
                    && colours
                        .iter()
                        .all(|&rgb| rgb == crate::player2::DEFAULT_PART_RGB)
                {
                    return None;
                }
                let sum = colours
                    .iter()
                    .fold(0u32, |sum, &rgb| sum.wrapping_add(rgb & 0x00FF_FFFF));
                Some(format!(
                    "{slot}:{hidden:x},{:06x},{sum:x}",
                    colours[0] & 0x00FF_FFFF
                ))
            })
            .collect();
        if rows.is_empty() {
            "-".to_string()
        } else {
            rows.join(";")
        }
    }

    /// Where each zombie in play is heading: the oracle's `zd` field, the
    /// slot and `Dest` (`+0x1c4`, `+0x1c6`). `Root_ck` writes it. Neither
    /// `Sce_em_set` nor `Em10_Init` does: the new work is allocated at
    /// `Mem_top`, so it starts as whatever last sat at that address.
    pub fn zombie_dest_trace(&self) -> String {
        let rows: Vec<String> = self
            .characters
            .0
            .iter()
            .enumerate()
            .filter_map(|(slot, character)| {
                let character = character.as_deref()?;
                character.zombie.as_ref()?;
                if character.be_flg & 1 == 0 {
                    return None;
                }
                let [x, z] = character.work.dest;
                Some(format!("{slot}:{x},{z}"))
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

    /// The `spl` field: `splayer_work`'s own words for `Spl_Auto`, `-` with no
    /// character there. `Type`, the follow point, its state, distance and
    /// wall turn, the wall and touch counts, the wait, `Timer0` to `Timer3`,
    /// `Dest`, the life, `Sca_info`'s 1, `Root_ck`'s turn,
    /// `Spl_pos_ck_timer`, `Neck_flg` and `At_em_flg`, which only the
    /// player's `Oba_ck_em2` writes and so is 0 here.
    pub fn partner_trace(&self) -> String {
        let Some(character) = self
            .characters
            .0
            .get(SPLAYER_SLOT)
            .and_then(|character| character.as_deref())
            .filter(|character| character.zombie.is_none() && character.id >= 0x40)
        else {
            return "-".to_string();
        };
        let Some(p) = character.partner.as_deref() else {
            return "-".to_string();
        };
        let [dest_x, dest_z] = character.work.dest;
        format!(
            "{:x},{},{},{:x},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:x},0",
            character.kind,
            p.soba[0],
            p.soba[1],
            p.state,
            p.dist,
            p.turn,
            p.stuck,
            p.touch,
            p.wait,
            p.timer0,
            p.timer1,
            p.timer2,
            p.timer3,
            dest_x,
            dest_z,
            p.life,
            p.max_life,
            u8::from(p.sca_hit),
            p.root_cnt,
            self.spl_pos_ck_timer,
            character.work.neck.flags,
        )
    }

    /// `fg_stop`'s second bit keeps `splayer_work` from running, and its third
    /// the enemy slots. A door the scripts opened this tick has raised them all
    /// before the enemies' pass ([`Self::script_door`]).
    pub fn character_stopped(&self, slot: usize) -> bool {
        self.script_door || self.flag(STOP_BANK, if slot == SPLAYER_SLOT { 1 } else { 2 })
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

    /// The oracle's `fguse` (`fg_use`'s four words as their bytes), `scv`
    /// (variables 0 to 3) and `atsce` (the player's `At_sce_no`), what
    /// `Sce_work_clr` and `Sce_work_clr_at` reset.
    pub fn script_work_trace(&self) -> String {
        let fg_use: String = self.banks[usize::from(FG_USE_BANK)][..4]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let vars = self.vars[..4]
            .iter()
            .map(|var| format!("{var:04x}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("fguse={fg_use} scv={vars} atsce={:02x}", self.player.at_sce)
    }

    /// The cut a sound is placed from: the one a script's `se_on` saw, or
    /// else the live `scd_var_cut` ([`Self::var_cut`]). `None` is the tick's
    /// cut.
    pub fn placement_cut(&self, placed: Option<u8>) -> Option<u8> {
        placed.or(self.var_cut)
    }

    /// `Sce_work_clr` (`0x4E3DA0`), at the end of every `Sce_scheduler_main`
    /// pass and before a room's init scripts: variables 0 to 3 go back to
    /// `0xFFFF`, and `fg_tick` and `fg_use`'s four words to 0, so a USE's
    /// item, an area's key item and the areas the player was in last one
    /// pass. `Mizu_div_ctr`, which it
    /// also zeroes, isn't modelled (`mizu_div_set` is a no-op).
    pub fn sce_work_clr(&mut self) {
        self.vars[..4].fill(0xFFFF);
        if let Some(words) = self.banks.get_mut(usize::from(ROOM_BANK)) {
            words[1] = 0;
        }
        if let Some(words) = self.banks.get_mut(usize::from(FG_USE_BANK)) {
            words[..4].fill(0);
        }
        for flag in std::mem::take(&mut self.unmodelled_arrivals) {
            self.raise_room_flag(flag);
        }
    }

    /// `Sce_work_clr_at` (`0x4E3DE0`), after [`Self::sce_work_clr`] at the
    /// end of `Sce_scheduler_main`'s pass: the player and every enemy in play
    /// forget their area, so a script sees only the one the last `Sce_at`
    /// found them in.
    pub fn sce_work_clr_at(&mut self) {
        self.player.at_sce = NO_AREA;
        for character in self.characters.0.iter_mut().flatten() {
            if character.be_flg & 1 != 0 {
                character.area = NO_AREA;
            }
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
    ///
    /// `player` is the player's place and life, for what an effect hits.
    /// [`Self::tick_effects`] under `fg_stop`'s `0x10000000` or not,
    /// whatever the bank holds now: a get-item window's frame
    /// ([`crate::game::GameState::re2_item_get_effects_frame`]).
    pub fn tick_effects_as(
        &mut self,
        stopped: bool,
        sca: Option<&crate::state::Sca>,
        player: Option<([i32; 3], &mut i16)>,
    ) {
        let held = self.flag(STOP_BANK, STOP_EFFECTS);
        self.set_flag(STOP_BANK, STOP_EFFECTS, u8::from(stopped));
        self.tick_effects(sca, player);
        self.set_flag(STOP_BANK, STOP_EFFECTS, u8::from(held));
    }

    pub fn tick_effects(
        &mut self,
        sca: Option<&crate::state::Sca>,
        player: Option<([i32; 3], &mut i16)>,
    ) {
        // `Joint_trans2` comes just before `Esp_move`, and makes `Parts_down`'s calls.
        for character in self.characters.0.iter_mut().flatten() {
            let Some(zombie) = character.zombie.as_mut() else {
                continue;
            };
            let (rnd, effects) = (&mut self.rnd, &mut self.effects);
            let mut off = Vec::new();
            zombie.thrown.retain_mut(|thrown| {
                if off.contains(&thrown.part) {
                    return false;
                }
                let falling = thrown.fall(rnd, effects, sca);
                if !falling {
                    crate::player2::set_part_flags(&mut character.work, thrown.part, 0);
                    if thrown.takes_below() {
                        for &part in &thrown.below {
                            crate::player2::set_part_flags(&mut character.work, part, 0);
                            off.push(part);
                        }
                    }
                }
                falling
            });
        }
        let atd = self.player.atd;
        self.effects.target = player.as_ref().map(|(pos, life)| crate::esp2::Target {
            pos: *pos,
            ofs_y: atd.map_or(-(crate::character2::PLAYER_HEIGHT as i16), |atd| atd.ofs_y),
            at_h: atd.map_or(crate::character2::PLAYER_HEIGHT, |atd| atd.at_h),
            life: **life,
            near_death: self.player.near_death,
            damage_cnt: self.player.damage_cnt,
            difficulty: self.difficulty(),
            ..Default::default()
        });
        self.effects.bodies = crate::character2::targets_of(self);
        if self.flag(STOP_BANK, STOP_EFFECTS) {
            self.effects.tick_stopped(&mut self.rnd, sca);
        } else {
            self.effects.tick(&mut self.rnd, sca);
        }
        self.effects.bodies.clear();
        for blast in std::mem::take(&mut self.effects.blasts) {
            let slot = match blast {
                crate::weapon2::BlastEffect::Tested(slot) => slot,
                crate::weapon2::BlastEffect::Hit(hit) => hit.slot,
            };
            let Some(character) = self.characters.0.get_mut(slot).and_then(Option::as_mut) else {
                continue;
            };
            if let Some(mut zombie) = character.zombie.take() {
                crate::zombie2::take_blast(character, &mut zombie, &blast);
                character.zombie = Some(zombie);
            }
        }
        if let (Some(target), Some((_, life))) = (self.effects.target.take(), player) {
            *life = target.life;
            self.player.near_death = target.near_death;
            self.player.damage_cnt = target.damage_cnt;
            if let Some(routine) = target.routine {
                self.player.routine = [routine, 0, 0, 0];
            }
            if target.poisoned {
                self.player.poisoned = true;
                self.player.poison_timer = 0x1E;
            }
        }
        for (id, pos) in std::mem::take(&mut self.effects.sounds) {
            if let Some(id) = crate::se2::SoundId::decode(id) {
                let pos = pos.map(i32::from);
                self.effect_sounds.push(SoundRequest::new(id, pos));
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
        if self.enemies == 0 && !self.partner_bit() {
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
                    // A script character's `Life` is -1: Kendo's is, in room 101,
                    // so the back walk's look (`FUN_004daa60`) passes him by.
                    // The partner's is hers (`Spl_in45`).
                    life: match (&character.zombie, &character.partner) {
                        (Some(zombie), _) => zombie.life,
                        (None, Some(partner)) => partner.life,
                        (None, None) => -1,
                    },
                    angle: character.body.angle,
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

    /// `Sce_work_clr_set` (`0x4E3E50`): every room starts with all of its mask
    /// groups on and with `fg_room`'s first word (`0x98EB6C`) clear, so a room's
    /// scripts never see the last room's bits, nor those of an earlier pass of
    /// their own load.
    pub fn reset_mask_groups(&mut self) {
        self.banks[MASK_BANK..MASK_BANK + MASK_BANKS].fill([u32::MAX; 8]);
        self.banks[usize::from(ROOM_BANK)][0] = 0;
    }

    /// The room's script set-up (`0x4E3F12`, from the room load's
    /// `0x4E42D0`) puts the voice channel back to `xa_vol`'s default.
    pub fn reset_xa_volume(&mut self) {
        self.xa_volume = XA_VOLUME;
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

    /// `fg_status`'s `0x10000000`: the partner is with the player.
    pub fn partner_bit(&self) -> bool {
        self.flag(STATUS_BANK, PARTNER_BIT)
    }

    pub fn set_partner_bit(&mut self, on: bool) {
        self.set_flag(STATUS_BANK, PARTNER_BIT, u8::from(on));
    }

    /// `fg_status`'s `0x2000`: last tick's `Om_move` pushed an object, which
    /// `pl_br_01` turns into the push routine.
    pub fn pushing(&self) -> bool {
        self.flag(STATUS_BANK, PUSH_BIT)
    }

    pub fn set_pushing(&mut self, on: bool) {
        self.set_flag(STATUS_BANK, PUSH_BIT, u8::from(on));
    }

    /// `fg_stop`'s `0x8000000`, which stops `Om_move`.
    pub fn objects_stopped(&self) -> bool {
        self.flag(STOP_BANK, OBJECTS_STOP_BIT)
    }

    pub fn flag(&self, bank: u8, index: u8) -> bool {
        let bit = 0x8000_0000 >> (index & 31);
        if bank == KEY_BANK {
            return self.door_locks[key_word(index)] & bit != 0;
        }
        let Some(words) = self.banks.get(usize::from(bank)) else {
            return false;
        };
        words[usize::from(index >> 5) & 7] & bit != 0
    }

    pub fn set_flag(&mut self, bank: u8, index: u8, op: u8) {
        let word = if bank == KEY_BANK {
            &mut self.door_locks[key_word(index)]
        } else {
            let Some(words) = self.banks.get_mut(usize::from(bank)) else {
                return;
            };
            &mut words[usize::from(index >> 5) & 7]
        };
        let bit = 0x8000_0000 >> (index & 31);
        match op {
            0 => *word &= !bit,
            1 => *word |= bit,
            7 => *word ^= bit,
            _ => {}
        }
    }

    /// `Set_room`'s `fg_status &= 0xFFF04000`, on every room load. Only word
    /// 0 of the bank is `fg_status`.
    pub fn enter_room_status(&mut self) {
        self.banks[usize::from(STATUS_BANK)][0] &= SET_ROOM_STATUS_KEEP;
    }
}

/// `Flg_addr[31]` is `fg_key` (`0x98ED2C`): the doors' lock bits, the
/// words [`Re2State::door_locks`] holds, so a script's `ck 31:n` and `set
/// 31:n` read and raise the bits a door's lock byte names.
pub const KEY_BANK: u8 = 31;

/// The `door_locks` word of `fg_key` bit `index`. The original's two words
/// are followed by `Item_work`, which an index of 64 or more would reach; no
/// script does, so such an index is logged and wraps onto the two words.
fn key_word(index: u8) -> usize {
    if index >= 64 {
        crate::logging::write(&format!(
            "warning: fg_key bit {index} is past the lock words"
        ));
    }
    usize::from(index >> 5) & 1
}

/// The gate on one interaction area, on top of its [`RoomAction`] box.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActionGate {
    /// The four corners of a `*_4p` area, which replace the box.
    pub quad: Option<[[i16; 2]; 4]>,
    /// The floor the area is on; `None` for every floor.
    pub floor: Option<u8>,
    /// The area's `Super` byte when its top bit is set: the area moves with
    /// a work (`0x40`, an index into `0x98A10C`: the player, the second
    /// player, then the enemy slots) or a room object, by that one's `x` and
    /// `z` (`Sce_at_check`, `0x4E9227`).
    pub parent: Option<u8>,
}

impl ActionGate {
    /// A gate from an area's floor and `Super` bytes.
    pub fn new(quad: Option<[[i16; 2]; 4]>, floor: u8, parent: u8) -> Self {
        Self {
            quad,
            floor: (floor < 0x80).then_some(floor),
            parent: (parent & 0x80 != 0).then_some(parent),
        }
    }

    /// The gate and box moved by `origin`, as `Sce_at_check` adds its
    /// parent's `x` and `z` to each corner (16-bit sums).
    pub fn shifted(&self, zone: [i16; 4], [dx, dz]: [i16; 2]) -> (Self, [i16; 4]) {
        let quad = self
            .quad
            .map(|corners| corners.map(|[x, z]| [x.wrapping_add(dx), z.wrapping_add(dz)]));
        let [x, z, width, depth] = zone;
        (
            Self { quad, ..*self },
            [x.wrapping_add(dx), z.wrapping_add(dz), width, depth],
        )
    }

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
/// `Id`.
const MEMBER_ID: u8 = 6;
/// `+0xB`, the enemy's last area ([`Character::area`]).
const MEMBER_AREA: u8 = 9;
/// [`Character::area`] before the enemy has been in an area: `sce_em_set`'s.
pub const NO_AREA: u8 = 0xFF;
/// `Type`.
const MEMBER_TYPE: u8 = 7;
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
pub(crate) fn log_unmodelled(mnemonic: &'static str) {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    static SEEN: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    if SEEN.lock().is_ok_and(|mut seen| seen.insert(mnemonic)) {
        crate::logging::write(&format!("RE2 script: {mnemonic} is not modelled"));
    }
}

/// Run one script instruction on `game`, for the tests outside this module.
#[cfg(test)]
pub(crate) fn run_op_for_test(game: &mut GameState, mnemonic: &str, operands: &[i64]) {
    let op = crate::scd2::opcode::by_mnemonic(mnemonic).unwrap();
    GameHost { game }.exec(0, op, operands);
}

/// The script host over the game state.
pub struct GameHost<'a> {
    pub game: &'a mut GameState,
}

impl GameHost<'_> {
    /// `Movie_on` (0x4E8C40): `Init_movie_work(id)` (0x4CAEC0), then
    /// `Movie_set(2)`. The script goes on in the same tick.
    ///
    /// `Init_movie_work` clears `fg_system`'s `0x8000`, raises `0x200` and
    /// closes the sound (`SsClose`). With the films on it leaves the film to
    /// `WinMain`, which plays it before the next `Psx_main`; with them off
    /// (as the oracle runs) it opens the sound again at once (`SsInit2`) and
    /// drops `0x200`, so no tick's end sees it. The gallery's registry flags
    /// aren't kept, and neither is `BYTE_0098F07B = 0`, which cancels a cut
    /// change's background load in the same tick: no room changes the cut and
    /// calls `movie_on` with no wait between them.
    fn movie_on(&mut self, id: u8) {
        let re2 = &mut self.game.re2;
        re2.set_flag(0, FG_SYSTEM_8000, 0);
        re2.set_flag(0, FG_SYSTEM_MOVIE, 1);
        re2.sound_close = Some(re2.sounds.len());
        if re2.films {
            re2.film = Some(id);
        } else {
            re2.sound_init = true;
            re2.set_flag(0, FG_SYSTEM_MOVIE, 0);
        }
        re2.movie_set = Some(2);
    }

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
            [at(2) as u8, at(3) as u8, at(4) as u8],
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
        // `Item_aot_set`: the object of an item that's been taken stays away,
        // whatever `Obj_model_set` asks of it.
        let taken = self.game.re2.flag(ITEM_BANK, flag);
        if model < 0x20
            && let Some(record) = self.game.objects.record_mut(usize::from(model))
        {
            record.item_flags = if taken { 0 } else { action };
            if taken || action & 2 != 0 {
                record.be_flg = 0x8000_0000;
                record.flag = 0;
            }
        }
        let params = [item, amount, flag, model, action, 0, 0, 0];
        self.write_area(
            at(0) as usize,
            sce,
            [at(2) as u8, at(3) as u8, at(4) as u8],
            zone,
            quad,
            params,
        );
    }

    /// Put an area in its slot; `[sat, floor, parent]` are its `Sat`,
    /// `nFloor` and `Super` bytes.
    fn write_area(
        &mut self,
        slot: usize,
        sce: u8,
        [sat, floor, parent]: [u8; 3],
        zone: [i16; 4],
        quad: Option<[[i16; 2]; 4]>,
        params: [u8; 8],
    ) {
        if slot >= ROOM_ACTION_SLOTS {
            crate::warning!("warning: RE2 area {slot} is past the action slots");
            return;
        }
        self.game.re2.gates[slot] = ActionGate::new(quad, floor, parent);
        self.game.doors[slot] = None;
        self.game.room_actions[slot] = Some(area_action(slot as u8, sce, sat, zone, params));
    }

    /// `Aot_reset` (`0x4E5600`): an existing area's kind, trigger and first
    /// three data words change; its floor, `Super`, box and the rest of its
    /// data stay. An item's data is `[item, count, flag]` as words, then the
    /// model and action bytes, which it keeps: room 103's key turns from
    /// Brad's enemy-only area into the player's when he dies, and stays a
    /// crouching pick-up.
    fn reset_area(&mut self, ops: &[i64]) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let slot = at(0) as usize;
        let Some(Some(old)) = self.game.room_actions.get(slot).copied() else {
            return;
        };
        let data: [u8; 6] = std::array::from_fn(|index| at(3 + index) as u8);
        let mut params = [0; 8];
        if at(1) as u8 == SCE_ITEM {
            let [model, action] = if old.kind == RoomActionKind::Item {
                [old.params[3], old.params[4]]
            } else {
                [old.params[6], old.params[7]]
            };
            params[..5].copy_from_slice(&[data[0], data[2], data[4], model, action]);
        } else {
            params[..6].copy_from_slice(&data);
            // `Aot_reset` writes only the area's first three data words: an
            // item's object and action bytes stay for a later reset to an
            // item (room 101's shotgun, off at the load and back on Kendo's
            // second check, is still picked up crouching).
            let kept = if old.kind == RoomActionKind::Item {
                [old.params[3], old.params[4]]
            } else {
                [old.params[6], old.params[7]]
            };
            params[6..].copy_from_slice(&kept);
        }
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
    /// `cut_old`. `FUN_004e5020` raises the pending load; `Cut_chg` drops
    /// it again when the operand's bit 7 is set.
    fn change_cut(&mut self, cut: u8) {
        // `FUN_004e5020` sets `scd_var_cut` at once, so every sound the
        // tick plays after it is placed from the new cut.
        self.game.re2.var_cut = Some(cut & 0x7F);
        self.game.re2.cut_old = self.game.camera.current_cut as u8;
        self.game.camera.locked = true;
        self.game.set_camera_cut(usize::from(cut & 0x7F));
        self.game.re2.bg_load = cut & 0x80 == 0;
        self.game.re2.cut_no_load = cut & 0x80 != 0;
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
        let licker = matches!(id, 0x22 | 0x24);
        let zombie = (0x10..=0x21).contains(&id)
            || licker
            || id == crate::zombie2::MRX_ID
            || id == crate::zombie2::ALLIGATOR_ID
            || id == crate::zombie2::G1_ID
            || id == crate::zombie2::G2_ID
            || id == crate::zombie2::G3_ID
            || id == crate::zombie2::SPIDER_ID
            || id == crate::zombie2::YOUNG_ID
            || id == crate::zombie2::EM2D_ID
            || id == crate::zombie2::ROACH_ID
            || id == crate::zombie2::EMBRYO_ID
            || crate::zombie2::is_plant(id);
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
            let room = u16::from(self.game.id.stage) << 8 | u16::from(self.game.id.room);
            let given = &mut self.game.re2_enemy_dest;
            if let Some(index) = given.iter().position(|&(r, s, _)| (r, s) == (room, slot)) {
                work.dest = given.remove(index).2;
            }
            let given = &mut self.game.re2_enemy_speed;
            if let Some(index) = given.iter().position(|&(r, s, _)| (r, s) == (room, slot)) {
                work.spd = given.remove(index).2;
            }
        }
        let room = u16::from(self.game.id.stage) << 8 | u16::from(self.game.id.room);
        let mut vars = Vec::new();
        self.game.re2_enemy_vars.retain(|&(r, s, index, value)| {
            let ours = zombie && (r, s) == (room, slot);
            if ours {
                vars.push((index, value));
            }
            !ours
        });
        let characters = &mut self.game.re2.characters.0;
        characters.resize_with(CHARACTER_SLOTS, || None);
        characters[slot] = Some(Box::new(Character {
            id,
            model: id,
            em_set: (at(5) as u8, at(7) as u8, dead),
            arms: Arms::default(),
            be_flg: 1,
            work,
            body,
            kind,
            start: (at(13) as u16, at(14) as u16),
            sound_type,
            distance: 0,
            area: NO_AREA,
            with_room: self.game.re2.room_loading,
            held_bank: None,
            kage: None,
            partner: (slot == SPLAYER_SLOT && !zombie)
                .then(|| Box::new(crate::partner2::Partner::placed())),
            zombie: zombie.then(|| {
                let mut zombie = crate::zombie2::spawned(slot as u8, at(5) as u8, at(7) as u8);
                zombie.dead_flag = dead;
                if licker {
                    zombie.licker = Some(Box::default());
                }
                for (index, value) in vars {
                    zombie.set_var(index, value);
                }
                zombie
            }),
        }));
    }

    /// Set the player's position or facing for `pos_set` and `dir_set`.
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

    /// `Sce_se_on`: its position is its offset from the body it names.
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
            // `pOm[n]`'s position, as `Sce_se_on` reads it.
            4 => self
                .game
                .objects
                .record(usize::from(work >> 8))
                .map_or(player, |object| object.pos),
            _ => player,
        };
        let offset = [at(3) as i32, at(4) as i32, at(5) as i32];
        self.game.re2.sounds.push(SoundRequest {
            id,
            pos: std::array::from_fn(|axis| origin[axis] + offset[axis]),
            cut: self.game.re2.var_cut,
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
            MEMBER_AREA => Some(i32::from(self.game.re2.player.at_sce)),
            // `Type`'s low bits are the weapon in hand, which the engine keeps
            // with the inventory.
            MEMBER_TYPE => Some(i32::from(
                self.game.re2.player.kind & !0x0FFF | u16::from(self.game.equipped_weapon_id),
            )),
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
        let slot = match self.game.re2.work.get(task) {
            Some(Work::Character(slot)) => *slot,
            _ => 0,
        };
        if let Some(character) = self.task_character(task) {
            set_character_member(character, member, value);
            if member == MEMBER_ID {
                become_zombie(character, slot);
            }
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
        let item_flags = record.item_flags;
        // Nor does it write `Old`, `Old2` (+0x44..+0x4E), `Push_cnt` (+0x144),
        // `At_obj_no` or `At_sce_no`: they outlast a re-set. `Om_init` clears
        // the routines.
        let kept = *record;
        let mut be_flg = if record.be_flg == 0 {
            (at(7) as i16 as i32 | 1) as u32
        } else {
            0
        };
        if attr & 0x20 != 0 {
            be_flg &= !1;
        }
        let selector = (at(5) as u16 >> 8) as u8;
        let parent = crate::objects::SuperParent::decode(selector, 0);
        let mut x = at(9) as i32;
        // `Obj_model_set` and `Om_trans`' own room: its first four 0x28 to -x (`Pos_x`, +0x38).
        if stage_room == (2, 3) && slot < 4 {
            x -= 0x28;
        }
        *record = crate::objects::ObjectRecord {
            flag: if be_flg & 1 != 0 {
                crate::objects::OBJECT_FLAG_ACTIVE
            } else {
                0
            },
            model: slot as u8,
            pos: [x, at(10) as i32, at(11) as i32],
            rotation: [at(12) as i16, at(13) as i16, at(14) as i16],
            asset: Some(slot as u8),
            be_flg,
            attr,
            draw_lift: if at(1) == 2 { 900 } else { 0 },
            id: at(1) as u8,
            // `Obj_model_set`'s bytes 6 and 8: the floor and `Type`.
            floor: (at(5) & 0xFF) as u8,
            kind: (at(6) & 0xFF) as u16,
            item_flags,
            // The format reads `At_h` and `At_d` as byte pairs.
            atd2: crate::objects::ObjectAtd {
                ofs: [at(15) as i16, at(16) as i16, at(17) as i16],
                at_w: at(18) as u16,
                at_h: (at(19) | at(20) << 8) as u16,
                at_d: (at(21) | at(22) << 8) as u16,
            },
            old: kept.old,
            old2: kept.old2,
            push_cnt: kept.push_cnt,
            at_sce_no: kept.at_sce_no,
            at_obj_no: kept.at_obj_no,
            ..crate::objects::ObjectRecord::default()
        };
        record.set_super_parent(parent);
        record.set_ofs();
        let player = (self.game.entities[0].pos, self.game.entities[0].angle);
        self.game.compose_object_world(slot, player);
    }

    /// `door_aot_set` and `door_aot_set_4p`: the area and its door record.
    fn set_door(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let slot = at(0) as usize;
        if slot >= ROOM_ACTION_SLOTS {
            crate::warning!("warning: RE2 door area {slot} is past the action slots");
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
        self.game.re2.gates[slot] = ActionGate::new(quad, floor, at(4) as u8);
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
                    // The scripts run after `Player_move`, and a crouching
                    // pick-up's `Sce_Item` sets the routine there and then.
                    if self.game.re2_crouch_start {
                        self.game.tick_re2_crouch();
                    }
                }
            }
            "message_on" => {
                self.game.show_re2_message(at(1) as u8);
                // Its last two bytes, a word, are the `fg_stop` bits the
                // message holds up while it shows: `0xFFFF` on a check in
                // play, none in a scene.
                let stop = u16::from_le_bytes([at(3) as u8, at(4) as u8]);
                self.game.hold_re2_message_stop(stop);
            }
            "cut_chg" => self.change_cut(at(0) as u8),
            "cut_old" => {
                let cut = self.game.re2.cut_old;
                self.game.re2.var_cut = Some(cut);
                self.game.camera.locked = false;
                self.game.set_camera_cut(usize::from(cut));
                // `Cut_old_0` goes through `FUN_004e5020` too.
                self.game.re2.bg_load = true;
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
            // `Super_set` (`0x4E59E0`): the object hangs on a parent, and its
            // position and rotation become offsets on it. `Om_trans` draws it
            // there (not drawn yet).
            "super_set" if self.task_object(task).is_some() => {
                let parent = crate::objects::SuperParent::decode(at(1) as u8, at(2) as u8);
                if let Some(object) = self.task_object(task) {
                    object.set_super_parent(parent);
                    object.pos = [at(3), at(4), at(5)].map(|v| i32::from(v as i16));
                    object.rotation = [at(6) as i16, at(7) as i16, at(8) as i16];
                }
            }
            // `Super_reset` (`0x4E5E30`): back on the identity matrix, at the
            // world position `Obj_model_set` or `Om_trans` last composed
            // (`+0x68..+0x70`), with the rotation from its operands. Five of
            // the scripts move the object on from there with `add_speed`.
            "super_reset" if self.task_object(task).is_some() => {
                if let Some(object) = self.task_object(task) {
                    object.set_super_parent(crate::objects::SuperParent::World);
                    object.pos = object.world.t;
                    object.rotation = [at(1), at(2), at(3)].map(|v| v as i16);
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
            // `Parts_set` writes the task's work's part: an object's, or no
            // work's, isn't modelled and does nothing.
            "parts_set" => {
                let (part, case, value) = (at(1) as u8 as i8, at(2) as u8, at(3) as i16);
                let room_702 = (self.game.id.stage, self.game.id.room) == (7, 2);
                if let Some(character) = self.task_character(task) {
                    crate::player2::parts_set(&mut character.work, part, case, value, room_702);
                } else if self.work_is_player(task) {
                    crate::player2::parts_set(
                        &mut self.game.re2.player,
                        part,
                        case,
                        value,
                        room_702,
                    );
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
                // `Plc_ret` clears the player's `Type` high nibble too (its
                // Id is under 0x10), `KIND_NOT_MOVED` among it.
                self.game.re2.player.kind = plc_ret_kind(0, self.game.re2.player.kind);
            }
            "plc_stop" if self.work_is_player(task) => {
                crate::player2::plc_stop_player(&mut self.game.re2.player);
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
                    // A partner's live `Type` is `kind`, an enemy's is
                    // `work.kind` (`sce_em_set`): `Plc_ret` clears +0x10f.
                    character.kind = plc_ret_kind(character.id, character.kind);
                    character.work.kind = plc_ret_kind(character.id, character.work.kind);
                }
            }
            "splc_sce" if self.task_character(task).is_some() => {
                if let Some(character) = self.task_character(task) {
                    splc_sce(character);
                }
                self.game.re2.set_partner_bit(false);
            }
            "splc_ret" if self.task_character(task).is_some() => {
                let life = self.game.re2.partner_life;
                if let Some(character) = self.task_character(task) {
                    splc_ret(character, life);
                }
                self.game.re2.set_partner_bit(true);
            }
            "heal_partner" => self.game.re2.heal_partner(),
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
            // `Member_set2` reads the variable as a signed word (`MOVSX`).
            "member_set2" => {
                let value = i32::from(self.var(at(1) as u8) as i16);
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
            // The PC executable has no rumble: both handlers (`0x4E9110`) only
            // step over their operands.
            "vib_set0" | "vib_set1" => {}
            // `0x4E84B0` keeps a level below 0x80 for the next line's start.
            "xa_vol" => {
                if let Ok(level) = u8::try_from(at(0))
                    && level < 0x80
                {
                    self.game.re2.xa_volume = level;
                }
            }
            "sce_item_lost" => {
                self.game.remove_item(at(0) as u8);
            }
            "movie_on" => self.movie_on(at(0) as u8),
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
        2..=5 => Some(i32::from(object.routine[usize::from(member) - 2])),
        MEMBER_ID => Some(i32::from(object.id)),
        MEMBER_TYPE => Some(i32::from(object.kind)),
        8 => Some(i32::from(object.at_obj_no)),
        MEMBER_AREA => Some(i32::from(object.at_sce_no)),
        // `Attribute`, an int of which `Obj_model_set` writes the low word.
        10 => Some(i32::from(object.attr as i16)),
        0x11 => Some(i32::from(object.floor)),
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
        2..=5 => object.routine[usize::from(member) - 2] = value as u8,
        MEMBER_ID => object.id = value as u8,
        MEMBER_TYPE => object.kind = value as u16,
        8 => object.at_obj_no = value as u8,
        MEMBER_AREA => object.at_sce_no = value as u8,
        10 => object.attr = value as u16,
        0x11 => object.floor = value as u8,
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
        // `Be_flg`'s low and high words, as `Load_member_branch` reads them.
        0 => Some(i32::from(character.be_flg as u16)),
        1 => Some(i32::from((character.be_flg >> 16) as u16)),
        MEMBER_ID => Some(i32::from(character.id)),
        MEMBER_AREA => Some(i32::from(character.area)),
        MEMBER_ANGLE => Some(i32::from(character.body.angle & 0x0FFF)),
        MEMBER_ZOMBIE_FLAGS => character
            .zombie
            .as_ref()
            .map(|zombie| i32::from(zombie.flags)),
        _ => crate::player2::member(&character.work, member),
    }
}

/// `Member_set_branch` on a character.
/// A character whose `Id` a script has made a zombie's. `Em_move` now gives it
/// to `Em_Zombie`, whose init runs in the body and with the animations it has:
/// the room loaded nothing new for it. Room 202 turns Marvin so.
fn become_zombie(character: &mut Character, slot: u8) {
    let id = character.id;
    if !(0x10..=0x21).contains(&id) || character.zombie.is_some() {
        return;
    }
    let (floor, model_type, dead) = character.em_set;
    let mut zombie = crate::zombie2::spawned(slot, floor, model_type);
    zombie.dead_flag = dead;
    character.zombie = Some(zombie);
    character.body.radius = 0x1C2;
}

fn set_character_member(character: &mut Character, member: u8, value: i32) {
    if let Some(axis) = usize::from(member)
        .checked_sub(MEMBER_POS_X)
        .filter(|axis| *axis < 3)
    {
        character.body.pos[axis] = value;
        return;
    }
    match member {
        // `Member_set_branch` writes a word of `Be_flg`, and leaves the other.
        0 => character.be_flg = character.be_flg & 0xFFFF_0000 | u32::from(value as u16),
        1 => character.be_flg = character.be_flg & 0xFFFF | u32::from(value as u16) << 16,
        MEMBER_ID => character.id = value as u8,
        MEMBER_AREA => character.area = value as u8,
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
    #[test]
    fn set_room_status_keep_drops_the_scene_and_voice_bits() {
        use super::{SET_ROOM_STATUS_KEEP, STATUS_SCENE, STATUS_VOICE};
        assert_eq!(SET_ROOM_STATUS_KEEP & (0x8000_0000 >> STATUS_SCENE), 0);
        assert_eq!(SET_ROOM_STATUS_KEEP & (0x8000_0000 >> STATUS_VOICE), 0);
    }

    use super::*;

    fn exec(game: &mut GameState, mnemonic: &str, operands: &[i64]) {
        let op = crate::scd2::opcode::by_mnemonic(mnemonic).unwrap();
        GameHost { game }.exec(0, op, operands);
    }

    #[test]
    fn movie_on_with_the_films_off_leaves_0x200_down_and_asks_for_two_held_frames() {
        let mut game = GameState::default();
        game.re2.set_flag(0, FG_SYSTEM_8000, 1);
        game.re2.sounds.push(SoundRequest::new(
            crate::se2::SoundId::decode(0x0200_0001).unwrap(),
            [0; 3],
        ));
        exec(&mut game, "movie_on", &[10]);
        assert!(!game.re2.flag(0, FG_SYSTEM_MOVIE));
        assert!(!game.re2.flag(0, FG_SYSTEM_8000));
        assert_eq!(game.re2.film, None);
        assert_eq!(game.re2.movie_set, Some(2));
        assert_eq!(game.re2.sound_close, Some(1));
        assert!(game.re2.sound_init);
    }

    #[test]
    fn movie_on_with_the_films_on_raises_0x200_and_queues_the_film() {
        let mut game = GameState::default();
        game.re2.films = true;
        exec(&mut game, "movie_on", &[10]);
        assert!(game.re2.flag(0, FG_SYSTEM_MOVIE));
        assert_eq!(game.re2.film, Some(10));
        assert_eq!(game.re2.sound_close, Some(0));
        assert!(!game.re2.sound_init);
    }

    #[test]
    fn room_200s_wait_loop_runs_once_with_the_films_off_and_holds_while_0x200_is_up() {
        let scripts = crate::scd2::akscr::compile(
            "#version 1\n#game re2\nproc main_00\n{\n    movie_on(5);\n    nop();\n    do\n    {\n        \
             evt_next();\n        nop();\n    }\n    while (ck(0, 22, 1));\n    set(4, 99, 1);\n    \
             evt_end(0);\n}\n",
        )
        .unwrap();
        for films in [false, true] {
            let mut game = GameState::default();
            game.re2.films = films;
            let mut vm = crate::scd2::vm::Vm::new(&scripts);
            vm.start_main();
            let mut tick = |game: &mut GameState| {
                vm.tick(&mut GameHost { game });
                game.re2.flag(4, 99)
            };
            assert!(!tick(&mut game), "the first tick waits at evt_next");
            if films {
                for _ in 0..4 {
                    assert!(!tick(&mut game), "the loop waits while the film plays");
                }
                game.re2.set_flag(0, FG_SYSTEM_MOVIE, 0);
            }
            assert!(tick(&mut game), "films {films}");
        }
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

    /// Room 206's event 2: `cut_chg(11)` then `se_on(2, 267, 1, ...)`. The
    /// sound is placed from cut 11, which `Cut_chg` made `scd_var_cut`.
    #[test]
    fn a_sound_after_a_scripts_cut_change_is_placed_from_the_new_cut() {
        let mut game = GameState::default();
        exec(&mut game, "se_on", &[2, 267, 1, 0, 0, 0]);
        exec(&mut game, "cut_chg", &[11]);
        exec(&mut game, "se_on", &[2, 267, 1, 0, 0, 0]);
        let cuts: Vec<_> = game.re2.sounds.iter().map(|sound| sound.cut).collect();
        assert_eq!(cuts, [None, Some(11)]);
    }

    /// `Snd_se_3D` places every sound from `scd_var_cut`, whoever plays it:
    /// an enemy's or a footstep after the scripts' `cut_chg` is placed from
    /// the new cut, and a script's `se_on` before it from the tick's.
    #[test]
    fn every_sound_after_a_scripts_cut_change_is_placed_from_the_new_cut() {
        let mut game = GameState::default();
        game.re2.var_cut = Some(3);
        exec(&mut game, "se_on", &[2, 267, 1, 0, 0, 0]);
        exec(&mut game, "cut_chg", &[11]);
        let id = game.re2.sounds[0].id;
        game.re2.sounds.push(SoundRequest::new(id, [0; 3]));
        let cuts: Vec<_> = game
            .re2
            .sounds
            .iter()
            .map(|sound| game.re2.placement_cut(sound.cut))
            .collect();
        assert_eq!(cuts, [Some(3), Some(11)], "the script's, then the enemy's");
        assert_eq!(game.re2.placement_cut(None), Some(11), "a footstep");
    }

    #[test]
    fn each_status_start_sets_the_status_screens_repeat() {
        use crate::game::pad_repeat::{STATUS_SETTING, TITLE_SETTING};
        let mut game = GameState::default();
        game.is_re2_room = true;
        assert_eq!(
            (game.pad_repeat.mask, game.pad_repeat.delays),
            TITLE_SETTING
        );
        // The status screen and the item box (`init_re2_status`).
        game.init_re2_status();
        assert_eq!(
            (game.pad_repeat.mask, game.pad_repeat.delays),
            STATUS_SETTING
        );
        // A get-item window, on its sixth tick, for an item or a file.
        game.pad_repeat.set(TITLE_SETTING);
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        for _ in 0..5 {
            game.tick_re2_item_get(crate::game::status2::Input::default());
            assert_eq!(game.pad_repeat.mask, TITLE_SETTING.0);
        }
        game.tick_re2_item_get(crate::game::status2::Input::default());
        assert_eq!(
            (game.pad_repeat.mask, game.pad_repeat.delays),
            STATUS_SETTING
        );
    }

    #[test]
    fn super_set_writes_the_parent_and_the_offsets() {
        use crate::objects::SuperParent;
        let mut game = GameState::default();
        game.objects.records = vec![crate::objects::ObjectRecord::default(); 10];
        exec(&mut game, "work_set", &[WK_OBJECT, 9]);
        let parent = |game: &GameState| game.objects.record(9).unwrap().super_parent;
        // Room 115's `main_1B`: the player's joint 1.
        exec(
            &mut game,
            "super_set",
            &[0, 64, 129, 161, 293, 349, 1024, -2752, 0],
        );
        let object = game.objects.record(9).unwrap();
        assert_eq!(object.super_parent, SuperParent::PlayerJoint(1));
        assert_eq!(object.pos, [161, 293, 349]);
        assert_eq!(object.rotation, [1024, -2752, 0]);
        // No joint: the player's own matrix.
        exec(&mut game, "super_set", &[0, 64, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(parent(&game), SuperParent::Player);
        // `main_18`'s second: enemy slot 0 (work 2), joint 14.
        exec(
            &mut game,
            "super_set",
            &[0, 130, 142, -1, 365, -40, 832, -2048, 60],
        );
        assert_eq!(parent(&game), SuperParent::WorkJoint(2, 14));
        assert_eq!(game.objects.record(9).unwrap().pos, [-1, 365, -40]);
        exec(&mut game, "super_set", &[0, 128, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(parent(&game), SuperParent::Work(0));
        exec(&mut game, "super_set", &[0, 0xC3, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(parent(&game), SuperParent::Object(3));
        exec(&mut game, "super_set", &[0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(parent(&game), SuperParent::World);
    }

    #[test]
    fn a_set_item_flag_drops_the_area_and_hides_the_object() {
        // Room 115's diary and medal with 8:20 and 8:104 up (`Item_aot_set`):
        // the area is inert, the object hidden, and `aot_on` does nothing.
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.objects.records = vec![crate::objects::ObjectRecord::default(); 10];
        game.re2.set_flag(ITEM_BANK, 104, 1);
        let ops = [2, 2, 49, 0, 0, -100, -100, 200, 200, 0x47, 1, 104, 3, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        assert_eq!(game.room_actions[2].unwrap().sce, SCE_AUTO);
        let object = game.objects.record(3).unwrap();
        assert_eq!((object.be_flg, object.flag), (0x8000_0000, 0));
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x47).unwrap(), &[2]);
        assert!(game.re2_item_get.is_none());
        assert!(!game.message.active);
    }

    #[test]
    fn cut_chg_raises_the_pending_load_unless_bit_7_is_set() {
        let mut game = GameState::default();
        exec(&mut game, "cut_chg", &[0]);
        assert!(game.re2.bg_load, "the cut already on screen loads again");
        assert!(!game.re2.cut_no_load);
        exec(&mut game, "cut_chg", &[0x81]);
        assert_eq!(game.camera.current_cut, 1);
        assert!(!game.re2.bg_load, "bit 7 drops the pending load");
        assert!(game.re2.cut_no_load);
        game.re2.bg_load = false;
        exec(&mut game, "cut_old", &[]);
        assert!(game.re2.bg_load, "`Cut_old_0` loads through `FUN_004e5020`");
    }

    #[test]
    fn a_setup_scripts_dest_starts_the_next_zombie_in_its_slot_and_room() {
        let mut game = GameState::default();
        game.id = crate::state::RoomId::parse("1020").unwrap();
        game.re2_enemy_dest = vec![(0x102, 2, [224, 28708]), (0x118, 2, [5, 6])];
        let zombie = [
            0, 2, 0x1F, 70, 128, 0, 7, 0, 54, 20000, 20000, 20000, 4056, 0, 0,
        ];
        let dest = |game: &GameState| game.re2.characters.0[2].as_ref().unwrap().work.dest;
        exec(&mut game, "sce_em_set", &zombie);
        assert_eq!(dest(&game), [224, 28708]);
        assert_eq!(game.re2_enemy_dest, [(0x118, 2, [5, 6])], "used once");
        exec(&mut game, "sce_em_set", &zombie);
        assert_eq!(dest(&game), [0, 0], "the room's next load starts it at 0");
    }

    #[test]
    fn a_setup_scripts_speed_and_vars_start_the_next_zombie_in_its_slot_and_room() {
        let mut game = GameState::default();
        game.id = crate::state::RoomId::parse("3030").unwrap();
        game.re2_enemy_speed = vec![(0x303, 3, [0, 8491, 9547]), (0x309, 3, [1, 2, 3])];
        game.re2_enemy_vars = vec![(0x303, 3, 1, 3335), (0x303, 3, 2, 264), (0x303, 2, 1, 7)];
        let embryo = [
            0, 3, 0x27, 3, 0, 3, 20, 0, 130, -23237, -5400, -23988, 1509, 0, 0,
        ];
        exec(&mut game, "sce_em_set", &embryo);
        let character = game.re2.characters.0[3].as_ref().unwrap();
        assert_eq!(character.work.spd, [0, 8491, 9547]);
        let vars = character.zombie.as_ref().unwrap().vars();
        assert_eq!(vars[1..3], [3335, 264]);
        assert_eq!(game.re2_enemy_speed, [(0x309, 3, [1, 2, 3])], "used once");
        assert_eq!(game.re2_enemy_vars, [(0x303, 2, 1, 7)], "only the slot's");
    }

    /// Room 205's slot 0: an `Em_2D` with `zombie` data, which `main_00`
    /// freezes through member 7 (`calc`'s 5 and 6 on the copy) and `main_03`
    /// lets go.
    #[test]
    fn sce_em_set_places_an_em2d_and_member_7_holds_it() {
        let mut game = GameState::default();
        let em2d = [
            0, 0, 45, 16, 0, 0, 17, 0, 255, -27150, -2580, -12350, 0, 0, 0,
        ];
        exec(&mut game, "sce_em_set", &em2d);
        exec(&mut game, "work_set", &[WK_ENEMY, 0]);
        exec(&mut game, "member_copy", &[16, 7]);
        let mut host = GameHost { game: &mut game };
        assert_eq!(host.var(16), 0x10);
        host.set_var(16, host.var(16) | 0x8000);
        exec(&mut game, "member_set2", &[7, 16]);
        let character = game.re2.characters.0[0].as_ref().unwrap();
        assert_eq!(character.id, crate::zombie2::EM2D_ID);
        assert!(character.zombie.is_some());
        assert_eq!(character.work.kind, 0x8010);
        assert_eq!(character.body.pos, [-27150, -2580, -12350]);
        let mut host = GameHost { game: &mut game };
        host.set_var(16, host.var(16) & 0x7FFF);
        exec(&mut game, "member_set2", &[7, 16]);
        let character = game.re2.characters.0[0].as_ref().unwrap();
        assert_eq!(character.work.kind, 0x0010);
        // No shot reaches it until its hits are ported; a zombie beside it is
        // still a target.
        assert!(crate::character2::targets(&game).is_empty());
        let zombie = [0, 1, 0x10, 0, 0, 0, 7, 0, 255, 0, 0, 0, 0, 0, 0];
        exec(&mut game, "sce_em_set", &zombie);
        let targets = crate::character2::targets(&game);
        assert_eq!(targets.iter().map(|t| t.slot).collect::<Vec<_>>(), [1]);
    }

    /// Room 207's `init_00` (scenario A): its four zombies, as the RDT's
    /// `sce_em_set` operands give them (slot, id, `Type`, floor, sound type,
    /// `Model_type`, dead flag, x, y, z, dir).
    const ROOM_207: [[i64; 15]; 4] = [
        [0, 0, 0x13, 6, 0, 0, 44, 0, 71, -25621, 0, -20311, 213, 0, 0],
        [
            0, 1, 0x1F, 0, 0, 0, 42, 1, 72, -21898, 0, -12962, 3049, 0, 0,
        ],
        [
            0, 2, 0x1F, 6, 0, 0, 42, 1, 115, -17938, 0, -10646, 1597, 0, 0,
        ],
        [
            0, 3, 0x1F, 6, 0, 0, 42, 1, 116, -25817, 0, -12071, 4491, 0, 0,
        ],
    ];

    /// Room 207's sound types are 44 then 42. (42, 44) is bank 0x38 in
    /// `Sce_se_set`'s table, so the pair is swapped, and slot 0 (type 44,
    /// `Se_tmp0`'s low byte) plays the bank's second half. `Sce_em_set`
    /// keeps slot 3's dir as given, past 4095, and counts four enemies.
    #[test]
    fn room_207s_zombies_pick_the_swapped_44_42_bank() {
        let mut game = GameState::default();
        for zombie in ROOM_207 {
            exec(&mut game, "sce_em_set", &zombie);
        }
        game.re2.sce_se_set();
        assert_eq!(game.re2.enemy_bank, Some(0x38));
        assert_eq!(game.re2.enemies, 4);
        let characters = &game.re2.characters.0;
        let second_half: Vec<bool> = (0..4)
            .map(|slot| characters[slot].as_ref().unwrap().be_flg & 0x2000 != 0)
            .collect();
        assert_eq!(second_half, [true, false, false, false]);
        assert_eq!(characters[3].as_ref().unwrap().body.angle, 4491);
    }

    /// The `zd` field: each zombie's slot and `Dest`, which `Sce_em_set`
    /// doesn't write.
    #[test]
    fn zd_prints_each_zombies_dest() {
        let mut game = GameState::default();
        assert_eq!(game.re2.zombie_dest_trace(), "-");
        for zombie in &ROOM_207[..2] {
            exec(&mut game, "sce_em_set", zombie);
        }
        game.re2.characters.0[0].as_mut().unwrap().work.dest = [100, -200];
        assert_eq!(game.re2.zombie_dest_trace(), "0:100,-200;1:0,0");
    }

    /// Room 204's `main_02` puts the scene's licker off the stage with
    /// `Type` 0x4510, and `main_09`, on the tick after its `movie_on`, moves
    /// it over the window and gives it the word (`Pos_set` writes only the
    /// place; member 23 is `Sce_free0`).
    #[test]
    fn room_204s_scene_moves_the_offstage_licker_and_gives_it_the_word() {
        let mut game = GameState::default();
        let licker = [
            0, 1, 0x22, 16, 69, 0, 14, 0, 61, 32000, -5700, 32000, 1024, 0, 0,
        ];
        exec(&mut game, "sce_em_set", &licker);
        let character = game.re2.characters.0[1].as_ref().unwrap();
        assert_eq!(character.work.kind, 0x4510);
        let angle = character.body.angle;
        exec(&mut game, "work_set", &[WK_ENEMY, 1]);
        exec(&mut game, "pos_set", &[0, -14869, -5700, -3479]);
        exec(&mut game, "member_set", &[23, 1]);
        let character = game.re2.characters.0[1].as_ref().unwrap();
        assert_eq!(character.body.pos, [-14869, -5700, -3479]);
        assert_eq!(character.work.sce_free[0], 1);
        assert_eq!(character.body.angle, angle);
    }

    /// `main_02`'s other branch, once 4:86 is set: a plain ceiling licker.
    #[test]
    fn room_204s_revisit_licker_is_a_plain_ceiling_licker() {
        let mut game = GameState::default();
        let licker = [
            0, 1, 0x22, 18, 5, 0, 14, 0, 61, -16169, -5400, -6579, 1024, 0, 0,
        ];
        exec(&mut game, "sce_em_set", &licker);
        let character = game.re2.characters.0[1].as_ref().unwrap();
        assert_eq!(character.work.kind, 0x0512);
    }

    /// Room 40B's `init_00` sets 24 cockroaches (id 0x29) at the top of its
    /// pipe: each keeps its slot as an enemy with a script, and each is a
    /// head the player can turn to.
    #[test]
    fn room_40bs_cockroaches_keep_their_slots() {
        let mut game = GameState::default();
        game.id = crate::state::RoomId::parse("40B0").unwrap();
        for slot in 0..24 {
            exec(
                &mut game,
                "sce_em_set",
                &[
                    0, slot, 0x29, 0, 0, 0, 15, 0, 255, -15372, -3600, -25133, 1024, 0, 0,
                ],
            );
        }
        for slot in 0..24 {
            let roach = game.re2.characters.0[slot].as_ref().unwrap();
            assert_eq!(roach.id, crate::zombie2::ROACH_ID);
            assert!(roach.zombie.is_some(), "slot {slot}");
        }
        assert!(game.re2.characters.0[24].is_none());
        assert_eq!(game.re2.enemies, 24);
        let targets = game.re2.neck_targets();
        assert_eq!(targets.len(), 24);
        let last = targets.iter().map(|t| t.index).max().unwrap();
        assert_eq!(last, 25);
        assert!(last < crate::player2::NECK_WORKS);
    }

    #[test]
    fn neck_targets_give_a_script_character_no_life_and_each_its_facing() {
        let mut game = GameState::default();
        spawn_character(&mut game, 0x48, 255);
        exec(
            &mut game,
            "sce_em_set",
            &[0, 1, 0x1F, 64, 0, 0, 5, 1, 62, 1000, 0, 2000, 1240, 0, 0],
        );
        let targets = game.re2.neck_targets();
        let kendo = targets.iter().find(|target| target.index == 2).unwrap();
        assert_eq!((kendo.life, kendo.angle), (-1, 3950));
        let zombie = targets.iter().find(|target| target.index == 3).unwrap();
        let life = game.re2.characters.0[1]
            .as_ref()
            .unwrap()
            .zombie
            .as_ref()
            .unwrap()
            .life;
        assert_eq!((zombie.life, zombie.angle), (life, 1240));
        assert!(life >= 0);
    }

    fn licker_work(game: &GameState) -> &crate::player2::Work {
        &game.re2.characters.0[0].as_ref().unwrap().work
    }

    #[test]
    fn parts_set_writes_a_parts_colour() {
        let mut game = GameState::default();
        game.id = crate::state::RoomId::parse("2010").unwrap();
        spawn_character(&mut game, 0x22, 24);
        exec(&mut game, "parts_set", &[0, 3, 0, 0x7070]);
        assert_eq!(
            licker_work(&game).part_rgb[3],
            0x0080_7070,
            "case 0 keeps blue"
        );
        exec(&mut game, "parts_set", &[0, 3, 1, 0x70]);
        let work = licker_work(&game);
        assert_eq!(work.part_rgb[3], 0x0070_7070);
        assert!(
            work.part_rgb
                .iter()
                .enumerate()
                .all(|(part, &rgb)| part == 3 || rgb == crate::player2::DEFAULT_PART_RGB)
        );
    }

    #[test]
    fn parts_set_in_room_702_writes_its_own_colour() {
        let mut game = GameState::default();
        game.id = crate::state::RoomId::parse("7020").unwrap();
        spawn_character(&mut game, 0x22, 24);
        exec(&mut game, "parts_set", &[0, 1, 0, 0x7070]);
        exec(&mut game, "parts_set", &[0, 1, 1, 0x70]);
        assert_eq!(licker_work(&game).part_rgb[1], 0x0005_0505);
    }

    #[test]
    fn parts_set_ignores_parts_out_of_range_and_cases_3_to_6() {
        let mut game = GameState::default();
        game.id = crate::state::RoomId::parse("2010").unwrap();
        spawn_character(&mut game, 0x22, 24);
        let before = licker_work(&game).clone();
        exec(&mut game, "parts_set", &[0, 0xFF, 2, 0]);
        exec(&mut game, "parts_set", &[0, 40, 0, 0]);
        for case in 3..=6 {
            exec(&mut game, "parts_set", &[0, 2, case, 0]);
        }
        let work = licker_work(&game);
        assert_eq!(work.hidden_parts, before.hidden_parts);
        assert_eq!(work.part_rgb, before.part_rgb);
        exec(&mut game, "parts_set", &[0, 2, 2, 0]);
        assert_eq!(licker_work(&game).hidden_parts, 1 << 2);
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
    fn obj_model_set_keeps_what_the_original_leaves() {
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let call = [
            1, 0, 0, 0, 0, 0x0002, 0, 0x85, 0, 100, -3600, 300, 0, 0, 0, 0, -900, 0, 432, 132, 3,
            176, 1,
        ];
        exec(&mut game, "obj_model_set", &call);
        let object = game.objects.record_mut(1).unwrap();
        assert_eq!(object.floor, 2, "nFloor, from the word's low byte");
        assert_eq!(object.at, [100, -4500, 300], "Oma_set_ofs");
        object.old = [1, 2, 3];
        object.old2 = [4, 5, 6];
        object.push_cnt = 7;
        object.at_sce_no = 3;
        object.be_flg = 0;
        exec(&mut game, "obj_model_set", &call);
        let object = game.objects.record_mut(1).unwrap();
        assert_eq!(object.be_flg, 0x85);
        assert_eq!(object.old, [1, 2, 3]);
        assert_eq!(object.old2, [4, 5, 6]);
        assert_eq!(object.push_cnt, 7);
        assert_eq!(object.at_sce_no, 3);
    }

    #[test]
    fn room_303s_first_four_objects_sit_0x28_to_minus_x() {
        let mut game = GameState::default();
        game.id.stage = 2;
        game.id.room = 3;
        game.objects = crate::objects::ObjectTable::new(5);
        for slot in [3, 4] {
            let call = [slot, 0, 0, 0, 0, 0, 0, 0x10, 0, 100, -200, 300];
            exec(&mut game, "obj_model_set", &call);
        }
        assert_eq!(game.objects.record_mut(3).unwrap().pos, [60, -200, 300]);
        assert_eq!(game.objects.record_mut(4).unwrap().pos, [100, -200, 300]);
    }

    /// Room 10C's east plate: area 3, objects only (`sat` 0x44), on floor 2,
    /// with a statue placed by `obj_model_set` at `pos` on `floor`.
    fn plate_hall(pos: [i64; 3], floor: i64) -> GameState {
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.objects = crate::objects::ObjectTable::new(3);
        let statue = [
            1, 0, 0, 0, 0, floor, 0, 0x85, 0, pos[0], pos[1], pos[2], 0, 0, 0, 0, -900, 0, 432,
            132, 3, 176, 1,
        ];
        exec(&mut game, "obj_model_set", &statue);
        let plate = [
            3,
            i64::from(SCE_WATER),
            0x44,
            2,
            0,
            -17250,
            -12700,
            600,
            400,
        ];
        exec(&mut game, "aot_set", &plate);
        game.objects.record_mut(1).unwrap().at_sce_no = NO_AREA;
        game
    }

    /// `member_cmp(0, 9, 0, 3)` on object 1.
    fn on_plate_3(game: &mut GameState) -> bool {
        exec(game, "work_set", &[WK_OBJECT, 1]);
        let op = crate::scd2::opcode::by_mnemonic("member_cmp").unwrap();
        GameHost { game }.exec(0, op, &[0, 9, 0, 3]) != Step::False
    }

    #[test]
    fn the_object_pass_uses_the_objects_floor() {
        // The player stands on floor 0, far from the plate.
        let mut game = plate_hall([-17000, -3600, -12500], 2);
        game.interact([0, 0, 0], 0, false);
        assert_eq!(game.objects.record(1).unwrap().at_sce_no, 3);
        assert_eq!(game.re2.vars[VAR_BODY_AREA], 3);

        // A statue on floor 0 at the same place misses a floor-2 plate, with
        // the player on floor 2.
        let mut game = plate_hall([-17000, -3600, -12500], 0);
        game.interact([-20000, -3600, -10000], 0, false);
        assert_eq!(game.objects.record(1).unwrap().at_sce_no, NO_AREA);
    }

    #[test]
    fn member_9_of_an_object_is_its_last_area() {
        let mut game = plate_hall([-17000, -3600, -12500], 2);
        assert!(!on_plate_3(&mut game));
        game.interact([0, 0, 0], 0, false);
        assert!(on_plate_3(&mut game));
        // Off the plate, it keeps the area until another hit.
        game.objects.record_mut(1).unwrap().pos = [-19000, -3600, -12500];
        game.interact([0, 0, 0], 0, false);
        assert!(on_plate_3(&mut game));
    }

    /// Room 205's `main_00` hangs object 3 on object 2 (`obj_model_set`
    /// selector 0xC2, 1000 below it), and `main_07` gives it back to the
    /// world with `super_reset` and lets it fall with `add_speed`, with no
    /// `pos_set` between: the fall starts where the object was drawn.
    #[test]
    fn super_reset_leaves_the_object_where_it_was_drawn() {
        use crate::objects::SuperParent;
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(8);
        let parent = [
            2, 0, 0, 0, 0, 0, 0, 10, 16, -17300, -3400, -7000, 1024, -256, 1024, 0, 0, 0, 0, 0, 0,
            0, 0,
        ];
        let child = [
            3, 0, 0, 0, 0, 49664, 0, 10, 16, 0, 1000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        exec(&mut game, "obj_model_set", &parent);
        exec(&mut game, "obj_model_set", &child);
        game.om_trans_old(([0; 3], 0));
        let drawn = crate::objects::world_matrix(&game.objects, 3, [0; 3], 0).t;
        assert_eq!(game.objects.record(3).unwrap().world.t, drawn);
        assert_ne!(drawn, [0, 1000, 0]);

        exec(&mut game, "work_set", &[WK_OBJECT, 3]);
        exec(&mut game, "super_reset", &[0, 1024, -256, 1024]);
        exec(&mut game, "speed_set", &[1, 50]);
        exec(&mut game, "add_speed", &[]);
        let object = game.objects.record(3).unwrap();
        assert_eq!(object.super_parent, SuperParent::World);
        assert_eq!(object.parent, 0xFF);
        assert_eq!(object.rotation, [1024, -256, 1024]);
        assert_eq!(object.pos, [drawn[0], drawn[1] + 50, drawn[2]]);
    }

    /// Hung on the player's joint with `super_set` (mode 0x40, joint 0x8A),
    /// an object is drawn on that joint's matrix, and `super_reset` puts it
    /// down there; an `Id` 2 box keeps `Om_trans`' 900 lift.
    #[test]
    fn super_reset_takes_the_position_om_trans_composed_on_a_joint() {
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let mut joints = vec![crate::anim::Mat4x3::default(); 15];
        joints[10] = crate::anim::entity_matrix([1000, -900, 2000], 1024);
        game.joint_worlds[0] = joints;
        for (id, lift) in [(0, 0), (2, 900)] {
            let call = [
                4, id, 0, 0, 0, 0, 0, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ];
            // A second `obj_model_set` on a slot in play clears it.
            game.objects.record_mut(4).unwrap().be_flg = 0;
            exec(&mut game, "obj_model_set", &call);
            exec(&mut game, "work_set", &[WK_OBJECT, 4]);
            exec(&mut game, "super_set", &[0, 64, 138, 0, 600, -50, 0, 0, 0]);
            // Before a frame is drawn, it is where `Obj_model_set` put it.
            exec(&mut game, "super_reset", &[0, 0, 0, 0]);
            assert_eq!(game.objects.record(4).unwrap().pos, [0, 0, 0]);
            exec(&mut game, "super_set", &[0, 64, 138, 0, 600, -50, 0, 0, 0]);
            game.om_trans_old(([0; 3], 0));
            exec(&mut game, "super_reset", &[0, 5, 6, 7]);
            let local = crate::anim::Mat4x3 {
                r: [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]],
                t: [0, 600, -50],
            };
            let mut expected = crate::anim::compose(&game.joint_worlds[0][10], &local).t;
            expected[1] -= lift;
            let object = game.objects.record(4).unwrap();
            assert_eq!(object.pos, expected, "id {id}");
            assert!(!object.hangs_on_a_character());
            assert_eq!(object.rotation, [5, 6, 7]);
        }
    }

    #[test]
    fn obj_model_set_and_super_set_write_the_one_parent() {
        use crate::objects::SuperParent;
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let call = |selector: i64| {
            [
                4,
                2,
                0,
                0,
                0,
                selector << 8,
                0,
                0x10,
                0,
                100,
                -200,
                300,
                1,
                2,
                3,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
        };
        // `Obj_model_set` switches on `selector & 0xC0` too: 0x80 | n is
        // work n (0 the player, 1 the second player, 2 + n an enemy slot).
        for (selector, parent, draw) in [
            (0x00, SuperParent::World, 0xFF),
            (0x40, SuperParent::Player, 0xFE),
            (0x82, SuperParent::Work(2), 0xFF),
            (0xC3, SuperParent::Object(3), 3),
        ] {
            game.objects.record_mut(4).unwrap().be_flg = 0;
            exec(&mut game, "obj_model_set", &call(selector));
            let object = game.objects.record(4).unwrap();
            assert_eq!(object.super_parent, parent, "{selector:#x}");
            assert_eq!(object.parent, draw, "{selector:#x}");
        }
        // `Super_set` writes the same pointer, and the selector with it.
        exec(&mut game, "work_set", &[WK_OBJECT, 4]);
        exec(&mut game, "super_set", &[0, 130, 142, 0, 220, 10, 0, 0, 0]);
        let object = game.objects.record(4).unwrap();
        assert_eq!(object.super_parent, SuperParent::WorkJoint(2, 14));
        assert_eq!(object.parent, 0xFF);
        assert!(object.hangs_on_a_character());
        assert_eq!(object.pos, [0, 220, 10]);
        exec(&mut game, "super_set", &[0, 0, 0, 5, 6, 7, 0, 0, 0]);
        let object = game.objects.record(4).unwrap();
        assert!(!object.hangs_on_a_character());
        assert_eq!(object.parent, 0xFF);
    }

    #[test]
    fn se_on_of_an_object_sounds_at_the_object() {
        let mut game = GameState::default();
        game.objects = crate::objects::ObjectTable::new(5);
        let call = [
            2, 2, 0, 0, 0, 0x4000, 0, 0x10, 0, 100, -200, 300, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        exec(&mut game, "obj_model_set", &call);
        // Room 102's gate: room slot 0x0a in 3D, at object 2 and 10 units up.
        exec(&mut game, "se_on", &[2, 0x10A, 0x204, 0, -10, 0]);
        let sound = game.re2.sounds.last().unwrap();
        assert_eq!(sound.id, crate::se2::SoundId::decode(0x020A_0001).unwrap());
        assert_eq!(sound.pos, [100, -210, 300]);
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

    /// Room 202's scene with Marvin reads the player's `Type & 0xFF` to choose
    /// whether to put the gun away.
    #[test]
    fn the_players_type_holds_the_weapon_in_hand() {
        let mut game = GameState::default();
        game.equipped_weapon_id = 2;
        game.re2.player.kind = crate::player2::KIND_NOT_MOVED;
        let mut host = GameHost { game: &mut game };
        exec(host.game, "work_set", &[WK_PLAYER, 0]);
        assert_eq!(
            host.member(0, MEMBER_TYPE),
            Some(i32::from(crate::player2::KIND_NOT_MOVED) | 2)
        );
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
    fn xa_vol_keeps_a_level_below_0x80_for_the_next_line() {
        let mut game = GameState::default();
        assert_eq!(game.re2.xa_volume, XA_VOLUME);
        exec(&mut game, "xa_vol", &[100]);
        assert_eq!(game.re2.xa_volume, 100);
        exec(&mut game, "xa_vol", &[0x80]);
        assert_eq!(game.re2.xa_volume, 100);
        game.re2.reset_xa_volume();
        assert_eq!(game.re2.xa_volume, 120);
        // `Snd_xa_vol`'s curve: two lines meeting at 0x20, clamped to full.
        assert_eq!(xa_millibels(120), -126);
        assert_eq!(xa_millibels(100), -486);
        assert_eq!(xa_millibels(127), 0);
        assert_eq!(xa_millibels(0x20), -1710);
        assert_eq!(xa_millibels(0x1F), -1971);
        assert_eq!(xa_millibels(0), -10_000);
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
        // An enemy that isn't modelled (`Em_2B`).
        spawn_character(&mut game, 0x2B, 255);
        assert!(game.re2.characters.0.is_empty());
        assert!(matches!(game.re2.work[0], Work::Other));
        // One that isn't modelled is there as soon as it's told to go.
        exec(&mut game, "plc_dest", &[0, 4, 33, 0, 0]);
        assert!(!game.re2.flag(ROOM_BANK, 33));
        game.re2.sce_work_clr();
        assert!(game.re2.flag(ROOM_BANK, 33));
    }

    #[test]
    fn a_walk_s_end_lasts_one_pass_of_the_scripts() {
        let mut state = Re2State::default();
        state.set_flag(ROOM_BANK, 0, 1);
        state.raise_room_flag(33);
        state.sce_work_clr();
        assert!(!state.flag(ROOM_BANK, 33));
        assert!(state.flag(ROOM_BANK, 0));
    }

    #[test]
    fn a_room_s_set_up_clears_the_room_bank_s_first_word() {
        let mut state = Re2State::default();
        state.set_flag(ROOM_BANK, 2, 1);
        state.set_flag(ROOM_BANK, 31, 1);
        state.set_flag(ROOM_BANK, 40, 1);
        state.reset_mask_groups();
        assert!(!state.flag(ROOM_BANK, 2));
        assert!(!state.flag(ROOM_BANK, 31));
        // The second word is `fg_tick`'s, which only the pass of the scripts clears.
        assert!(state.flag(ROOM_BANK, 40));
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
    fn a_door_the_scripts_open_stops_every_character_for_the_tick() {
        let mut state = Re2State::default();
        assert!(!state.character_stopped(0));
        assert!(!state.character_stopped(SPLAYER_SLOT));
        // `Sce_Door` has raised `fg_stop` before the original's characters
        // run on that tick: the enemies and `splayer_work` both sit it out.
        state.script_door = true;
        assert!(state.character_stopped(0));
        assert!(state.character_stopped(5));
        assert!(state.character_stopped(SPLAYER_SLOT));
    }

    #[test]
    fn the_tick_after_a_doors_hand_back_masks_the_pad_as_a_stop() {
        const CONFIRM: u16 = 0x1000;
        const FORWARD: u16 = 0x0001;
        const RUN: u16 = 0x0200;
        // The original's `fg_stop` is `0xEF000000` at that tick's `Pad_set`.
        let mut state = Re2State {
            door_key_stop: true,
            ..Default::default()
        };
        assert!(state.input_stopped());
        state.set_key(CONFIRM | FORWARD | RUN);
        assert_eq!(
            state.key, CONFIRM,
            "a run held through the door waits a tick"
        );
        state.door_key_stop = false;
        state.set_key(CONFIRM | FORWARD | RUN);
        assert_eq!(state.key, CONFIRM | FORWARD | RUN);
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

    /// Room 103's area 3 (`aot_set 3, 3, 66, 3, ...`, the enemy bit and the
    /// body probe): `Sce_at_check`'s enemy pass puts the area in the zombie's
    /// `+0xB`, which the room's script reads as member 9, and in the script's
    /// body-area variable. The scripts' pass ends with `Sce_work_clr_at`, which
    /// clears it, so once the zombie has left the next probe leaves none.
    #[test]
    fn an_enemy_area_is_the_enemys_member_nine_until_the_scripts_pass() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        exec(
            &mut game,
            "sce_em_set",
            &[
                0, 1, 31, 6, 16, 3, 2, 0, 255, -24137, -5400, -25367, 3061, 0, 0,
            ],
        );
        exec(&mut game, "work_set", &[WK_ENEMY, 1]);
        let area = |game: &mut GameState| GameHost { game }.member(0, MEMBER_AREA);
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
        let ops = [3, 3, 66, 3, 0, -26066, -23500, 2450, 1570, 0, 0, 0, 0, 0, 0];
        GameHost { game: &mut game }.exec(0, crate::scd2::opcode::lookup(0x2C).unwrap(), &ops);
        game.interact([0, 0, 0], 0, false);
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
        game.re2.characters.0[1].as_mut().unwrap().body.pos = [-24330, -5400, -23367];
        game.interact([0, 0, 0], 0, false);
        assert_eq!(area(&mut game), Some(3));
        assert_eq!(game.re2.vars[VAR_BODY_AREA], 3);
        game.re2.sce_work_clr_at();
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
        game.interact([0, 0, 0], 0, false);
        assert_eq!(area(&mut game), Some(3));
        game.re2.sce_work_clr_at();
        game.re2.characters.0[1].as_mut().unwrap().body.pos = [-10000, -5400, -23367];
        game.interact([0, 0, 0], 0, false);
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
    }

    #[test]
    fn sce_work_clr_clears_the_pass_s_script_work() {
        let mut state = Re2State::default();
        state.banks[usize::from(FG_USE_BANK)][..5].fill(0xFFFF_FFFF);
        state.banks[usize::from(ROOM_BANK)][1] = 0xFFFF_FFFF;
        state.vars[..5].fill(7);
        state.sce_work_clr();
        assert_eq!(state.banks[usize::from(FG_USE_BANK)][..4], [0; 4]);
        assert_eq!(state.banks[usize::from(FG_USE_BANK)][4], 0xFFFF_FFFF);
        assert_eq!(state.banks[usize::from(ROOM_BANK)][1], 0);
        assert_eq!(state.vars[..4], [0xFFFF; 4]);
        assert_eq!(state.vars[4], 7);
    }

    /// Room 200's statue: the Unicorn Medal's USE area (13, kind 6, `sat`
    /// 33, data `10, 71, 1`) and, for the gate, a passive event area (3) in
    /// the same box, with the player where `statue_200` stands.
    fn statue_200() -> GameState {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let aot_set = crate::scd2::opcode::lookup(0x2C).unwrap();
        let mut host = GameHost { game: &mut game };
        let use_area = [
            13, 6, 33, 0, 0, -15948, -9485, 4460, 2310, 10, 0, 71, 0, 1, 0,
        ];
        host.exec(0, aot_set, &use_area);
        let event = [3, 5, 33, 0, 0, -15948, -9485, 4460, 2310, 0, 0, 0, 3, 0, 0];
        host.exec(0, aot_set, &event);
        game
    }

    const STATUE_POS: [i32; 3] = [-13700, 0, -9900];
    const STATUE_ANGLE: u16 = 3072;
    const MEDAL: u8 = 71;

    #[test]
    fn the_status_open_s_area_pass_raises_fg_use_and_starts_nothing() {
        let mut game = statue_200();
        game.interact(STATUE_POS, STATUE_ANGLE, false);
        assert!(game.re2.flag(FG_USE_BANK, MEDAL));
        assert_eq!(game.re2.pending_events, [(0, 3)]);
        // START's tick: the scheduler's pass ends in `Sce_work_clr`, and no
        // `Sce_at` follows it.
        game.re2.pending_events.clear();
        game.re2.sce_work_clr();
        assert!(!game.re2.flag(FG_USE_BANK, MEDAL));
        assert_eq!(
            game.re2.vars[VAR_FORWARD_AREA], 0xFFFF,
            "no area until a hit"
        );
        // The open's room ticks: `Sce_at` with the scheduler off.
        game.re2_stopped_area_pass(STATUE_POS, STATUE_ANGLE);
        assert!(game.re2.flag(FG_USE_BANK, MEDAL));
        assert_eq!(game.re2.vars[VAR_FORWARD_AREA], 13, "the last hit");
        assert!(
            game.re2.pending_events.is_empty(),
            "`Sce_Event` gates itself"
        );
        assert!(!game.re2.areas_stopped);
        game.interact(STATUE_POS, STATUE_ANGLE, false);
        assert_eq!(game.re2.pending_events, [(0, 3)], "play's pass starts it");
    }

    /// Only `Sce_Event` tests the scheduler's stop: a `Sce_Windows` area
    /// under the player acts on the status open's room ticks as in play.
    #[test]
    fn the_status_open_s_area_pass_gates_only_the_events() {
        let mut game = statue_200();
        let windows = [
            14, 14, 33, 0, 0, -15948, -9485, 4460, 2310, 0, 0, 15, 0, 0, 0,
        ];
        GameHost { game: &mut game }.exec(0, crate::scd2::opcode::lookup(0x2C).unwrap(), &windows);
        game.re2_stopped_area_pass(STATUE_POS, STATUE_ANGLE);
        assert!(game.re2.pending_events.is_empty());
        assert!(game.re2_computer_request.is_some(), "`Sce_Windows` acts");
    }

    /// The passive pass records a kind-0 area (`Sce_Auto`) in `At_sce_no`
    /// too: `Sce_at_check` writes it before `Nothing` runs.
    #[test]
    fn the_passive_pass_records_a_kind_0_area_in_member_9() {
        let mut game = statue_200();
        let auto = [15, 0, 33, 0, 0, -15948, -9485, 4460, 2310, 0, 0, 0, 0, 0, 0];
        GameHost { game: &mut game }.exec(0, crate::scd2::opcode::lookup(0x2C).unwrap(), &auto);
        game.interact(STATUE_POS, STATUE_ANGLE, false);
        assert_eq!(game.re2.player.at_sce, 15);
        assert_eq!(
            game.re2.vars[VAR_FORWARD_AREA], 13,
            "`Nothing` keeps the variables"
        );
    }

    #[test]
    fn the_players_member_9_is_the_last_area_its_probe_was_in() {
        let mut game = statue_200();
        // Area 10, the statue's press-only event (`sat` 49), in the same box.
        let press = [10, 5, 49, 0, 0, -15948, -9485, 4460, 2310, 0, 0, 0, 7, 0, 0];
        GameHost { game: &mut game }.exec(0, crate::scd2::opcode::lookup(0x2C).unwrap(), &press);
        exec(&mut game, "work_set", &[WK_PLAYER, 0]);
        let area = |game: &mut GameState| GameHost { game }.member(0, MEMBER_AREA);
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
        // The press pass stops at the statue's event, then the passive pass
        // runs every area the probe is in and the last one wins.
        game.check_action_object(STATUE_POS, STATUE_ANGLE);
        assert_eq!(area(&mut game), Some(10));
        game.interact(STATUE_POS, STATUE_ANGLE, false);
        assert_eq!(area(&mut game), Some(13));
        game.re2.sce_work_clr_at();
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
        // Out of every area, nothing records one.
        game.interact([0, 0, 10000], STATUE_ANGLE, false);
        assert_eq!(area(&mut game), Some(i32::from(NO_AREA)));
        assert!(game.re2.script_work_trace().ends_with(" atsce=ff"));
    }

    #[test]
    fn the_used_item_lasts_the_first_pass_after_the_close() {
        let scripts = crate::scd2::akscr::compile(
            "#version 1\n#game re2\nproc main_00\n{\n    do\n    {\n        evt_next();\n        \
             if (cmp(0, 2, 0, 71))\n        {\n            set(4, 72, 1);\n        }\n    }\n    \
             while (ck(4, 73, 0));\n    evt_end(0);\n}\n",
        )
        .unwrap();
        let mut game = statue_200();
        let mut vm = crate::scd2::vm::Vm::new(&scripts);
        vm.start_main();
        let mut pass = |game: &mut GameState| {
            vm.tick(&mut GameHost { game: &mut *game });
            game.re2.sce_work_clr();
            game.re2.flag(4, 72)
        };
        assert!(!pass(&mut game));
        // USE's step 3, while the status is up and the scheduler off.
        game.re2.vars[VAR_USED_ITEM] = u16::from(MEDAL);
        assert!(pass(&mut game), "the first pass after the close reads it");
        assert_eq!(game.re2.vars[VAR_USED_ITEM], 0xFFFF);
        game.re2.set_flag(4, 72, 0);
        assert!(!pass(&mut game), "and only that pass");
    }

    /// Room 302's water area (`aot_set 2, 7, 79, 0, ...`): `Sce_Water` gives
    /// the player its level while inside, and `Sce_at` clears it outside.
    #[test]
    fn a_water_area_sets_the_players_water_while_inside() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [
            2, 7, 79, 0, 0, -20592, -20566, 18000, 29370, 76, 255, 0, 0, 0, 0,
        ];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x2C).unwrap(), &ops);
        game.interact([-13500, 0, -8500], 2048, false);
        assert_eq!(game.re2.player.water, -180);
        game.interact([0, 0, 10000], 2048, false);
        assert_eq!(game.re2.player.water, 0);
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
        take_through_the_window(&mut game, false);
        assert_eq!(game.item_count(0x14), 15);
        assert!(game.re2.flag(ITEM_BANK, 3));
        assert_eq!(game.room_actions[8].unwrap().handler, 0);
        // The room's next init sees the flag.
        let mut host = GameHost { game: &mut game };
        host.exec(0, item, &ops);
        assert_eq!(game.room_actions[8].unwrap().handler, 0);
    }

    #[test]
    fn the_difficulty_rewrites_the_item_counts() {
        use crate::zombie2::Difficulty;
        let hard = Difficulty {
            hard: true,
            uncensored: false,
        };
        let uncensored = Difficulty {
            hard: false,
            uncensored: true,
        };
        assert_eq!(sce_item_count(0x14, 15, hard), 10);
        assert_eq!(sce_item_count(0x14, 30, hard), 20);
        assert_eq!(sce_item_count(0x14, 10, hard), 20);
        assert_eq!(sce_item_count(0x15, 7, hard), 5);
        assert_eq!(sce_item_count(0x16, 8, hard), 6);
        for item in 0x18..=0x1A {
            assert_eq!(sce_item_count(item, 9, hard), 4);
        }
        assert_eq!(sce_item_count(0x1D, 18, hard), 12);
        for item in [0x17, 0x1B, 0x1C, 0x1E, 0x26] {
            assert_eq!(sce_item_count(item, 3, hard), 3);
        }
        assert_eq!(sce_item_count(0x1E, 3, uncensored), 2);
        assert_eq!(sce_item_count(0x14, 15, uncensored), 15);
        for item in 0x14..=0x1F {
            assert_eq!(sce_item_count(item, 15, Difficulty::default()), 15);
            assert_eq!(st_get_item_count(item, 15, Difficulty::default()), 15);
        }
        // The take's pass is the table alone: no ribbons, and 10 becomes 20.
        assert_eq!(st_get_item_count(0x1E, 3, uncensored), 3);
        assert_eq!(st_get_item_count(0x14, 10, hard), 20);
        assert_eq!(st_get_item_count(0x15, 5, hard), 5);
    }

    /// A HARD pick-up of a 15-round area: `Sce_Item` leaves the area at 10
    /// for the check, and the take rewrites it again, to the 20 the
    /// inventory gets.
    #[test]
    fn a_hard_pick_up_rewrites_its_area_twice() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.re2.start_new_game(false, true, false);
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 15, 3, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        assert_eq!(game.room_actions[8].unwrap().params[1], 10);
        take_through_the_window(&mut game, false);
        assert_eq!(game.item_count(0x14), 20);
        assert_eq!(game.room_actions[8].unwrap().params[1], 20);
    }

    /// A refused HARD pick-up leaves the area at the check's count, which
    /// the next check rewrites again: 15, 10, then 20.
    #[test]
    fn a_refused_hard_pick_up_keeps_the_checks_count() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.re2.start_new_game(false, true, false);
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 15, 3, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        game.fire_re2_area(game.room_actions[8].unwrap());
        take_through_the_window(&mut game, true);
        assert_eq!(game.item_count(0x14), 0);
        assert_eq!(game.room_actions[8].unwrap().params[1], 10);
        game.fire_re2_area(game.room_actions[8].unwrap());
        assert_eq!(game.room_actions[8].unwrap().params[1], 20);
        take_through_the_window(&mut game, false);
        assert_eq!(game.item_count(0x14), 20);
    }

    /// The stack the check chose takes the take's count with a byte add:
    /// 240 and HARD's 20 wrap to 4, in the same slot.
    #[test]
    fn a_hard_pick_up_onto_a_full_stack_wraps() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.re2.start_new_game(false, true, true);
        game.inventory = vec![crate::game::InventoryItem {
            id: 0x14,
            quantity: 240,
        }];
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 15, 3, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        game.fire_re2_area(game.room_actions[8].unwrap());
        take_through_the_window(&mut game, false);
        assert_eq!(
            game.inventory,
            [crate::game::InventoryItem {
                id: 0x14,
                quantity: 4
            }]
        );
    }

    /// Run the get-item window to its close, answering its question as the
    /// oracle's room 102 runs do: the window's tick, then the message's.
    /// Returns the tick numbers (from the pick-up's) the take, the window's
    /// cue and the close landed on.
    fn take_through_the_window(
        game: &mut GameState,
        refuse: bool,
    ) -> (Option<u32>, Option<u32>, u32) {
        use crate::game::Re2ItemGetPhase;
        use crate::game::status2::Sound;
        let mut taken = None;
        let mut cue = None;
        for tick in 1..400 {
            let before = game.inventory.clone();
            for sound in game.tick_re2_item_get(crate::game::status2::Input::default()) {
                assert_eq!(sound, Sound::Cancel);
                assert_eq!(cue, None, "one cue");
                cue = Some(tick);
            }
            if game.inventory != before {
                taken = Some(tick);
            }
            let Some(get) = game.re2_item_get else {
                return (taken, cue, tick);
            };
            // The window's message, answered (or closed) the tick it shows.
            if game.message.active
                && matches!(get.phase, Re2ItemGetPhase::Ask | Re2ItemGetPhase::Shown)
            {
                if get.phase == Re2ItemGetPhase::Ask && refuse {
                    game.message.set_menu_choice_id(1);
                } else {
                    game.message.set_menu_choice_id(0);
                }
                game.message.active = false;
            }
        }
        panic!("the window never closed");
    }

    #[test]
    fn the_get_item_window_keeps_the_originals_ticks() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.inventory = vec![crate::game::InventoryItem {
            id: 2,
            quantity: 18,
        }];
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        assert_eq!(
            game.inventory.len(),
            1,
            "the window takes the item, not the area"
        );
        // The question shows on the seventh tick and is answered on it; the
        // take is 20 ticks after the answer, the close ten after the tick
        // that sees the "taken" message gone, and the player restarts its
        // motion. The window's cancel cue plays on the tick that sees the
        // message gone.
        let (taken, cue, closed) = take_through_the_window(&mut game, false);
        assert_eq!(taken, Some(7 + 20));
        assert_eq!(cue, Some(7 + 20 + 1));
        assert_eq!(closed, 7 + 20 + 1 + 10);
        assert_eq!(game.item_count(0x14), 30);
        assert_eq!(game.re2.player.routine[2], 0);
        assert_eq!(game.re2.player.move_cnt, 1);
        assert!(game.re2.flag(ITEM_BANK, 154));
    }

    #[test]
    fn a_refused_item_stays_and_the_window_counts_out() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        let (taken, cue, closed) = take_through_the_window(&mut game, true);
        assert_eq!(taken, None);
        assert_eq!(cue, Some(7 + 16), "the count out ends in sub 0's cue");
        assert_eq!(closed, 7 + 16 + 10);
        assert!(!game.re2.flag(ITEM_BANK, 154));
        assert_eq!(game.room_actions[8].unwrap().handler, 1);
    }

    #[test]
    fn a_pick_up_merges_only_into_a_stack_with_room_for_all_of_it() {
        use crate::game::InventoryItem;
        let stack = |id, quantity| InventoryItem { id, quantity };
        // The oracle's room 102 with 230 and 220 rounds: the 30 skip the
        // first stack, which can't take them all, and fill the second to 250.
        let inventory = [
            stack(2, 18),
            stack(1, 1),
            stack(0x14, 230),
            stack(0x14, 220),
        ];
        assert_eq!(
            item_get_slot(&inventory, 8, 0x14, 30),
            Some(ItemGetSlot::Merge(3))
        );
        let full: Vec<_> = (0..8).map(|_| stack(0x14, 240)).collect();
        assert_eq!(item_get_slot(&full, 8, 0x14, 30), None);
        assert_eq!(
            item_get_slot(&full, 8, 0x14, 10),
            Some(ItemGetSlot::Merge(0))
        );
        // An item that doesn't stack needs a free slot.
        assert_eq!(item_get_slot(&inventory, 4, 0x1F, 1), None);
        assert_eq!(
            item_get_slot(&inventory, 8, 0x59, 1),
            Some(ItemGetSlot::New)
        );
        assert_eq!(item_max(0x14), 250);
        assert_eq!(item_max(0x1D), 240);
        assert_eq!(item_max(0x17), 100);
        assert_eq!(item_max(0x02), 18);
        assert_eq!(item_max(0x59), 1);
    }

    #[test]
    fn a_full_inventory_shows_its_message_and_leaves_the_item() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.inventory = (0..8)
            .map(|_| crate::game::InventoryItem { id: 1, quantity: 1 })
            .collect();
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        let (taken, _, closed) = take_through_the_window(&mut game, false);
        assert_eq!(taken, None);
        assert_eq!(closed, 7 + 1 + 10);
        assert_eq!(game.item_count(0x14), 0);
    }

    #[test]
    fn a_crouching_pick_up_opens_the_window_when_the_crouch_ends() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        // Action bit 0, as room 103's key has.
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 1];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.re2.player.routine = [1, 0, 1, 0];
        game.fire_re2_area(action);
        assert!(game.re2_item_get.is_none(), "the crouch comes first");
        // `Sce_Item` runs after this tick's `Player_move`, so the routine
        // changes after it.
        assert_eq!(game.re2.player.routine, [1, 0, 1, 0]);
        game.tick_re2_crouch();
        assert_eq!(game.re2.player.routine, [1, 6, 0, 0]);
        // A second press while crouching does nothing.
        game.fire_re2_area(action);
        game.tick_re2_crouch();
        assert_eq!(game.re2.player.routine, [1, 6, 0, 0]);
        assert!(game.re2_item_get.is_none());
        // `pl_mv06`'s sub 2 opens the window.
        game.re2.player.routine[2] = 3;
        game.re2.player.item_window = true;
        game.tick_re2_crouch();
        assert!(game.re2_item_get.is_some());
        assert!(game.re2_crouch_item.is_none());
        // The window's close runs `Pl_init`, which stands the player up.
        game.re2.player.move_no = 6;
        let (taken, _, _) = take_through_the_window(&mut game, false);
        assert_eq!(taken, Some(7 + 20));
        assert_eq!(game.re2.player.routine, [1, 0, 0, 0]);
        assert_eq!((game.re2.player.move_no, game.re2.player.move_cnt), (2, 1));
        assert_eq!(game.item_count(0x14), 30);
    }

    #[test]
    fn room_101s_shotgun_is_picked_up_crouching_from_the_scripts() {
        use crate::scd2::opcode::lookup;
        let mut game = GameState::default();
        game.is_re2_room = true;
        game.re2.player.routine = [1, 0, 1, 0];
        let mut host = GameHost { game: &mut game };
        // Room 101's `main_00`: the shotgun's area, put out at once.
        let set = [
            11, 2, 49, 0, 0, -17422, -15899, 2000, 2000, 7, 5, 15, 255, 1,
        ];
        host.exec(0, lookup(0x4E).unwrap(), &set);
        host.exec(0, lookup(0x46).unwrap(), &[11, 0, 0, 0, 0, 0, 0, 0, 0]);
        // `main_1D`, Kendo's second check: its first three words back, with
        // four shells, and the area run.
        host.exec(0, lookup(0x46).unwrap(), &[11, 2, 49, 7, 0, 4, 0, 15, 0]);
        host.exec(0, lookup(0x47).unwrap(), &[11]);
        // The action byte stayed: a crouch, set in the scripts' pass.
        assert!(game.re2_item_get.is_none());
        assert_eq!(game.re2.player.routine, [1, 6, 0, 0]);
        game.re2.player.item_window = true;
        game.tick_re2_crouch();
        let (taken, _, _) = take_through_the_window(&mut game, false);
        assert!(taken.is_some());
        assert_eq!(game.item_count(7), 4);
        // The close holds the next tick's keys, as a door's does.
        assert!(game.re2.door_key_stop);
    }

    #[test]
    fn a_checks_message_holds_fg_stop_until_it_is_dismissed() {
        use crate::scd2::opcode::lookup;
        let mut game = GameState::default();
        game.is_re2_room = true;
        *game.re2.fg_stop_mut() = 0x0100_0000;
        let mut host = GameHost { game: &mut game };
        // A scene's line holds nothing.
        host.exec(0, lookup(0x2B).unwrap(), &[0, 0, 0, 0, 0]);
        assert_eq!(*game.re2.fg_stop_mut(), 0x0100_0000);
        game.cancel_message();
        // Kendo's body: `message_on(0, 18, 0, 255, 255)`.
        let mut host = GameHost { game: &mut game };
        host.exec(0, lookup(0x2B).unwrap(), &[0, 18, 0, 255, 255]);
        assert!(game.message.active);
        assert_eq!(*game.re2.fg_stop_mut(), 0xFFFF_0000);
        assert!(game.re2.player_stopped());
        game.cancel_message();
        assert_eq!(*game.re2.fg_stop_mut(), 0x0100_0000);
    }

    #[test]
    fn a_crouch_that_is_cut_short_drops_its_item() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 1];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        game.tick_re2_crouch();
        // A hit takes the player out of `pl_mv06`.
        game.re2.player.routine = [2, 0, 0, 0];
        game.tick_re2_crouch();
        assert!(game.re2_crouch_item.is_none());
        assert!(game.re2_item_get.is_none());
    }

    #[test]
    fn the_get_item_window_clears_the_cut_while_the_room_is_hidden() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        let mut cuts = Vec::new();
        for _ in 0..7 {
            game.tick_re2_item_get(crate::game::status2::Input::default());
            cuts.push(game.re2_item_get_cut(3));
        }
        // `0x501CD9` on the sixth tick; the question shows on the seventh.
        assert_eq!(cuts, [3, 3, 3, 3, 3, 0, 0]);
        assert!(game.message.active);
        game.message.active = false;
        // The yes, the take, the "taken" message.
        while game
            .re2_item_get
            .is_some_and(|get| get.phase != crate::game::Re2ItemGetPhase::Close)
        {
            game.tick_re2_item_get(crate::game::status2::Input::default());
            game.message.active = false;
        }
        let mut cuts = Vec::new();
        while game.re2_item_get.is_some() {
            cuts.push(game.re2_item_get_cut(3));
            game.tick_re2_item_get(crate::game::status2::Input::default());
        }
        // `0x502027` on the close's eighth tick.
        assert_eq!(cuts, [0, 0, 0, 0, 0, 0, 0, 0, 3, 3]);
        assert_eq!(game.re2_item_get_cut(3), 3);
    }

    #[test]
    fn the_get_item_windows_game_frames_move_the_effects() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 30, 154, 1, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        let mut frames = Vec::new();
        while game.re2_item_get.is_some() {
            frames.push(game.re2_item_get_effects_frame());
            assert_eq!(
                frames.last().unwrap().is_some(),
                game.re2_item_get_stopped_frame(),
                "the effects run on the window's game frames, and only there"
            );
            game.tick_re2_item_get(crate::game::status2::Input::default());
            game.message.active = false;
        }
        // The opening fade's three frames are under `0xFF000000`: only the
        // kinds that carry on through a stop move. The close's last frame is
        // under `0xEF000000`, without `0x10000000`, so every effect moves.
        let shown: Vec<_> = frames.iter().flatten().copied().collect();
        assert_eq!(shown, [true, true, true, false]);
        assert_eq!(frames.first(), Some(&Some(true)));
        assert_eq!(frames.last(), Some(&Some(false)));

        // The pass takes the frame's stop and leaves the bank as it was
        // (`Esp_move`'s kinds under a stop are `esp2`'s own tests).
        for held in [0, 1] {
            let mut state = Re2State::default();
            state.set_flag(STOP_BANK, STOP_EFFECTS, held);
            state.tick_effects_as(held == 0, None, None);
            assert_eq!(state.flag(STOP_BANK, STOP_EFFECTS), held == 1);
        }
    }

    /// A hand-built room with one standing item area in slot 8: `item` with
    /// `flag` and object `model`, the press fired and the window open.
    fn press_item_area(item: u8, flag: u8, model: u8) -> GameState {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let (item, flag, model) = (i64::from(item), i64::from(flag), i64::from(model));
        let ops = [
            8, 2, 49, 0, 0, -100, -100, 200, 200, item, 1, flag, model, 0,
        ];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        // Ten live objects, to see which one a take clears.
        game.objects.records = vec![
            crate::objects::ObjectRecord {
                flag: 1,
                be_flg: 0x11,
                ..Default::default()
            };
            10
        ];
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        game
    }

    /// The objects whose `Be_flg` a take has cleared.
    fn cleared_objects(game: &GameState) -> Vec<usize> {
        (0..10)
            .filter(|&n| game.objects.record(n).unwrap().be_flg == 0)
            .collect()
    }

    /// Run the file window's ticks until it is in `phase`.
    fn run_to(game: &mut GameState, phase: crate::game::Re2ItemGetPhase) {
        for _ in 0..60 {
            if game.re2_item_get.is_some_and(|get| get.phase == phase) {
                return;
            }
            game.tick_re2_item_get(crate::game::status2::Input::default());
        }
        panic!("the window never reached {phase:?}");
    }

    /// Cancel on the first page, then let message 10 go down; returns the
    /// cues of the ticks after the cancel.
    fn file_it(game: &mut GameState) -> Vec<crate::game::status2::Sound> {
        use crate::game::status2::{Input, Sound};
        run_to(game, crate::game::Re2ItemGetPhase::FileRead);
        for _ in 0..13 {
            game.tick_re2_item_get(Input::default());
        }
        let cancel = Input {
            cancel: true,
            ..Input::default()
        };
        assert_eq!(game.tick_re2_item_get(cancel), [Sound::Cancel]);
        assert!(game.message.active, "message 10 is up");
        game.message.active = false;
        let mut cues = Vec::new();
        while game.re2_item_get.is_some() {
            cues.extend(game.tick_re2_item_get(Input::default()));
        }
        cues
    }

    #[test]
    fn a_file_area_skips_the_question_and_the_inventory() {
        use crate::game::Re2ItemGetPhase;
        let mut game = GameState::default();
        game.is_re2_room = true;
        let capacity = game.inventory_capacity();
        game.inventory = (0..capacity)
            .map(|_| crate::game::InventoryItem { id: 2, quantity: 1 })
            .collect();
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x6B, 1, 94, 2, 0];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
        let before = game.inventory.clone();
        let mut cuts = Vec::new();
        for _ in 0..6 {
            assert_eq!(game.re2.files[0], 0xFF, "the file goes in on tick 6");
            game.tick_re2_item_get(crate::game::status2::Input::default());
            cuts.push(game.re2_item_get_cut(3));
        }
        assert_eq!(cuts, [3, 3, 3, 3, 3, 0]);
        assert_eq!(game.re2.files[0], 3);
        assert_eq!(
            game.re2_file.blink,
            crate::game::status2::files::Blink::OPEN
        );
        assert_eq!(game.re2_file.x, 0x138);
        assert_eq!(game.re2_item_get.unwrap().phase, Re2ItemGetPhase::FileFade);
        for _ in 0..5 {
            game.tick_re2_item_get(crate::game::status2::Input::default());
            assert!(!game.message.active, "no question");
            assert_eq!(game.re2_item_get_cut(3), 0);
        }
        assert_eq!(game.inventory, before);
    }

    #[test]
    fn closing_the_filed_message_takes_the_area() {
        use crate::game::status2::Sound;
        let mut game = press_item_area(0x6B, 94, 2);
        let cues = file_it(&mut game);
        assert!(cues.contains(&Sound::Cancel));
        assert!(game.re2.flag(crate::scd2::host::ITEM_BANK, 94));
        assert_eq!(cleared_objects(&game), [2], "only the area's object goes");
        assert_eq!(game.objects.record(2).unwrap().flag, 0);
        assert_eq!(game.objects.record(3).unwrap().flag, 1);
        assert_eq!(game.room_actions[8].unwrap().sce, SCE_AUTO);
        assert!(game.inventory.is_empty());
        // The window closed as the item path's does, and a second press
        // finds nothing.
        assert!(game.re2.door_key_stop);
        assert!(game.re2_item_get.is_none());
    }

    #[test]
    fn a_file_whose_object_is_0xff_clears_no_object() {
        // `St_get_file` sub 6: `if (object != 0xff) pOm[object * 0x7e] = 0`.
        let mut game = press_item_area(0x71, 25, 0xFF);
        file_it(&mut game);
        assert_eq!(game.re2.files[0], 9);
        assert!(game.re2.flag(crate::scd2::host::ITEM_BANK, 25));
        assert_eq!(cleared_objects(&game), Vec::<usize>::new());
        assert_eq!(game.room_actions[8].unwrap().sce, SCE_AUTO);
    }

    #[test]
    fn a_file_from_aot_on_goes_to_the_file_list() {
        // Room 115's diary: a one-unit area, set on by the script.
        let mut game = press_item_area(0x68, 20, 7);
        file_it(&mut game);
        assert_eq!(game.re2.files[0], 0);
        assert!(game.re2.flag(crate::scd2::host::ITEM_BANK, 20));
        assert!(game.inventory.is_empty());
    }

    #[test]
    fn an_item_area_still_asks() {
        let mut game = press_item_area(0x47, 104, 0xFF);
        for _ in 0..7 {
            game.tick_re2_item_get(crate::game::status2::Input::default());
        }
        assert!(game.message.active);
        assert_eq!(
            game.re2_item_get.unwrap().phase,
            crate::game::Re2ItemGetPhase::Ask
        );
        assert_eq!(game.re2.files, [NO_FILE; FILE_SLOTS]);
    }

    #[test]
    fn an_area_on_a_body_moves_with_it() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        // `Super` 0xC0: relative to the player, as room 103's key (0xC2) is
        // to zombie Brad in enemy slot 0.
        let ops = [
            8, 2, 49, 0, 0xC0, -1000, -1000, 2000, 2000, 0x53, 1, 156, 255, 1,
        ];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let zone = game.room_actions[8].unwrap().zone;
        game.entities[0].pos = [5000, 0, 5000];
        let (gate, moved) = game.re2_area_gate(8, zone);
        assert!(gate.contains(moved, 5500, 5500, 0));
        assert!(!gate.contains(moved, 500, 500, 0));
        assert_eq!(gate.parent, Some(0xC0));
        // Without bit 0x80 the area stays where the script put it.
        let ops = [
            9, 2, 49, 0, 0x40, -1000, -1000, 2000, 2000, 0x53, 1, 156, 255, 1,
        ];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let zone = game.room_actions[9].unwrap().zone;
        let (gate, moved) = game.re2_area_gate(9, zone);
        assert_eq!(moved, zone);
        assert!(gate.contains(moved, 500, 500, 0));
    }

    #[test]
    fn aot_reset_gives_an_item_new_operands_and_keeps_its_action() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        // Room 103's key: the enemies' area (0x32) until Brad dies, then the
        // player's (0x31).
        let ops = [
            8, 2, 50, 0, 0xC2, -1000, -1000, 2000, 2000, 0x53, 1, 156, 255, 1,
        ];
        let mut host = GameHost { game: &mut game };
        host.exec(0, crate::scd2::opcode::lookup(0x4E).unwrap(), &ops);
        let reset = [8, 2, 49, 83, 0, 1, 0, 156, 0];
        host.exec(0, crate::scd2::opcode::lookup(0x46).unwrap(), &reset);
        let action = game.room_actions[8].unwrap();
        assert_eq!(action.kind, RoomActionKind::Item);
        assert_eq!(action.params[..5], [83, 1, 156, 255, 1]);
        assert_eq!(game.re2.gates[8].parent, Some(0xC2));
    }

    #[test]
    fn the_banks_reach_every_flg_addr_entry() {
        let mut state = Re2State::default();
        state.set_flag(33, 2, 1);
        state.set_flag(35, 4, 1);
        state.set_flag(39, 0, 1);
        assert!(state.flag(33, 2) && state.flag(35, 4) && state.flag(39, 0));
        assert!(!state.flag(40, 0));
        state.set_flag(40, 0, 1);
        assert!(!state.flag(40, 0));
    }

    #[test]
    fn a_save_keeps_the_map_banks() {
        let mut state = Re2State::default();
        for (bank, index) in [(9, 0x1F), (32, 1), (33, 2), (35, 4), (34, 7), (31, 3)] {
            state.set_flag(bank, index, 1);
        }
        state.vars[5] = 77;
        let bytes = state.to_save_bytes();
        assert_eq!(bytes.len(), SAVE_LEN);
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&bytes).unwrap();
        assert_eq!(loaded.banks, state.banks);
        assert_eq!(loaded.vars[5], 77);
        // A save from before them loads with banks 32 to 39 clear.
        let mut old = Re2State::default();
        old.set_flag(33, 5, 1);
        old.load_save_bytes(&bytes[..SAVE_LEN_NO_MAP]).unwrap();
        assert!(
            old.banks[SAVE_BANKS..]
                .iter()
                .flatten()
                .all(|&word| word == 0)
        );
        assert_eq!(old.banks[..SAVE_BANKS], state.banks[..SAVE_BANKS]);
        assert_eq!(old.vars[5], 77);
    }

    /// Sherry in `splayer_work` with `Type` `type_hi << 8`, the task's work.
    fn place_sherry(game: &mut GameState, type_hi: i64) {
        exec(
            game,
            "sce_em_set",
            &[
                0, 255, 69, 0, type_hi, 0, 32, 0, 255, -16641, 0, -23700, 2336, 0, 0,
            ],
        );
        exec(game, "work_set", &[WK_SPLAYER, 0]);
    }

    fn splayer(game: &GameState) -> &Character {
        game.re2.characters.0[SPLAYER_SLOT].as_deref().unwrap()
    }

    #[test]
    fn plc_ret_hands_a_scene_sherry_to_spl_auto() {
        let mut game = GameState::default();
        place_sherry(&mut game, 0x50);
        exec(&mut game, "plc_ret", &[]);
        let sherry = splayer(&game);
        assert_eq!(sherry.kind, 0);
        assert_eq!(sherry.work.routine, [1, 0, 0, 0]);
        assert!(sherry.partner.is_some());
        assert_eq!(
            plc_ret_kind(0x01, 0x5C12),
            0x0C12,
            "a player keeps the low nibble"
        );
    }

    /// `Plc_ret` clears `Type`'s 0x4000 and 0x1000 on any work: on an enemy
    /// that is `work.kind`, which `zombie2::kind()` reads (room 204's licker,
    /// placed with 0x4510).
    #[test]
    fn plc_ret_clears_an_enemys_type_too() {
        let mut game = GameState::default();
        let licker = [
            0, 1, 0x22, 16, 69, 0, 14, 0, 61, 32000, -5700, 32000, 1024, 0, 0,
        ];
        exec(&mut game, "sce_em_set", &licker);
        exec(&mut game, "work_set", &[WK_ENEMY, 1]);
        exec(&mut game, "plc_ret", &[]);
        let licker = game.re2.characters.0[1].as_ref().unwrap();
        assert_eq!(licker.work.kind, 0x0510);
        assert_eq!(licker.kind & 0x5000, 0);
        // And the partner's, in both fields.
        place_sherry(&mut game, 0x50);
        exec(&mut game, "plc_ret", &[]);
        let sherry = splayer(&game);
        assert_eq!((sherry.kind, sherry.work.kind & 0x5000), (0, 0));
    }

    /// `Plc_ret` on the player clears `Type`'s high nibble, so a scene that
    /// froze the player with `KIND_NOT_MOVED` hands it back to the pad.
    #[test]
    fn plc_ret_on_the_player_clears_its_type_high_nibble() {
        let mut game = GameState::default();
        game.re2.player.kind = crate::player2::KIND_NOT_MOVED | 0x0C00;
        exec(&mut game, "work_set", &[WK_PLAYER, 0]);
        exec(&mut game, "plc_ret", &[]);
        assert_eq!(game.re2.player.kind, 0x0C00);
    }

    #[test]
    fn splc_sce_and_splc_ret_swap_her_between_script_and_partner() {
        let mut game = GameState::default();
        game.re2.partner_life = 120;
        place_sherry(&mut game, 4);
        game.re2.set_partner_bit(true);
        if let Some(sherry) = game.re2.characters.0[SPLAYER_SLOT].as_deref_mut() {
            sherry.work.routine = [1, 0, 0, 0];
            sherry.partner.as_deref_mut().unwrap().state = crate::partner2::STATE_LOST;
        }
        exec(&mut game, "splc_sce", &[]);
        assert!(!game.re2.partner_bit());
        let sherry = splayer(&game);
        assert_eq!(sherry.work.routine, [4, 6, 0, 0]);
        assert_eq!(sherry.kind, 0x5000);
        assert_eq!(
            (sherry.work.neck.flags, sherry.work.neck.target),
            (0x92, None)
        );
        let partner = sherry.partner.as_deref().unwrap();
        assert_eq!((partner.life, partner.max_life), (-1, -1));
        exec(&mut game, "splc_ret", &[]);
        assert!(game.re2.partner_bit());
        let sherry = splayer(&game);
        assert_eq!(sherry.work.routine, [1, 0, 0, 0]);
        assert_eq!(sherry.kind, 0x0400);
        assert_eq!(
            (sherry.work.neck.flags, sherry.work.neck.target),
            (0, Some(0))
        );
        let partner = sherry.partner.as_deref().unwrap();
        assert_eq!((partner.life, partner.max_life), (120, 200));
        assert_eq!(partner.state & crate::partner2::STATE_LOST, 0, "not lost");
    }

    #[test]
    fn heal_partner_restores_200() {
        let mut game = GameState::default();
        place_sherry(&mut game, 4);
        game.re2.partner_life = 50;
        game.re2.partner_poisoned = true;
        if let Some(sherry) = game.re2.characters.0[SPLAYER_SLOT].as_deref_mut() {
            sherry.partner.as_deref_mut().unwrap().life = 50;
        }
        exec(&mut game, "heal_partner", &[]);
        assert_eq!(game.re2.partner_life, 200);
        assert!(!game.re2.partner_poisoned);
        let partner = splayer(&game).partner.as_deref().unwrap();
        assert_eq!((partner.life, partner.max_life), (200, 200));
    }

    #[test]
    fn the_partner_life_survives_a_save() {
        let state = Re2State {
            partner_life: 77,
            partner_poisoned: true,
            ..Default::default()
        };
        let bytes = state.to_save_bytes();
        assert_eq!(bytes.len(), SAVE_LEN);
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&bytes).unwrap();
        assert_eq!((loaded.partner_life, loaded.partner_poisoned), (77, true));
        // A save from before them: the new game's.
        let mut old = Re2State {
            partner_life: 5,
            ..Default::default()
        };
        old.load_save_bytes(&bytes[..SAVE_LEN_NO_PARTNER]).unwrap();
        assert_eq!((old.partner_life, old.partner_poisoned), (200, false));
    }

    #[test]
    fn a_script_reaches_the_map_banks() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let mut host = GameHost { game: &mut game };
        host.set_flag(33, 2, 1);
        assert!(host.flag(33, 2));
        assert!(game.re2.flag(33, 2));
    }

    #[test]
    fn the_personal_item_survives_a_save() {
        let state = Re2State {
            personal_item: [0x2F, 1],
            ..Default::default()
        };
        let bytes = state.to_save_bytes();
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&bytes).unwrap();
        assert_eq!(loaded.personal_item, [0x2F, 1]);
        // A save from before the slot loads with it empty.
        let mut old = Re2State::default();
        old.load_save_bytes(&bytes[..SAVE_LEN_NO_PERSONAL]).unwrap();
        assert_eq!(old.personal_item, [0, 0]);
    }

    #[test]
    fn the_item_box_survives_a_save() {
        let mut state = Re2State::default();
        state.item_box[0] = [0x14, 15, 0];
        state.item_box[63] = [0x0E, 7, 3];
        let bytes = state.to_save_bytes();
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&bytes).unwrap();
        assert_eq!(loaded.item_box, state.item_box);
        // A save from before the box loads with it empty.
        let mut old = Re2State::default();
        old.load_save_bytes(&bytes[..SAVE_LEN_NO_BOX]).unwrap();
        assert_eq!(old.item_box[0], [0; 3]);
    }

    /// Room 200's area 8 is `Sce_Windows` (kind 14), whose box nothing
    /// reaches: event 6's `aot_on 8` asks for game mode 7 with its first byte
    /// plus its third shifted by 4, mode 0 and cut 15, and nothing else.
    #[test]
    fn aot_on_of_sce_windows_requests_game_mode_7() {
        let mut game = GameState::default();
        exec(
            &mut game,
            "aot_set",
            &[8, 14, 49, 1, 0, 0, 0, 1, 1, 0, 0, 15, 0, 0, 0],
        );
        let flags = game.re2.banks;
        exec(&mut game, "aot_on", &[8]);
        assert!(game.re2_computer_request.is_some(), "`Sce_Windows` acts");
        assert!(game.re2_computer.is_none() && !game.message.active);
        assert_eq!(game.re2.banks, flags);
    }

    /// The computer with the Blue Card Key, driven through the game with
    /// confirm held and the card's question answered yes: the card leaves
    /// its slot and the herb after it moves up, 4:70 is raised, and the cut
    /// comes back to the counter's when it exits.
    #[test]
    fn the_computer_takes_the_card_and_releases_the_doors() {
        use crate::game::InventoryItem;
        let mut game = GameState::default();
        game.inventory = [(2, 18), (1, 1), (0x35, 1), (0x26, 1)]
            .map(|(id, quantity)| InventoryItem { id, quantity })
            .to_vec();
        game.equipped_weapon_id = 2;
        game.set_camera_cut(0);
        game.re2_computer = Some(crate::game::computer2::Computer::new(
            0xF0,
            crate::game::computer2::test_strings(),
        ));
        let mut cuts = Vec::new();
        for _ in 0..2000 {
            if game.re2_computer.is_none() {
                break;
            }
            cuts.extend(game.tick_re2_computer(true));
            // The question closes with its default, yes.
            game.message.active = false;
        }
        assert!(game.re2_computer.is_none());
        let ids: Vec<u8> = game.inventory.iter().map(|stack| stack.id).collect();
        assert_eq!(ids, [2, 1, 0x26]);
        assert_eq!(game.equipped_weapon_id, 2);
        assert!(game.re2.flag(4, 70));
        assert_eq!(cuts, [15, 9, 2, 15, 0]);
        assert_eq!(game.camera.current_cut, 0);
    }

    /// Bank 31 is `fg_key` (`Flg_addr[31] = 0x98ED2C`), the doors' lock bits:
    /// room 200's `set 31:17` is the bit room 20A's door 0 (lock `0x91`)
    /// unlocks, and a door unlocked from 20A's side is the bit room 200 checks.
    #[test]
    fn bank_31_is_the_door_lock_bits() {
        let mut state = Re2State::default();
        state.set_flag(KEY_BANK, 17, 1);
        assert_eq!(state.door_locks, [0x4000, 0]);
        assert_eq!(state.banks[usize::from(KEY_BANK)], [0; 8]);
        state.door_locks[1] |= 0x8000_0000;
        assert!(state.flag(KEY_BANK, 32) && state.flag(KEY_BANK, 17));
        state.set_flag(KEY_BANK, 17, 0);
        assert_eq!(state.door_locks, [0, 0x8000_0000]);

        // An old save's bank 31 bits merge into the locks.
        let mut old = Re2State::default();
        old.banks[usize::from(KEY_BANK)][0] = 0x4000;
        old.door_locks[0] = 0x1;
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&old.to_save_bytes()).unwrap();
        assert_eq!(loaded.door_locks[0], 0x4001);
        assert_eq!(loaded.banks[usize::from(KEY_BANK)], [0; 8]);
    }

    #[test]
    fn files_ride_in_the_save_and_old_saves_load_none() {
        let mut state = Re2State::default();
        assert_eq!(state.files, [NO_FILE; FILE_SLOTS]);
        state.files[0] = 1;
        state.files[23] = 0x18;
        let bytes = state.to_save_bytes();
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&bytes).unwrap();
        assert_eq!(loaded.files, state.files);
        // A save from before the files loads holding none.
        let mut old = Re2State {
            files: [3; FILE_SLOTS],
            ..Default::default()
        };
        old.load_save_bytes(&bytes[..SAVE_LEN_NO_FILES]).unwrap();
        assert_eq!(old.files, [NO_FILE; FILE_SLOTS]);
    }

    #[test]
    fn file_set_fills_the_first_free_slot() {
        let mut state = Re2State::default();
        assert_eq!(state.file_set(4), Some(0));
        state.files[1] = 9;
        assert_eq!(state.file_set(5), Some(2));
        state.files = [0; FILE_SLOTS];
        assert_eq!(state.file_set(6), None);
    }

    #[test]
    fn the_new_game_choice_sets_the_flags_and_survives_a_save() {
        let mut state = Re2State::default();
        state.start_new_game(true, false, true);
        assert_eq!(state.banks[0][0], 0x0008_0060);
        assert_eq!(state.banks[usize::from(STATUS_BANK)][0], 0x0400_0000);
        assert!(state.censorship_off && !state.hard_mode);
        let mut hard = Re2State::default();
        hard.start_new_game(false, true, false);
        assert_eq!(hard.banks[0][0], 0);
        let bytes = state.to_save_bytes();
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&bytes).unwrap();
        assert!(loaded.censorship_off && !loaded.hard_mode);
        assert_eq!(loaded.banks[0][0], 0x0008_0060);
        let mut loaded = Re2State::default();
        loaded.load_save_bytes(&hard.to_save_bytes()).unwrap();
        assert!(loaded.hard_mode && !loaded.censorship_off);
        // A save from before the two bytes loads with both clear.
        let mut old = Re2State::default();
        old.load_save_bytes(&bytes[..SAVE_LEN_NO_CHOICE]).unwrap();
        assert!(!old.censorship_off && !old.hard_mode);
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
