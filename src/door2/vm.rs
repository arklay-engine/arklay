//! The RE2 door-animation interpreter.
//!
//! RE2 runs a door with its normal SCD scheduler on four tasks (10-13) over
//! the DO2's procedure list. `Door_init` starts task 10 on procedure 0, sets
//! the camera (from `(10000, 0, 0)` to the origin, focal `0x122`) and seeds
//! four script variables: `0x0C` the door type's low seven bits, `0x0D` the
//! "still loading" flag, `0x0E` the type's top bit and `0x0F` the DO2 id.
//! Each frame `Door_move` runs every task until it yields, then `Door_Trans`
//! draws the ten door parts. A fresh skip press (`Door_move`'s trigger
//! check) stops task 10 at once. Once task 10 has ended `Door_move` draws
//! nothing more, and when fade 0 has run out `Door_exit` holds the screen
//! black and plays sound 1 when a `door_model_set` set attribute bit `0x800`.
//!
//! Only the opcodes the retail DO2s use are run (control flow, variables,
//! `work_set`/`speed_set`/`add_speed`/`add_aspeed`, the `member_*` position
//! members, `door_model_set`, `sce_fade_*` and `se_on`). Any other opcode
//! ends its task, as does a jump outside the procedure list.
//!
//! The frame is a [`crate::door::vm::Frame`], so the RE1 door renderer and
//! transition draw it: each part is an order whose flags draw it (`0x8001`)
//! while its `be_flg` is set, and whose [`DoorShading`] says how it is lit.
//!
//! `Door_Trans` lights a part through the GTE: `SetColorMatrix(door_lc)`,
//! `SetLightMatrix(door_ll * M)` with `M` the part's view matrix
//! (`Sp_matrix` times its world matrix), and a back colour of `0x44`
//! (`0x88` with attribute bit `0x1000`). The PC port hands Marni the light
//! and colour matrices (`FUN_004462e0`) but never reads the back colour, and
//! no code feeds Marni's own ambient, so a part's corner colour is
//! `min(255, sum_i (door_lc >> 4) * max(0, (door_ll * M)_i . n))`; see
//! [`crate::render::re2_door_light`]. The attribute bit `0x100` pulse writes
//! a part colour that nothing on the PC reads, so it is not run. Attribute
//! `0x4000` draws the part in Marni's additive mode (`0x200000`), and door
//! `0x2D` draws its parts with `attribute & 0xC0 == 0x40` through
//! `Marni_door_disp1`, as flat `0x808080` (full-bright) polygons.

use crate::anim::{self, Mat4x3};
use crate::door::vm::{DoorCamera, DoorShading, Fade, Frame, OrderFrame, Sfx};
use crate::model::Tmd;

use super::Do2;

/// Number of door parts (`pDwork`).
pub const PART_COUNT: usize = 10;
/// Number of script tasks the door runs on (10-13).
pub const TASK_COUNT: usize = 4;
/// Focal length of the door scene (`SetGeomScreen(0x122)`).
pub const FOCAL_LENGTH: i32 = 0x122;
/// The door scene camera's eye (`Set_view`).
pub const CAMERA_FROM: [i32; 3] = [10000, 0, 0];
/// Nesting depth of `gosub`.
const SUB_LEVELS: usize = 4;
/// Loop frames per subroutine level.
const LOOP_LEVELS: usize = 4;
/// `if` stack entries per subroutine level.
const STACK_LEVELS: usize = 8;
/// Script variables (`scd_variables`).
const VARIABLE_COUNT: usize = 0x100;
/// Opcodes one task may run in a frame before the VM gives up on it.
const MAX_OPS_PER_FRAME: usize = 0x1000;
/// Variable `0x0D`: the door's art is still loading.
const VAR_LOADING: usize = 0x0D;
/// Attribute bit: the part hangs off part `attribute & 0xF`.
const ATTR_PARENT: u16 = 0x10;
/// Attribute bit: play sound 1 when the door ends.
const ATTR_EXIT_SOUND: u16 = 0x800;
/// Attribute bit: draw the part additively (`Marni_door_disp0`'s flag).
const ATTR_ADDITIVE: u16 = 0x4000;
/// The door `Marni_door_disp1` draws unlit, for its parts with
/// `attribute & 0xC0 == 0x40`.
const FLAT_DOOR: u8 = 0x2D;
/// `door_ll` (bio2.exe `0x525F5C`): the GTE light matrix, one light per row.
pub const LIGHT_MATRIX: [[i16; 3]; 3] =
    [[400, 800, -500], [-1800, -1000, -2700], [3500, 6700, 1200]];
/// `door_lc` (bio2.exe `0x525F7C`): every light white at `1600`.
pub const COLOUR_MATRIX: [[i16; 3]; 3] = [[1600; 3]; 3];
/// The `se_on` sound type of the door's own bank.
const DOOR_SOUND_TYPE: u8 = 0;

/// The door record fields the scripts read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoorParams {
    /// The door type byte: the low seven bits pick the animation variant
    /// (variable `0x0C`), the top bit is variable `0x0E`.
    pub kind: u8,
    /// The DO2 id (variable `0x0F`).
    pub door: u8,
}

/// What a handler tells the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The condition failed: pop the innermost `if`.
    False,
    /// Run the next instruction.
    Next,
    /// The task is done for this frame.
    Yield,
}

/// One scheduler task (`SCE_TASK`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Task {
    active: bool,
    data: usize,
    sub: usize,
    ifel: [i8; SUB_LEVELS],
    loops: [i8; SUB_LEVELS],
    loop_if_class: [[i8; LOOP_LEVELS]; SUB_LEVELS],
    lstack: [[usize; LOOP_LEVELS]; SUB_LEVELS],
    lbreak: [[usize; LOOP_LEVELS]; SUB_LEVELS],
    lcnt: [[i16; LOOP_LEVELS]; SUB_LEVELS],
    stack: [usize; SUB_LEVELS * STACK_LEVELS],
    sp: usize,
    ret: [usize; SUB_LEVELS],
    /// The part `work_set` selected.
    work: Option<usize>,
    /// `spd`, `dspd`, `aspd` and `adspd`.
    speed: [i16; 12],
}

/// One door part (`pDwork` entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Part {
    visible: bool,
    model: u8,
    attribute: u16,
    position: [i32; 3],
    rotation: [i16; 3],
    world: Mat4x3,
}

/// A step of the script went wrong; the task ends.
#[derive(Debug)]
struct Fault;

/// The RE2 door VM.
#[derive(Debug, Clone)]
pub struct Vm {
    scripts: Vec<u8>,
    procedures: Vec<usize>,
    mesh: Tmd,
    door: u8,
    tasks: [Task; TASK_COUNT],
    parts: [Part; PART_COUNT],
    vars: [i16; VARIABLE_COUNT],
    fade: Fade,
    fade_writes: u32,
    exit_sound: bool,
    sfx: Vec<Sfx>,
    frame: u32,
    ended: bool,
    /// The last step ran `Door_Trans`.
    drawn: bool,
    done: bool,
    faults: u32,
}

impl Vm {
    /// `Door_init`: start task 10 on procedure 0.
    pub fn new(do2: &Do2, params: DoorParams) -> Self {
        let mut vm = Self {
            scripts: do2.scripts.clone(),
            procedures: do2.procedures.clone(),
            mesh: do2.mesh.clone(),
            door: params.door,
            tasks: Default::default(),
            parts: [Part::default(); PART_COUNT],
            vars: [0; VARIABLE_COUNT],
            fade: Fade {
                fade_type: 2,
                counter: 0,
                state: -1,
            },
            fade_writes: 0,
            exit_sound: false,
            sfx: Vec::new(),
            frame: 0,
            ended: false,
            drawn: false,
            done: false,
            faults: 0,
        };
        vm.vars[0x0C] = i16::from(params.kind & 0x7F);
        vm.vars[VAR_LOADING] = 1;
        vm.vars[0x0E] = i16::from(params.kind & 0x80);
        vm.vars[0x0F] = i16::from(params.door);
        vm.event_init(0, 0);
        vm
    }

    /// `Door_move` and `Door_Trans` for one 30 Hz frame.
    pub fn step(&mut self) -> Frame<'_> {
        self.sfx.clear();
        if !self.done {
            self.drawn = !self.ended;
            if self.ended {
                // `Door_move` finds task 10 gone and draws nothing.
                self.exit_if_faded();
            } else {
                // The art is loaded before the door starts.
                self.vars[VAR_LOADING] = 0;
                for task in 0..TASK_COUNT {
                    if self.tasks[task].active && self.run_task(task).is_err() {
                        self.faults += 1;
                        self.tasks[task].active = false;
                    }
                }
                self.transform();
                if !self.tasks[0].active {
                    self.ended = true;
                }
            }
            self.advance_fade();
            self.frame += 1;
        }
        self.frame()
    }

    /// The skip press: `Door_move` clears task 10, so the door ends the way
    /// it does when task 10 runs out, once fade 0 has.
    pub fn finish(&mut self) {
        self.ended = true;
    }

    /// A renderable snapshot of the current state.
    pub fn frame(&self) -> Frame<'_> {
        let orders = self
            .parts
            .iter()
            .map(|part| OrderFrame {
                flags: if part.visible { 0x8001 } else { 0 },
                model: part.model,
                parent: (part.attribute & ATTR_PARENT != 0).then_some((part.attribute & 0xF) as u8),
                tpage: 0,
                local: part.world,
                matrix: part.world,
                rotation: part.rotation,
                velocity: [0; 3],
                rot_velocity: [0; 3],
                mesh: self.mesh.objects.get(usize::from(part.model)),
                vertex_writes: Vec::new(),
                shading: self.shading(part.attribute),
            })
            .collect();
        Frame {
            camera: DoorCamera {
                from: CAMERA_FROM,
                to: [0; 3],
                focal: FOCAL_LENGTH,
            },
            fade: self.fade,
            orders,
            sfx: self.sfx.clone(),
            messages: Vec::new(),
            done: self.done,
            phase: 0,
            frame: self.frame,
            hold: 0,
            black: !self.drawn,
        }
    }

    /// How `Door_Trans` draws a part with `attribute`.
    fn shading(&self, attribute: u16) -> DoorShading {
        if attribute & 0xC0 == 0x40 && self.door == FLAT_DOOR {
            DoorShading::FullBright
        } else if attribute & ATTR_ADDITIVE != 0 {
            DoorShading::LitAdditive
        } else {
            DoorShading::Lit
        }
    }

    /// The number of `sce_fade_set`/`sce_fade_adjust` ops run so far.
    pub fn fade_writes(&self) -> u32 {
        self.fade_writes
    }

    /// Whether the door has ended.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// The number of tasks that stopped on an unsupported opcode or a jump
    /// outside the scripts.
    pub fn faults(&self) -> u32 {
        self.faults
    }

    /// The script variable `index`.
    pub fn var(&self, index: u8) -> i16 {
        self.vars[usize::from(index)]
    }

    /// `Door_exit` once fade 0 has run out (its accumulator went negative).
    fn exit_if_faded(&mut self) {
        if self.fade.state < 0 {
            self.exit();
        }
    }

    fn exit(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        for task in &mut self.tasks {
            task.active = false;
        }
        // Full black, as `Fade_adjust(0, 0x7FFF)` leaves it.
        self.fade = Fade {
            fade_type: 2,
            counter: 0,
            state: 0x7FFF,
        };
        self.fade_writes += 1;
        if self.exit_sound {
            self.sfx.push(door_sfx(1));
        }
    }

    /// `System_trans`: a running fade steps once a frame and stops when its
    /// accumulator goes negative.
    fn advance_fade(&mut self) {
        if self.fade.state >= 0 {
            self.fade.state = self.fade.state.wrapping_add(self.fade.counter as i16);
        }
    }

    /// `Event_exec`: start procedure `event` on `task` (10-13; past 13 picks
    /// the first free task).
    fn event_exec(&mut self, task: u8, event: u8) {
        let index = if task > 13 {
            match (0..TASK_COUNT).find(|&index| !self.tasks[index].active) {
                Some(index) => index,
                None => return,
            }
        } else if task >= 10 {
            usize::from(task - 10)
        } else {
            return;
        };
        self.tasks[index] = Task {
            work: self.tasks[index].work,
            ..Task::default()
        };
        self.event_init(index, event);
    }

    /// `Event_init`.
    fn event_init(&mut self, index: usize, event: u8) {
        let start = self.procedures.get(usize::from(event)).copied();
        let task = &mut self.tasks[index];
        task.active = start.is_some();
        task.data = start.unwrap_or(0);
        task.sub = 0;
        task.sp = 0;
        task.ifel[0] = -1;
        task.loops[0] = -1;
    }

    /// `Door_Trans`: each visible part's matrix, in part order.
    fn transform(&mut self) {
        for index in 0..PART_COUNT {
            let part = self.parts[index];
            if !part.visible {
                continue;
            }
            let local = Mat4x3 {
                r: anim::rotation_matrix(
                    i32::from(part.rotation[0]),
                    i32::from(part.rotation[1]),
                    i32::from(part.rotation[2]),
                ),
                t: part.position,
            };
            self.parts[index].world = if part.attribute & ATTR_PARENT != 0 {
                let parent = self.parts[usize::from(part.attribute & 0xF)].world;
                anim::compose(&parent, &local)
            } else {
                local
            };
        }
    }

    /// The scheduler's loop for one task.
    fn run_task(&mut self, index: usize) -> Result<(), Fault> {
        for _ in 0..MAX_OPS_PER_FRAME {
            match self.op(index)? {
                Step::Next => {}
                Step::Yield => return Ok(()),
                Step::False => {
                    let task = &mut self.tasks[index];
                    let sub = task.sub;
                    if task.ifel[sub] < 0 {
                        return Ok(());
                    }
                    task.sp = task.sp.checked_sub(1).ok_or(Fault)?;
                    task.data = task.stack[task.sp];
                    task.ifel[sub] -= 1;
                }
            }
            if !self.tasks[index].active {
                return Ok(());
            }
        }
        Err(Fault)
    }

    fn byte(&self, at: usize) -> Result<u8, Fault> {
        self.scripts.get(at).copied().ok_or(Fault)
    }

    fn short(&self, at: usize) -> Result<i16, Fault> {
        Ok(i16::from_le_bytes([self.byte(at)?, self.byte(at + 1)?]))
    }

    fn ushort(&self, at: usize) -> Result<usize, Fault> {
        Ok(usize::from(self.short(at)? as u16))
    }

    /// Run the instruction at task `index`'s cursor.
    fn op(&mut self, index: usize) -> Result<Step, Fault> {
        let at = self.tasks[index].data;
        let opcode = self.byte(at)?;
        let byte = |offset: usize| self.byte(at + offset);
        let short = |offset: usize| self.short(at + offset);
        let ushort = |offset: usize| self.ushort(at + offset);
        let step = match opcode {
            // nop
            0x00 => self.advance(index, 1),
            // evt_end
            0x01 => {
                let task = &mut self.tasks[index];
                if task.sub == 0 {
                    task.active = false;
                    return Ok(Step::Yield);
                }
                self.leave_sub(index)?;
                Step::Next
            }
            // evt_next
            0x02 => {
                self.tasks[index].data += 1;
                Step::Yield
            }
            // evt_exec
            0x04 => {
                let (task, event) = (byte(1)?, byte(3)?);
                self.tasks[index].data += 4;
                self.event_exec(task, event);
                Step::Next
            }
            // if
            0x06 => {
                let target = at + 4 + ushort(2)?;
                let task = &mut self.tasks[index];
                task.data = at + 4;
                task.ifel[task.sub] += 1;
                *task.stack.get_mut(task.sp).ok_or(Fault)? = target;
                task.sp += 1;
                Step::Next
            }
            // else
            0x07 => {
                let target = at + ushort(2)?;
                let task = &mut self.tasks[index];
                task.sp = task.sp.checked_sub(1).ok_or(Fault)?;
                task.data = target;
                task.ifel[task.sub] -= 1;
                Step::Next
            }
            // endif
            0x08 => {
                let task = &mut self.tasks[index];
                task.sp = task.sp.checked_sub(1).ok_or(Fault)?;
                task.data = at + 2;
                task.ifel[task.sub] -= 1;
                Step::Next
            }
            // sleep: push a counter, then fall into the `sleeping` that
            // follows it.
            0x09 => {
                let count = short(2)?;
                let slot = self.push_loop(index)?;
                let task = &mut self.tasks[index];
                task.data = at + 1;
                task.lcnt[task.sub][slot] = count;
                Step::Next
            }
            // sleeping
            0x0A => {
                let task = &mut self.tasks[index];
                let (sub, slot) = loop_slot(task)?;
                task.lcnt[sub][slot] = task.lcnt[sub][slot].wrapping_sub(1);
                if task.lcnt[sub][slot] == 0 {
                    task.data = at + 3;
                    task.loops[sub] -= 1;
                }
                Step::Yield
            }
            // for
            0x0D => {
                let (len, count) = (ushort(2)?, short(4)?);
                if count == 0 {
                    self.tasks[index].data = at + 6 + len;
                } else {
                    let slot = self.push_loop(index)?;
                    let task = &mut self.tasks[index];
                    let sub = task.sub;
                    task.lcnt[sub][slot] = count;
                    task.data = at + 6;
                    task.lbreak[sub][slot] = at + 6 + len;
                    task.lstack[sub][slot] = at + 6;
                    task.loop_if_class[sub][slot] = task.ifel[sub];
                }
                Step::Next
            }
            // next
            0x0E => {
                let task = &mut self.tasks[index];
                let (sub, slot) = loop_slot(task)?;
                task.lcnt[sub][slot] = task.lcnt[sub][slot].wrapping_sub(1);
                if task.lcnt[sub][slot] == 0 {
                    task.data = at + 2;
                    task.loops[sub] -= 1;
                } else {
                    task.data = task.lstack[sub][slot];
                }
                Step::Next
            }
            // while
            0x0F => {
                let (condition_len, len) = (usize::from(byte(1)?), ushort(2)?);
                let slot = self.push_loop(index)?;
                let body = at + 4;
                {
                    let task = &mut self.tasks[index];
                    let sub = task.sub;
                    task.lstack[sub][slot] = at;
                    task.data = body;
                    task.lbreak[sub][slot] = body + len;
                    task.loop_if_class[sub][slot] = task.ifel[sub];
                }
                if !self.condition(index, body, condition_len)? {
                    let task = &mut self.tasks[index];
                    let sub = task.sub;
                    task.data = task.lbreak[sub][slot];
                    task.loops[sub] -= 1;
                }
                Step::Next
            }
            // ewhile
            0x10 => {
                let task = &mut self.tasks[index];
                let (sub, slot) = loop_slot(task)?;
                task.data = task.lstack[sub][slot];
                task.loops[sub] -= 1;
                Step::Next
            }
            // switch
            0x13 => self.switch(index, at)?,
            // case met in the flow: fall into its body.
            0x14 => self.advance(index, 6),
            // default
            0x15 => self.advance(index, 2),
            // eswitch
            0x16 => {
                let task = &mut self.tasks[index];
                task.loops[task.sub] -= 1;
                task.data = at + 2;
                Step::Next
            }
            // goto
            0x17 => {
                let (ifel, loops, offset) = (byte(1)? as i8, byte(2)? as i8, short(4)?);
                let task = &mut self.tasks[index];
                let sub = task.sub;
                task.ifel[sub] = ifel;
                task.loops[sub] = loops;
                task.sp = stack_top(sub, ifel)?;
                task.data = at.checked_add_signed(isize::from(offset)).ok_or(Fault)?;
                Step::Next
            }
            // gosub
            0x18 => {
                let start = *self.procedures.get(usize::from(byte(1)?)).ok_or(Fault)?;
                let task = &mut self.tasks[index];
                let sub = task.sub;
                if sub + 1 >= SUB_LEVELS {
                    return Err(Fault);
                }
                task.ret[sub] = at + 2;
                task.ifel[sub + 1] = -1;
                task.loops[sub + 1] = -1;
                task.sp = (sub + 1) * STACK_LEVELS;
                task.sub = sub + 1;
                task.data = start;
                Step::Next
            }
            // return
            0x19 => {
                self.leave_sub(index)?;
                Step::Next
            }
            // break
            0x1A => {
                let task = &mut self.tasks[index];
                let (sub, slot) = loop_slot(task)?;
                task.data = task.lbreak[sub][slot];
                task.ifel[sub] = task.loop_if_class[sub][slot];
                task.loops[sub] -= 1;
                Step::Next
            }
            // work_copy: patch the instruction stream.
            0x1D => {
                let (var, offset, word) = (byte(1)?, usize::from(byte(2)?), byte(3)? != 0);
                let value = self.vars[usize::from(var)];
                let target = at + 4 + offset;
                let len = if word { 2 } else { 1 };
                let bytes = value.to_le_bytes();
                self.scripts
                    .get_mut(target..target + len)
                    .ok_or(Fault)?
                    .copy_from_slice(&bytes[..len]);
                self.advance(index, 4)
            }
            // cmp
            0x23 => {
                let (var, relation, value) = (byte(2)?, byte(3)?, short(4)?);
                self.tasks[index].data = at + 6;
                let current = self.vars[usize::from(var)];
                let pass = match relation {
                    0 => current == value,
                    1 => value < current,
                    2 => value <= current,
                    3 => current < value,
                    4 => current <= value,
                    5 => current != value,
                    6 => current & value != 0,
                    _ => current != 0,
                };
                if pass { Step::Next } else { Step::False }
            }
            // save
            0x24 => {
                let (var, value) = (usize::from(byte(1)?), short(2)?);
                self.vars[var] = value;
                self.advance(index, 4)
            }
            // copy
            0x25 => {
                let (to, from) = (usize::from(byte(1)?), usize::from(byte(2)?));
                self.vars[to] = self.vars[from];
                self.advance(index, 3)
            }
            // calc
            0x26 => {
                let (operation, var, value) = (byte(2)?, usize::from(byte(3)?), short(4)?);
                self.vars[var] = calc(operation, self.vars[var], value);
                self.advance(index, 6)
            }
            // work_set: only kind 5 (a door part) has a target here.
            0x2E => {
                let (kind, part) = (byte(1)?, byte(2)? as i8);
                let task = &mut self.tasks[index];
                task.speed = [0; 12];
                task.data = at + 3;
                if kind == 5 {
                    task.work = usize::try_from(part).ok().filter(|&part| part < PART_COUNT);
                }
                Step::Next
            }
            // speed_set
            0x2F => {
                let (slot, value) = (usize::from(byte(1)?), short(2)?);
                *self.tasks[index].speed.get_mut(slot).ok_or(Fault)? = value;
                self.advance(index, 4)
            }
            // add_speed
            0x30 => {
                let Task { work, speed, .. } = self.tasks[index];
                if let Some(part) = work.map(|part| &mut self.parts[part]) {
                    for axis in 0..3 {
                        part.position[axis] += i32::from(speed[axis]);
                        part.rotation[axis] = part.rotation[axis].wrapping_add(speed[3 + axis]);
                    }
                }
                self.advance(index, 1)
            }
            // add_aspeed
            0x31 => {
                let speed = &mut self.tasks[index].speed;
                for slot in 0..6 {
                    speed[slot] = speed[slot].wrapping_add(speed[6 + slot]);
                }
                self.advance(index, 1)
            }
            // member_set
            0x34 => {
                let (member, value) = (byte(1)?, short(2)?);
                self.set_member(index, member, value);
                self.advance(index, 4)
            }
            // member_set2
            0x35 => {
                let (member, var) = (byte(1)?, byte(2)?);
                self.set_member(index, member, self.vars[usize::from(var)]);
                self.advance(index, 3)
            }
            // se_on
            0x36 => {
                let (kind, sound) = (byte(1)?, short(2)?);
                if kind == DOOR_SOUND_TYPE {
                    self.sfx.push(door_sfx((sound & 0xFF) as u8));
                }
                self.advance(index, 12)
            }
            // member_copy
            0x3D => {
                let (var, member) = (byte(1)?, byte(2)?);
                let value = self.member(index, member);
                self.vars[usize::from(var)] = value;
                self.advance(index, 3)
            }
            // door_model_set
            0x4D => {
                let part = usize::from(byte(1)?);
                let attribute = ushort(6)? as u16;
                let mut position = [0i32; 3];
                let mut rotation = [0i16; 3];
                for axis in 0..3 {
                    position[axis] = i32::from(short(10 + axis * 2)?);
                    rotation[axis] = short(16 + axis * 2)?;
                }
                let visible = byte(4)? != 0;
                let model = byte(5)?;
                let slot = self.parts.get_mut(part).ok_or(Fault)?;
                *slot = Part {
                    visible,
                    model,
                    attribute,
                    position,
                    rotation,
                    world: slot.world,
                };
                if attribute & ATTR_EXIT_SOUND != 0 {
                    self.exit_sound = true;
                }
                self.advance(index, 22)
            }
            // sce_fade_set: only fade 0 is the screen fade.
            0x53 => {
                let (fade, kind, add) = (byte(1)?, byte(2)?, short(4)?);
                if fade == 0 {
                    self.fade = Fade {
                        fade_type: kind,
                        counter: i32::from(add),
                        state: if add < 0 {
                            add.wrapping_add(i16::MIN)
                        } else {
                            0
                        },
                    };
                    self.fade_writes += 1;
                }
                self.advance(index, 6)
            }
            // sce_fade_adjust
            0x74 => {
                let (fade, state) = (byte(1)?, short(2)?);
                if fade == 0 {
                    self.fade.state = state;
                    self.fade_writes += 1;
                }
                self.advance(index, 4)
            }
            _ => return Err(Fault),
        };
        Ok(step)
    }

    fn advance(&mut self, index: usize, len: usize) -> Step {
        self.tasks[index].data += len;
        Step::Next
    }

    /// `return`, and `evt_end` inside a `gosub`.
    fn leave_sub(&mut self, index: usize) -> Result<(), Fault> {
        let task = &mut self.tasks[index];
        let sub = task.sub.checked_sub(1).ok_or(Fault)?;
        task.sub = sub;
        task.data = task.ret[sub];
        task.sp = stack_top(sub, task.ifel[sub])?;
        Ok(())
    }

    /// Open a loop frame on task `index` and return its slot.
    fn push_loop(&mut self, index: usize) -> Result<usize, Fault> {
        let task = &mut self.tasks[index];
        let sub = task.sub;
        task.loops[sub] += 1;
        let slot = usize::try_from(task.loops[sub]).map_err(|_| Fault)?;
        if slot >= LOOP_LEVELS {
            return Err(Fault);
        }
        Ok(slot)
    }

    /// `While_main`: the first condition, then `(and/or, condition)` pairs
    /// up to `start + len`.
    fn condition(&mut self, index: usize, start: usize, len: usize) -> Result<bool, Fault> {
        let mut result = self.op(index)? != Step::False;
        let end = start + len;
        let mut guard = 0;
        while self.tasks[index].data != end {
            let at = self.tasks[index].data;
            if at > end || guard >= MAX_OPS_PER_FRAME {
                return Err(Fault);
            }
            guard += 1;
            let or = self.short(at)? != 0;
            self.tasks[index].data = at + 2;
            let pass = self.op(index)? != Step::False;
            result = if or { result | pass } else { result & pass };
        }
        Ok(result)
    }

    /// `switch`: open a loop frame and jump to the matching case's body,
    /// `default` or past `eswitch`.
    fn switch(&mut self, index: usize, at: usize) -> Result<Step, Fault> {
        let (var, len) = (usize::from(self.byte(at + 1)?), self.ushort(at + 2)?);
        let slot = self.push_loop(index)?;
        {
            let task = &mut self.tasks[index];
            let sub = task.sub;
            task.lbreak[sub][slot] = at + 4 + len;
            task.loop_if_class[sub][slot] = task.ifel[sub];
        }
        let value = self.vars[var];
        let mut entry = at + 4;
        for _ in 0..MAX_OPS_PER_FRAME {
            match self.byte(entry)? {
                0x15 => {
                    self.tasks[index].data = entry + 2;
                    return Ok(Step::Next);
                }
                0x16 => {
                    let task = &mut self.tasks[index];
                    task.loops[task.sub] -= 1;
                    task.data = entry + 2;
                    return Ok(Step::Next);
                }
                _ => {
                    if self.short(entry + 4)? == value {
                        self.tasks[index].data = entry + 6;
                        return Ok(Step::Next);
                    }
                    entry += 6 + self.ushort(entry + 2)?;
                }
            }
        }
        Err(Fault)
    }

    /// `Member_set_branch` for the members the door scripts use: the
    /// position (`0x0B`-`0x0D`) and rotation (`0x0E`-`0x10`).
    fn set_member(&mut self, index: usize, member: u8, value: i16) {
        let Some(part) = self.tasks[index].work.map(|part| &mut self.parts[part]) else {
            return;
        };
        match member {
            0x0B..=0x0D => part.position[usize::from(member - 0x0B)] = i32::from(value),
            0x0E..=0x10 => part.rotation[usize::from(member - 0x0E)] = value,
            _ => {}
        }
    }

    /// `Load_member_branch` for the same members.
    fn member(&self, index: usize, member: u8) -> i16 {
        let Some(part) = self.tasks[index].work.map(|part| &self.parts[part]) else {
            return 0;
        };
        match member {
            0x0B..=0x0D => part.position[usize::from(member - 0x0B)] as i16,
            0x0E..=0x10 => part.rotation[usize::from(member - 0x0E)],
            _ => 0,
        }
    }
}

/// The current loop frame of a task.
fn loop_slot(task: &Task) -> Result<(usize, usize), Fault> {
    let sub = task.sub;
    let slot = usize::try_from(task.loops[sub]).map_err(|_| Fault)?;
    if slot >= LOOP_LEVELS {
        return Err(Fault);
    }
    Ok((sub, slot))
}

/// The stack pointer for `ifel` open `if`s at subroutine level `sub`.
fn stack_top(sub: usize, ifel: i8) -> Result<usize, Fault> {
    let depth = usize::try_from(i16::from(ifel) + 1).map_err(|_| Fault)?;
    if depth > STACK_LEVELS {
        return Err(Fault);
    }
    Ok(sub * STACK_LEVELS + depth)
}

/// `Calc_branch`.
fn calc(operation: u8, value: i16, operand: i16) -> i16 {
    let shift = (operand & 0x1F) as u32;
    match operation {
        0 => value.wrapping_add(operand),
        1 => value.wrapping_sub(operand),
        2 => value.wrapping_mul(operand),
        3 => value.checked_div(operand).unwrap_or(value),
        4 => value.checked_rem(operand).unwrap_or(value),
        5 => value | operand,
        6 => value & operand,
        7 => value ^ operand,
        8 => !value,
        9 => ((value as u16) << shift.min(15)) as i16,
        10 => ((value as u16) >> shift.min(15)) as i16,
        11 => value >> shift.min(15),
        _ => value,
    }
}

/// A sound from the door's own bank.
fn door_sfx(index: u8) -> Sfx {
    Sfx {
        bank: DOOR_SOUND_TYPE,
        id: index,
        volume: 0x7F,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Texture8, TmdObject};

    /// A DO2 with `procedures` as its script list.
    fn do2(procedures: &[&[u8]]) -> Do2 {
        let mut scripts = Vec::new();
        let mut at = procedures.len() * 2;
        let mut starts = Vec::new();
        for procedure in procedures {
            scripts.extend_from_slice(&(at as u16).to_le_bytes());
            starts.push(at);
            at += procedure.len();
        }
        for procedure in procedures {
            scripts.extend_from_slice(procedure);
        }
        Do2 {
            sounds: [None; super::super::SOUND_COUNT],
            vab: Default::default(),
            scripts,
            procedures: starts,
            mesh: Tmd {
                objects: vec![TmdObject::default(); 2],
            },
            texture: Texture8 {
                width: 0,
                height: 0,
                indices: Vec::new(),
                palettes: Vec::new(),
                stp: Vec::new(),
            },
        }
    }

    fn model_set(part: u8, attribute: u16, position: [i16; 3]) -> Vec<u8> {
        let mut op = vec![0x4D, part, 0, 0, 1, part];
        op.extend_from_slice(&attribute.to_le_bytes());
        op.extend_from_slice(&[0, 0]);
        for value in position {
            op.extend_from_slice(&value.to_le_bytes());
        }
        op.extend_from_slice(&[0; 6]);
        assert_eq!(op.len(), 22);
        op
    }

    #[test]
    fn switch_picks_the_variant_and_runs_its_procedure() {
        // switch var 0x0C { case 0: gosub 1; break; case 1: gosub 2; break }
        let main: &[u8] = &[
            0x13, 0x0C, 0x16, 0x00, //
            0x14, 0x00, 0x04, 0x00, 0x00, 0x00, 0x18, 0x01, 0x1A, 0x00, //
            0x14, 0x00, 0x04, 0x00, 0x01, 0x00, 0x18, 0x02, 0x1A, 0x00, //
            0x16, 0x00, 0x01, 0x00,
        ];
        let first = [model_set(0, 0x800, [10, 0, 0]), vec![0x01, 0x00]].concat();
        let second = [model_set(1, 0, [20, 0, 0]), vec![0x01, 0x00]].concat();
        for (kind, part) in [(0u8, 0usize), (1, 1)] {
            let mut vm = Vm::new(&do2(&[main, &first, &second]), DoorParams { kind, door: 3 });
            let frame = vm.step();
            assert_eq!(frame.orders[part].flags, 0x8001, "variant {kind}");
            assert_eq!(frame.orders[1 - part].flags, 0);
            assert_eq!(vm.faults(), 0);
            assert_eq!(vm.var(0x0F), 3);
        }
    }

    #[test]
    fn attributes_pick_how_a_part_is_shaded() {
        // Part 0 plain, part 1 additive, part 2 `disp1`'s flat attribute.
        let main = [
            model_set(0, 0x80, [0; 3]),
            model_set(1, 0x40D0, [0; 3]),
            model_set(2, 0x52, [0; 3]),
            vec![0x01, 0x00],
        ]
        .concat();
        let shading = |door| {
            let mut vm = Vm::new(&do2(&[&main]), DoorParams { kind: 0, door });
            vm.step().orders[..3]
                .iter()
                .map(|order| order.shading)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shading(0x2D),
            [
                DoorShading::Lit,
                DoorShading::LitAdditive,
                DoorShading::FullBright
            ]
        );
        // Only door 0x2D draws through `Marni_door_disp1`.
        assert_eq!(
            shading(0x00),
            [DoorShading::Lit, DoorShading::LitAdditive, DoorShading::Lit]
        );
    }

    #[test]
    fn loops_and_speeds_move_a_part_one_step_a_frame() {
        // door_model_set 0; work_set 5 0; speed_set 0 = 5; speed_set 4 = 8;
        // for 3 { add_speed; evt_next } ; evt_end
        let body = [0x30, 0x02, 0x0E, 0x00];
        let script = [
            model_set(0, 0, [0, 0, 0]),
            vec![
                0x2E, 0x05, 0x00, 0x2F, 0x00, 0x05, 0x00, 0x2F, 0x04, 0x08, 0x00,
            ],
            vec![0x0D, 0x00, body.len() as u8, 0x00, 0x03, 0x00],
            body.to_vec(),
            vec![0x01, 0x00],
        ]
        .concat();
        let mut vm = Vm::new(&do2(&[&script]), DoorParams::default());
        let mut xs = Vec::new();
        for _ in 0..4 {
            let frame = vm.step();
            xs.push((frame.orders[0].matrix.t[0], frame.orders[0].rotation[1]));
        }
        assert_eq!(xs, [(5, 8), (10, 16), (15, 24), (15, 24)]);
        assert_eq!(vm.faults(), 0);
    }

    #[test]
    fn a_false_if_skips_to_its_else_and_sleep_yields() {
        // if (var 0x0C == 1) { save 0x20 = 1 } else { save 0x20 = 2 };
        // sleep 2; evt_end
        let then = [0x24, 0x20, 0x01, 0x00, 0x07, 0x00, 0x06, 0x00];
        let otherwise = [0x24, 0x20, 0x02, 0x00, 0x00, 0x00];
        let script = [
            vec![0x06, 0x00, (6 + then.len()) as u8, 0x00],
            vec![0x23, 0x00, 0x0C, 0x00, 0x01, 0x00],
            then.to_vec(),
            otherwise.to_vec(),
            vec![0x09, 0x0A, 0x02, 0x00, 0x01, 0x00],
        ]
        .concat();
        let mut vm = Vm::new(&do2(&[&script]), DoorParams::default());
        vm.step();
        assert_eq!(vm.var(0x20), 2);
        assert_eq!(vm.faults(), 0);
        assert!(!vm.ended);
        vm.step();
        vm.step();
        assert!(vm.ended);
    }

    #[test]
    fn the_door_waits_for_the_fade_and_plays_the_exit_sound() {
        // door_model_set with the exit-sound bit; fade 0 to black at +0x4000;
        // evt_end.
        let script = [
            model_set(0, 0x800, [0, 0, 0]),
            vec![0x53, 0x00, 0x02, 0x07, 0x00, 0x40],
            vec![0x01, 0x00],
        ]
        .concat();
        let mut vm = Vm::new(&do2(&[&script]), DoorParams::default());
        assert_eq!(vm.step().fade.state, 0x4000);
        assert!(!vm.is_done());
        // 0x4000 + 0x4000 wraps negative: the fade is over, and `Door_exit`
        // runs on the next frame.
        assert!(!vm.step().done);
        let frame = vm.step();
        assert!(frame.done && frame.black);
        assert_eq!(frame.sfx.len(), 1);
        assert_eq!(frame.sfx[0].id, 1);
    }

    #[test]
    fn a_skip_stops_the_door_and_blanks_it_until_the_fade_ends() {
        // A part, a slow fade 0, then a long sleep.
        let script = [
            model_set(0, 0, [0, 0, 0]),
            vec![0x53, 0x00, 0x02, 0x00, 0x00, 0x01],
            vec![0x09, 0x0A, 0xFF, 0x7F, 0x01, 0x00],
        ]
        .concat();
        let mut vm = Vm::new(&do2(&[&script]), DoorParams::default());
        assert!(!vm.step().black);
        vm.finish();
        let frame = vm.step();
        assert!(frame.black && !frame.done);
        // The fade, 0x100 a frame, wraps negative on its 128th frame (the
        // 127th after the skip) and `Door_exit` runs on the next.
        let mut frames = 1;
        while !vm.step().done {
            frames += 1;
        }
        assert_eq!(frames, 127);
    }

    #[test]
    fn an_unknown_opcode_ends_its_task() {
        let mut vm = Vm::new(&do2(&[&[0x8E]]), DoorParams::default());
        vm.step();
        assert_eq!(vm.faults(), 1);
        assert!(vm.ended);
    }

    #[test]
    fn calc_follows_calc_branch() {
        assert_eq!(calc(0, 3, 4), 7);
        assert_eq!(calc(3, 7, 0), 7);
        assert_eq!(calc(10, -2, 1), 0x7FFF);
        assert_eq!(calc(11, -2, 1), -1);
    }
}
