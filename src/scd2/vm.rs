//! The RE2 script VM: the scheduler and control flow of bio2's `Sce_scheduler`.
//!
//! A task runs one procedure from the current list, up to a `sleep` or
//! `evt_next` each tick. Its `if` and loop stacks are per call depth and follow
//! the original's counters: a failed condition jumps to the innermost `if`'s
//! end, and outside any `if` it just ends the task's tick. Everything that is
//! not control flow, flags or variables goes to the [`Host`].

use super::opcode::{self, Op};
use super::{ProcList, Scripts};

/// The number of task slots (`get_max_tasks`).
pub const TASKS: usize = 14;
/// The task slots a procedure can be auto-assigned to by `evt_exec`.
const AUTO_TASKS: std::ops::Range<usize> = 2..10;
/// The first task id `evt_exec` treats as "any free slot".
const AUTO_TASK_ID: usize = 10;
/// How many instructions one task may run in a tick before it is cut off, so
/// a retail script that spins waiting on a state this engine never reaches
/// costs a tick, not the session.
const MAX_STEPS: usize = 4096;
/// The `gosub` nesting the original has room for.
const MAX_DEPTH: usize = 4;

/// What a host instruction asks of the running task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Run the next instruction now.
    Next,
    /// The instruction's condition failed.
    False,
    /// Stop this task until the next tick, after this instruction.
    Yield,
}

/// The game side of the VM.
pub trait Host {
    /// The bit `index` of flag bank `bank`.
    fn flag(&self, bank: u8, index: u8) -> bool;
    /// Clear (0), set (1) or toggle (7) a flag bit.
    fn set_flag(&mut self, bank: u8, index: u8, op: u8);
    /// The script variable `index`.
    fn var(&self, index: u8) -> u16;
    /// Write the script variable `index`.
    fn set_var(&mut self, index: u8, value: u16);
    /// Whether `wsleep` still waits: a message or a screen holds the script.
    fn waiting(&self) -> bool;
    /// The next random number of `sce_rnd`.
    fn random(&mut self) -> u16;
    /// Run a non control flow instruction with its decoded operands, for the
    /// task `task`.
    fn exec(&mut self, task: usize, op: &'static Op, operands: &[i64]) -> Step;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Loop {
    /// Where `next`, `ewhile` and a true `edwhile` return to.
    start: usize,
    /// Where `break` goes.
    end: usize,
    /// `for`'s and `sleep`'s remaining count.
    count: u16,
    /// The `if` depth `break` returns to.
    if_depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Frame {
    proc: usize,
    pc: usize,
    ifs: Vec<usize>,
    loops: Vec<Loop>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Task {
    frame: Frame,
    calls: Vec<Frame>,
    running: bool,
}

/// The list a task's procedure numbers index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Active {
    Init,
    Main,
}

/// A room's script run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vm {
    init: Vec<Vec<u8>>,
    main: Vec<Vec<u8>>,
    active: Active,
    tasks: [Task; TASKS],
}

fn procedures(list: &ProcList) -> Vec<Vec<u8>> {
    list.procs.iter().map(|proc| proc.to_bytes()).collect()
}

fn u16_at(code: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*code.get(at)?, *code.get(at + 1)?]))
}

impl Vm {
    pub fn new(scripts: &Scripts) -> Self {
        Self {
            init: procedures(&scripts.init),
            main: procedures(&scripts.main),
            active: Active::Main,
            tasks: std::array::from_fn(|_| Task::default()),
        }
    }

    fn list(&self) -> &[Vec<u8>] {
        match self.active {
            Active::Init => &self.init,
            Active::Main => &self.main,
        }
    }

    fn start(&mut self, task: usize, proc: usize) {
        self.tasks[task] = Task {
            frame: Frame {
                proc,
                ..Frame::default()
            },
            calls: Vec::new(),
            running: proc < self.list().len(),
        };
    }

    /// Run the room's init procedure to its end: the original runs it inside
    /// `room_set` before the room shows, so every sleep in it is ignored by
    /// running on until it finishes or the step budget ends it.
    pub fn run_init(&mut self, host: &mut impl Host) {
        self.active = Active::Init;
        self.start(0, 0);
        let mut ticks = 0;
        while self.tasks[0].running && ticks < 256 {
            self.run_task(0, host);
            ticks += 1;
        }
        self.tasks[0].running = false;
        self.active = Active::Main;
        // Tasks the init started index the main list from here on.
    }

    /// Start the room's main procedure, as `room_set` does after the init.
    pub fn start_main(&mut self) {
        self.active = Active::Main;
        if !self.tasks[0].running && !self.main.is_empty() {
            self.start(0, 0);
        }
    }

    /// One tick: every task runs in slot order.
    pub fn tick(&mut self, host: &mut impl Host) {
        for task in 0..TASKS {
            if self.tasks[task].running {
                self.run_task(task, host);
            }
        }
    }

    /// `Sce_event`: start main procedure `event` on `task` for an interaction
    /// area. A task id past the fixed slots takes any free one.
    pub fn exec_event(&mut self, task: usize, event: usize) {
        self.active = Active::Main;
        let slot = if task >= AUTO_TASK_ID {
            AUTO_TASKS.clone().find(|slot| !self.tasks[*slot].running)
        } else {
            Some(task)
        };
        if let Some(slot) = slot.filter(|slot| *slot < TASKS) {
            self.start(slot, event);
        }
    }

    /// The room's main procedure count.
    pub fn main_procedures(&self) -> usize {
        self.main.len()
    }

    /// The instruction each running task is at, for diagnostics.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (task, state) in self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.running)
        {
            let frame = &state.frame;
            let code = self.list().get(frame.proc).map_or(&[][..], Vec::as_slice);
            let op = code
                .get(frame.pc)
                .and_then(|op| super::opcode::lookup(*op))
                .map_or("?", |op| op.mnemonic);
            lines.push(format!(
                "task {task} proc {} pc {} {op}",
                frame.proc, frame.pc
            ));
        }
        lines
    }

    /// Whether any task is still running.
    pub fn running(&self) -> bool {
        self.tasks.iter().any(|task| task.running)
    }

    fn run_task(&mut self, index: usize, host: &mut impl Host) {
        for _ in 0..MAX_STEPS {
            if !self.tasks[index].running {
                return;
            }
            match self.step(index, host) {
                Step::Next => {}
                Step::Yield => return,
                Step::False => {
                    let frame = &mut self.tasks[index].frame;
                    match frame.ifs.pop() {
                        Some(end) => frame.pc = end,
                        None => return,
                    }
                }
            }
        }
    }

    /// Evaluate the conditions of a `while` or `edwhile`: `size` bytes of
    /// them from the frame's `pc`, joined by a `u16` (0 and, else or).
    fn conditions(&mut self, index: usize, size: usize, host: &mut impl Host) -> bool {
        let end = self.tasks[index].frame.pc + size;
        let mut result = self.step(index, host) != Step::False;
        while self.tasks[index].running && self.tasks[index].frame.pc < end {
            let (proc, pc) = {
                let frame = &self.tasks[index].frame;
                (frame.proc, frame.pc)
            };
            let join = u16_at(&self.list()[proc], pc).unwrap_or(0);
            self.tasks[index].frame.pc = pc + 2;
            let next = self.step(index, host) != Step::False;
            result = if join == 0 {
                result && next
            } else {
                result || next
            };
        }
        result
    }

    /// Run the task's next instruction.
    fn step(&mut self, index: usize, host: &mut impl Host) -> Step {
        let (proc, pc) = {
            let frame = &self.tasks[index].frame;
            (frame.proc, frame.pc)
        };
        let Some(code) = self.list().get(proc) else {
            self.tasks[index].running = false;
            return Step::Yield;
        };
        let Some(op) = code.get(pc).and_then(|byte| opcode::lookup(*byte)) else {
            self.tasks[index].running = false;
            return Step::Yield;
        };
        if pc + op.width > code.len() {
            self.tasks[index].running = false;
            return Step::Yield;
        }
        let bytes = code[pc..pc + op.width].to_vec();
        let values = opcode::operands(op, &bytes);
        let byte = |at: usize| bytes[at];
        let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let next = pc + op.width;
        let frame = &mut self.tasks[index].frame;
        match op.op {
            0x01 => {
                // `evt_end`: back to the caller, or the task's end.
                self.leave(index);
                return if self.tasks[index].running {
                    Step::Next
                } else {
                    Step::Yield
                };
            }
            0x02 => {
                frame.pc = next;
                return Step::Yield;
            }
            0x03 => {
                let event = usize::from(byte(3));
                self.start(index, event);
                return Step::Next;
            }
            0x04 => {
                frame.pc = next;
                let (task, event) = (usize::from(byte(1)), usize::from(byte(3)));
                let slot = if task >= AUTO_TASK_ID {
                    AUTO_TASKS.clone().find(|slot| !self.tasks[*slot].running)
                } else {
                    Some(task)
                };
                if let Some(slot) = slot {
                    self.start(slot, event);
                }
                return if self.tasks[index].running {
                    Step::Next
                } else {
                    Step::Yield
                };
            }
            0x05 => {
                frame.pc = next;
                if let Some(task) = self.tasks.get_mut(usize::from(byte(1))) {
                    task.running = false;
                }
                return Step::Next;
            }
            0x06 => {
                frame.ifs.push(next + usize::from(word(2)));
                frame.pc = next;
            }
            0x07 => {
                frame.pc = pc + usize::from(word(2));
                frame.ifs.pop();
            }
            0x08 => {
                frame.ifs.pop();
                frame.pc = next;
            }
            0x09 => {
                // `sleep`'s bytes end with the `sleeping` that counts it down.
                frame.loops.push(Loop {
                    start: pc + 1,
                    end: pc + 1,
                    count: word(2),
                    if_depth: frame.ifs.len(),
                });
                frame.pc = pc + 1;
            }
            0x0A => {
                let done = frame.loops.last_mut().is_none_or(|top| {
                    top.count = top.count.saturating_sub(1);
                    top.count == 0
                });
                if done {
                    frame.loops.pop();
                    frame.pc = next;
                }
                return Step::Yield;
            }
            0x0B => {
                frame.loops.push(Loop {
                    start: next,
                    end: next,
                    count: 0,
                    if_depth: frame.ifs.len(),
                });
                frame.pc = next;
            }
            0x0C => {
                if !host.waiting() {
                    frame.loops.pop();
                    frame.pc = next;
                }
                return Step::Yield;
            }
            0x0D | 0x1B => {
                let count = if op.op == 0x0D {
                    word(4)
                } else {
                    host.var(byte(5))
                };
                let end = next + usize::from(word(2));
                if count == 0 {
                    frame.pc = end;
                } else {
                    frame.loops.push(Loop {
                        start: next,
                        end,
                        count,
                        if_depth: frame.ifs.len(),
                    });
                    frame.pc = next;
                }
            }
            0x0E => match frame.loops.last_mut() {
                Some(top) if top.count > 1 => {
                    top.count -= 1;
                    frame.pc = top.start;
                }
                _ => {
                    frame.loops.pop();
                    frame.pc = next;
                }
            },
            0x0F => {
                let end = next + usize::from(word(2));
                let size = usize::from(byte(1));
                frame.loops.push(Loop {
                    start: pc,
                    end,
                    count: 0,
                    if_depth: frame.ifs.len(),
                });
                frame.pc = next;
                if !self.conditions(index, size, host) {
                    let frame = &mut self.tasks[index].frame;
                    frame.pc = end;
                    frame.loops.pop();
                }
            }
            0x10 => {
                let top = frame.loops.pop();
                frame.pc = top.map_or(next, |top| top.start);
            }
            0x11 => {
                frame.loops.push(Loop {
                    start: next,
                    end: next + usize::from(word(2)),
                    count: 0,
                    if_depth: frame.ifs.len(),
                });
                frame.pc = next;
            }
            0x12 => {
                let size = usize::from(byte(1));
                frame.pc = next;
                if self.conditions(index, size, host) {
                    let frame = &mut self.tasks[index].frame;
                    frame.pc = frame.loops.last().map_or(next + size, |top| top.start);
                } else {
                    self.tasks[index].frame.loops.pop();
                }
            }
            0x13 => {
                let value = host.var(byte(1));
                let if_depth = frame.ifs.len();
                frame.loops.push(Loop {
                    start: next,
                    end: next + usize::from(word(2)),
                    count: 0,
                    if_depth,
                });
                let code = self.list()[proc].clone();
                let frame = &mut self.tasks[index].frame;
                let mut scan = next;
                loop {
                    match code.get(scan) {
                        Some(&opcode::DEFAULT) => {
                            frame.pc = scan + 2;
                            break;
                        }
                        Some(&opcode::CASE) => {
                            let matched = u16_at(&code, scan + 4) == Some(value);
                            if matched {
                                frame.pc = scan + 6;
                                break;
                            }
                            scan += 6 + usize::from(u16_at(&code, scan + 2).unwrap_or(0));
                        }
                        _ => {
                            // `eswitch`, or a table that runs out.
                            frame.loops.pop();
                            frame.pc = scan + 2;
                            break;
                        }
                    }
                }
            }
            0x14 | 0x15 => frame.pc = next,
            0x16 => {
                frame.loops.pop();
                frame.pc = next;
            }
            0x17 => {
                // The bytes count the depths the target stands at, 0xFF for
                // none.
                let depth = |value: u8| usize::from(value.wrapping_add(1));
                frame.ifs.truncate(depth(byte(1)));
                frame.loops.truncate(depth(byte(2)));
                let jump = i16::from_le_bytes([bytes[4], bytes[5]]) as isize;
                frame.pc = pc.wrapping_add_signed(jump);
            }
            0x18 => {
                frame.pc = next;
                if self.tasks[index].calls.len() >= MAX_DEPTH {
                    return Step::Next;
                }
                let target = usize::from(byte(1));
                let caller = std::mem::replace(
                    &mut self.tasks[index].frame,
                    Frame {
                        proc: target,
                        ..Frame::default()
                    },
                );
                self.tasks[index].calls.push(caller);
            }
            0x19 => {
                self.leave(index);
                if !self.tasks[index].running {
                    return Step::Yield;
                }
            }
            0x1A => {
                if let Some(top) = frame.loops.pop() {
                    frame.ifs.truncate(top.if_depth);
                    frame.pc = top.end;
                } else {
                    frame.pc = next;
                }
            }
            0x1D => {
                // `work_copy` writes a variable into the instruction stream.
                frame.pc = next;
                let value = host.var(byte(1)).to_le_bytes();
                let at = next + usize::from(byte(2));
                let wide = byte(3) != 0;
                let list = match self.active {
                    Active::Init => &mut self.init,
                    Active::Main => &mut self.main,
                };
                if let Some(code) = list.get_mut(proc) {
                    if let Some(slot) = code.get_mut(at) {
                        *slot = value[0];
                    }
                    if wide && let Some(slot) = code.get_mut(at + 1) {
                        *slot = value[1];
                    }
                }
            }
            0x21 => {
                frame.pc = next;
                let on = host.flag(byte(1), byte(2));
                return if on == (byte(3) != 0) {
                    Step::Next
                } else {
                    Step::False
                };
            }
            0x22 => {
                frame.pc = next;
                host.set_flag(byte(1), byte(2), byte(3));
            }
            0x23 => {
                frame.pc = next;
                let left = i32::from(host.var(byte(2)) as i16);
                let right = i32::from(i16::from_le_bytes([bytes[4], bytes[5]]));
                let result = match byte(3) {
                    0 => left == right,
                    1 => left > right,
                    2 => left >= right,
                    3 => left < right,
                    4 => left <= right,
                    5 => left != right,
                    6 => host.var(byte(2)) & word(4) != 0,
                    _ => left != 0,
                };
                return if result { Step::Next } else { Step::False };
            }
            0x24 => {
                frame.pc = next;
                host.set_var(byte(1), word(2));
            }
            0x25 => {
                frame.pc = next;
                let value = host.var(byte(2));
                host.set_var(byte(1), value);
            }
            0x26 => {
                frame.pc = next;
                calc(
                    host,
                    byte(2),
                    byte(3),
                    i16::from_le_bytes([bytes[4], bytes[5]]),
                );
            }
            0x27 => {
                frame.pc = next;
                let value = host.var(byte(3)) as i16;
                calc(host, byte(1), byte(2), value);
            }
            0x28 => {
                frame.pc = next;
                let value = host.random();
                host.set_var(RANDOM_VAR, value);
            }
            _ => {
                frame.pc = next;
                return host.exec(index, op, &values);
            }
        }
        Step::Next
    }

    /// `return`/`evt_end`: back to the caller's frame, or stop the task.
    fn leave(&mut self, index: usize) {
        let task = &mut self.tasks[index];
        match task.calls.pop() {
            Some(caller) => task.frame = caller,
            None => task.running = false,
        }
    }
}

/// The script variable `sce_rnd` writes.
const RANDOM_VAR: u8 = 29;

/// `Calc_branch`.
fn calc(host: &mut impl Host, operation: u8, index: u8, value: i16) {
    let old = host.var(index);
    let signed = old as i16;
    let shift = u32::from(value as u8 & 0x1F);
    let new = match operation {
        0 => old.wrapping_add(value as u16),
        1 => old.wrapping_sub(value as u16),
        2 => old.wrapping_mul(value as u16),
        3 if value != 0 => signed.wrapping_div(value) as u16,
        4 if value != 0 => signed.wrapping_rem(value) as u16,
        5 => old | value as u16,
        6 => old & value as u16,
        7 => old ^ value as u16,
        8 => !old,
        9 => old.wrapping_shl(shift),
        10 => old.wrapping_shr(shift),
        11 => signed.wrapping_shr(shift) as u16,
        _ => old,
    };
    host.set_var(index, new);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestHost {
        flags: std::collections::HashSet<(u8, u8)>,
        vars: std::collections::HashMap<u8, u16>,
        executed: Vec<(&'static str, Vec<i64>)>,
        waiting: bool,
    }

    impl Host for TestHost {
        fn flag(&self, bank: u8, index: u8) -> bool {
            self.flags.contains(&(bank, index))
        }
        fn set_flag(&mut self, bank: u8, index: u8, op: u8) {
            match op {
                0 => {
                    self.flags.remove(&(bank, index));
                }
                1 => {
                    self.flags.insert((bank, index));
                }
                _ => {
                    if !self.flags.remove(&(bank, index)) {
                        self.flags.insert((bank, index));
                    }
                }
            }
        }
        fn var(&self, index: u8) -> u16 {
            self.vars.get(&index).copied().unwrap_or(0)
        }
        fn set_var(&mut self, index: u8, value: u16) {
            self.vars.insert(index, value);
        }
        fn waiting(&self) -> bool {
            self.waiting
        }
        fn random(&mut self) -> u16 {
            7
        }
        fn exec(&mut self, _: usize, op: &'static Op, operands: &[i64]) -> Step {
            self.executed.push((op.mnemonic, operands.to_vec()));
            Step::Next
        }
    }

    /// A procedure assembled byte by byte: the script text syntax is covered
    /// by `akscr`'s own tests, and these bytes pin the VM's jump arithmetic.
    fn vm(main: Vec<Vec<u8>>) -> Vm {
        Vm {
            init: Vec::new(),
            main,
            active: Active::Main,
            tasks: std::array::from_fn(|_| Task::default()),
        }
    }

    const MARK_1: [u8; 2] = [0x29, 1]; // cut_chg 1
    const MARK_2: [u8; 2] = [0x29, 2];
    const END: [u8; 2] = [0x01, 0];

    fn marks(host: &TestHost) -> Vec<i64> {
        host.executed.iter().map(|(_, ops)| ops[0]).collect()
    }

    fn run(code: Vec<u8>, host: &mut TestHost) -> Vm {
        let mut vm = vm(vec![code]);
        vm.start_main();
        vm.tick(host);
        vm
    }

    fn concat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn a_failed_ck_jumps_to_the_ifs_else() {
        // if { ck 0,1,1; mark1; else { mark2 } } end
        let code = concat(&[
            &[0x06, 0, 10, 0],
            &[0x21, 0, 1, 1],
            &MARK_1,
            &[0x07, 0, 6, 0],
            &MARK_2,
            &[0x08, 0],
            &END,
        ]);
        let mut host = TestHost::default();
        run(code.clone(), &mut host);
        assert_eq!(marks(&host), [2]);

        let mut host = TestHost::default();
        host.flags.insert((0, 1));
        run(code, &mut host);
        assert_eq!(marks(&host), [1]);
    }

    #[test]
    fn a_failed_condition_outside_an_if_ends_the_tick_past_it() {
        let code = concat(&[&[0x21, 0, 1, 1], &MARK_1, &END]);
        let mut host = TestHost::default();
        let mut vm = vm(vec![code]);
        vm.start_main();
        vm.tick(&mut host);
        assert!(marks(&host).is_empty());
        vm.tick(&mut host);
        assert_eq!(marks(&host), [1]);
    }

    #[test]
    fn sleep_counts_down_in_ticks() {
        let code = concat(&[&[0x09, 0x0A, 2, 0], &MARK_1, &END]);
        let mut host = TestHost::default();
        let mut vm = vm(vec![code]);
        vm.start_main();
        vm.tick(&mut host);
        assert!(marks(&host).is_empty());
        vm.tick(&mut host);
        assert!(marks(&host).is_empty());
        vm.tick(&mut host);
        assert_eq!(marks(&host), [1]);
        assert!(!vm.running());
    }

    #[test]
    fn for_runs_its_body_count_times() {
        let code = concat(&[&[0x0D, 0, 2, 0, 3, 0], &MARK_1, &[0x0E, 0], &END]);
        let mut host = TestHost::default();
        run(code, &mut host);
        assert_eq!(marks(&host), [1, 1, 1]);
    }

    #[test]
    fn switch_picks_the_matching_case() {
        // switch var5 { case 1: mark1 break; case 2: mark2 break; }
        let body = concat(&[
            &[0x14, 0, 4, 0, 1, 0],
            &MARK_1,
            &[0x1A, 0],
            &[0x14, 0, 4, 0, 2, 0],
            &MARK_2,
            &[0x1A, 0],
            &[0x16, 0],
        ]);
        let code = concat(&[&[0x13, 5, body.len() as u8, 0], &body, &END]);
        let mut host = TestHost::default();
        host.vars.insert(5, 2);
        let vm = run(code, &mut host);
        assert_eq!(marks(&host), [2]);
        assert!(!vm.running());
    }

    #[test]
    fn gosub_returns_to_its_caller() {
        let main = concat(&[&[0x18, 1], &MARK_1, &END]);
        let sub = concat(&[&MARK_2, &[0x19, 0]]);
        let mut host = TestHost::default();
        let mut vm = vm(vec![main, sub]);
        vm.start_main();
        vm.tick(&mut host);
        assert_eq!(marks(&host), [2, 1]);
    }

    #[test]
    fn evt_exec_starts_another_task_in_the_same_tick() {
        let main = concat(&[&[0x04, 3, 0, 1], &END]);
        let other = concat(&[&MARK_1, &END]);
        let mut host = TestHost::default();
        let mut vm = vm(vec![main, other]);
        vm.start_main();
        vm.tick(&mut host);
        assert_eq!(marks(&host), [1]);
    }

    #[test]
    fn set_cmp_save_and_calc_drive_variables_and_flags() {
        let code = concat(&[
            &[0x24, 4, 5, 0],       // save var4 = 5
            &[0x26, 0, 0, 4, 3, 0], // calc var4 += 3
            &[0x23, 0, 4, 0, 8, 0], // cmp var4 == 8
            &[0x22, 3, 9, 1],       // set bank 3 bit 9
            &END,
        ]);
        let mut host = TestHost::default();
        run(code, &mut host);
        assert_eq!(host.var(4), 8);
        assert!(host.flag(3, 9));
    }

    #[test]
    fn a_spinning_script_costs_only_its_tick() {
        // do { } edwhile (var0 == 0): never leaves, and must not hang.
        let code = concat(&[&[0x11, 0, 8, 0], &[0x12, 6], &[0x23, 0, 0, 0, 0, 0], &END]);
        let mut host = TestHost::default();
        let mut vm = vm(vec![code]);
        vm.start_main();
        vm.tick(&mut host);
        assert!(vm.running());
    }
}
