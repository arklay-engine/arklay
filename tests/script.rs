//! `arklay script`: the Lua driver-script suite.
//!
//! Every test spawns the built binary with `--headless` against synthetic
//! packs in temporary directories: no game assets, no display and no
//! network.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use arklay::pack::PackWriter;
use arklay::state::{Image, RoomId};

/// Self-deleting temporary directory unique to the test's label.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("arklay-script-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The binary with no user config, so a developer's `arklay.toml` cannot
/// change a run.
fn arklay(dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_arklay"));
    command.current_dir(&dir.path);
    command.env("ARKLAY_CONFIG_DIR", dir.join("no-config"));
    command
}

/// Run `source` as a headless script read from stdin, with `args` after it.
fn script(dir: &TempDir, source: &str, args: &[&str]) -> Output {
    let mut child = arklay(dir)
        .args(["script", "--headless", "-"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the arklay binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Assert a successful run and return its stdout.
fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "the script failed ({:?}):\n{}",
        output.status.code(),
        stderr(output)
    );
    stdout(output)
}

/// Assert a run that failed with status 1, `needle` in stderr and no panic.
fn assert_failure(output: &Output, needle: &str) {
    let err = stderr(output);
    assert_eq!(output.status.code(), Some(1), "stderr:\n{err}");
    assert!(err.contains(needle), "stderr lacks `{needle}`:\n{err}");
    assert!(!err.contains("panicked at"), "panic leaked:\n{err}");
}

/// A 320x240 RGBA image so the renderer always has a full background.
fn background() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(320 * 240 * 4);
    for y in 0..240u32 {
        for x in 0..320u32 {
            let v = ((x * 3 + y) % 256) as u8;
            rgba.extend_from_slice(&[v, v ^ 0x55, v.wrapping_mul(37), 255]);
        }
    }
    arklay::bmp::encode_to_vec(&Image {
        width: 320,
        height: 240,
        rgba,
    })
    .unwrap()
}

/// The smallest RDT the room loader accepts: one camera cut, empty scripts.
fn synthetic_rdt() -> Vec<u8> {
    let mut rdt = vec![0u8; 0x94];
    rdt[0x01] = 1;
    rdt.extend_from_slice(&[0u8; 44]);
    rdt
}

/// A synthetic one-room pack (room 100) plus `extra` entries.
fn write_pack(path: &Path, extra: &[(&str, &[u8])]) {
    let id = RoomId::parse("1000").unwrap();
    let mut writer = PackWriter::new();
    writer.add(&id.rdt_entry(), synthetic_rdt()).unwrap();
    writer.add(&id.cut_entry(0), background()).unwrap();
    for (entry, data) in extra {
        writer.add(entry, data.to_vec()).unwrap();
    }
    writer.write(path).unwrap();
}

/// `source` for a run on the synthetic pack: it has no player model, so
/// booting it raises engine warnings a strict run fails on.
fn lenient(source: &str) -> String {
    format!(
        "strict(false)
{source}"
    )
}

/// A temporary directory holding `game.akpak`, the synthetic room pack.
fn pack_dir(label: &str) -> TempDir {
    let dir = TempDir::new(label);
    write_pack(&dir.join("game.akpak"), &[]);
    dir
}

#[test]
fn print_and_json_are_the_only_stdout() {
    let dir = pack_dir("stdout");
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        tick(5)
        print("room", game:room())
        print(json({ frame = game:frame(), items = game:inventory(), ok = true }))
        "#,
        ),
        &[],
    ));
    assert_eq!(
        out, "room\t100\n{\"frame\":5,\"items\":[],\"ok\":true}\n",
        "stdout carries exactly what the script printed"
    );
}

#[test]
fn a_script_file_receives_its_arguments() {
    let dir = TempDir::new("args");
    std::fs::write(
        dir.join("args.lua"),
        "print(arg[0], arg[1], arg[2], #arg)\n",
    )
    .unwrap();
    let output = arklay(&dir)
        .args(["script", "--headless", "args.lua", "one", "--two"])
        .output()
        .unwrap();
    assert_eq!(success(&output), "args.lua\tone\t--two\t2\n");
}

#[test]
fn exit_ends_the_run_with_its_status_even_under_pcall() {
    let dir = TempDir::new("exit");
    let output = script(&dir, "print('before') pcall(exit, 3) print('after')", &[]);
    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert_eq!(stdout(&output), "before\n");
}

#[test]
fn a_lua_error_fails_with_its_line() {
    let dir = TempDir::new("lua-error");
    let output = script(&dir, "local t = nil\nprint(t.x)\n", &[]);
    assert_failure(&output, "stdin:2:");
}

#[test]
fn engine_calls_fail_with_a_message() {
    let dir = pack_dir("engine-errors");
    assert_failure(&script(&dir, "tick()", &[]), "no game pack is open");
    assert_failure(
        &script(&dir, "load_game_pack('missing.akpak')", &[]),
        "missing.akpak",
    );
    assert_failure(
        &script(&dir, "load_game_pack('game.akpak') tick()", &[]),
        "nothing is booted",
    );
    assert_failure(
        &script(&dir, "load_game_pack('game.akpak') load_room('zz')", &[]),
        "load_room",
    );
    assert_failure(
        &script(
            &dir,
            "load_game_pack('game.akpak', { qiuck_turn = true })",
            &[],
        ),
        "unknown option `qiuck_turn`",
    );
    assert_failure(
        &script(&dir, "load_game_pack('game.akpak') hold('jump')", &[]),
        "unknown button `jump`",
    );
    assert_failure(
        &script(
            &dir,
            "strict(false) load_game_pack('game.akpak') load_room('100') game:fly()",
            &[],
        ),
        "game has no method `fly`",
    );
}

#[test]
fn the_sandbox_has_no_io_or_os() {
    let dir = TempDir::new("sandbox");
    let out = success(&script(
        &dir,
        "print(io, os, package, dofile, load, math.floor(2.5), utf8 ~= nil)",
        &[],
    ));
    assert_eq!(out, "nil\tnil\tnil\tnil\tnil\t2\ttrue\n");
}

#[test]
fn a_timeout_stops_a_runaway_script() {
    let dir = TempDir::new("timeout");
    let output = arklay(&dir)
        .args(["script", "--headless", "--timeout", "0.5", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"while true do end")?;
            child.wait_with_output()
        })
        .unwrap();
    assert_failure(&output, "timed out");
}

#[test]
fn a_script_trace_matches_the_cli_trace() {
    let dir = pack_dir("trace");
    let cli = arklay(&dir)
        .args([
            "game.akpak",
            "--ui",
            "game",
            "--room",
            "100",
            "--ticks",
            "30",
            "--trace",
            "cli.trace",
        ])
        .output()
        .unwrap();
    success(&cli);
    success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        new_game{ room = "100" }
        trace_start("script.trace")
        tick(30)
        "#,
        ),
        &[],
    ));
    let cli = std::fs::read(dir.join("cli.trace")).unwrap();
    let ours = std::fs::read(dir.join("script.trace")).unwrap();
    assert!(cli.len() > 100, "the CLI wrote a trace");
    assert!(
        cli == ours,
        "the script's trace differs from `--ui game --trace`"
    );
}

#[test]
fn scripted_input_matches_an_input_file() {
    let dir = pack_dir("input");
    std::fs::write(dir.join("input.txt"), "0 up run\n10 left\n20\n").unwrap();
    success(
        &arklay(&dir)
            .args([
                "game.akpak",
                "--ui",
                "game",
                "--room",
                "100",
                "--ticks",
                "30",
                "--input",
                "input.txt",
                "--trace",
                "cli.trace",
            ])
            .output()
            .unwrap(),
    );
    success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        new_game{ room = "100" }
        trace_start("script.trace", { all_frames = true })
        hold("up", "run")
        tick(10)
        release()
        hold("left")
        tick(10)
        release()
        tick(10)
        "#,
        ),
        &[],
    ));
    assert!(
        std::fs::read(dir.join("cli.trace")).unwrap()
            == std::fs::read(dir.join("script.trace")).unwrap(),
        "held buttons and an --input file drive the same ticks"
    );
}

#[test]
fn game_edits_reach_the_session_and_the_save() {
    let dir = pack_dir("edits");
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        game:set_player_pos(1200, 0, -800)
        game:give_item(2, 15)
        game:flag_set(0, 3)
        tick(2)
        local x, y, z = game:player_pos()
        print(x, y, z, game:item_count(2), game:flag_get(0, 3))
        save_game("out.dat")
        load_save("out.dat")
        print(game:item_count(2), game:flag_get(0, 3))
        "#,
        ),
        &[],
    ));
    assert_eq!(out, "1200\t0\t-800\t15\ttrue\n15\ttrue\n");
}

#[test]
fn tick_until_counts_and_gives_up() {
    let dir = pack_dir("tick-until");
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        print(tick_until(function() return game:frame() >= 7 end, 100))
        print(tick_until(function() return true end, 0))
        print(pcall(tick_until, function() return false end, 3))
        "#,
        ),
        &[],
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "7");
    assert_eq!(lines[1], "0");
    assert!(
        lines[2].starts_with("false") && lines[2].contains("within 3 ticks"),
        "{out}"
    );
}

#[test]
fn capture_writes_a_png_or_a_bmp() {
    let dir = pack_dir("capture");
    success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        capture("boot.png")
        tick(3)
        capture("after.bmp")
        "#,
        ),
        &[],
    ));
    let png = std::fs::read(dir.join("boot.png")).unwrap();
    assert_eq!(&png[..4], b"\x89PNG");
    let bmp = std::fs::read(dir.join("after.bmp")).unwrap();
    assert_eq!(&bmp[..2], b"BM");
}

#[test]
fn an_engine_warning_fails_a_strict_run() {
    let dir = TempDir::new("strict");
    // A mod hook that fails on its first tick: the engine disables it with a
    // `[lua]` warning and the game carries on.
    write_pack(
        &dir.join("game.akpak"),
        &[("lua/broken.lua", b"function on_tick(api) error('boom') end")],
    );
    // The boot's own warnings (the pack has no player model) are let
    // through; strict mode then catches the hook's.
    let boot = "strict(false) load_game_pack('game.akpak') load_room('100')";
    let output = script(&dir, &format!("{boot} strict() tick(3)"), &[]);
    assert_failure(&output, "boom");
    assert_failure(&output, "strict(false)");
    let out = success(&script(&dir, &format!("{boot} tick(3) print('done')"), &[]));
    assert_eq!(out, "done\n");
}

#[test]
fn a_pcall_cannot_outlast_the_timeout() {
    let dir = TempDir::new("pcall-timeout");
    let output = arklay(&dir)
        .args(["script", "--headless", "--timeout", "0.3", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(
                b"for _ = 1, 3 do pcall(function() while true do end end) end print('survived')",
            )?;
            child.wait_with_output()
        })
        .unwrap();
    assert_failure(&output, "timed out");
    assert_eq!(stdout(&output), "");
}

#[test]
fn exit_works_from_a_callback_and_checks_its_status() {
    let dir = pack_dir("exit-callback");
    let output = script(
        &dir,
        &lenient(
            "load_game_pack('game.akpak') load_room('100') \
             tick_until(function() exit(5) end, 10)",
        ),
        &[],
    );
    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    // Inside an engine call (an options table's metamethod) too.
    let output = script(
        &dir,
        "load_game_pack('game.akpak', setmetatable({}, { __index = function() exit(6) end }))",
        &[],
    );
    assert_eq!(output.status.code(), Some(6), "{}", stderr(&output));
    assert!(!stderr(&output).contains("panicked"));
    assert_failure(&script(&dir, "exit(256)", &[]), "0..=255");
}

#[test]
fn game_methods_need_a_colon() {
    let dir = pack_dir("dot-call");
    let output = script(
        &dir,
        &lenient("load_game_pack('game.akpak') load_room('100') game.give_item(2, 15)"),
        &[],
    );
    assert_failure(&output, "game:give_item(...)");
}

#[test]
fn mode_pad_and_trace_restarts() {
    let dir = pack_dir("mode-pad-trace");
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        print(mode())
        load_game_pack("game.akpak")
        print(mode())
        load_room("100")
        print(mode())
        print(pad(5), pad(), pad(0))
        trace_start("a.trace")
        tick(2)
        trace_start("b.trace")
        tick(3)
        trace_stop()
        tick(4)
        "#,
        ),
        &[],
    ));
    assert_eq!(out, "none\nnone\nplay\n0\t5\t5\n");
    let lines = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .unwrap()
            .lines()
            .count()
    };
    // Each trace has its header and one line per tick it was running.
    assert_eq!(lines("a.trace"), 3);
    assert_eq!(lines("b.trace"), 4);
}

#[test]
fn presses_match_an_input_file() {
    let dir = pack_dir("press");
    std::fs::write(dir.join("input.txt"), "0 confirm\n1\n2 confirm\n3\n").unwrap();
    success(
        &arklay(&dir)
            .args([
                "game.akpak",
                "--ui",
                "game",
                "--room",
                "100",
                "--ticks",
                "6",
                "--input",
                "input.txt",
                "--trace",
                "cli.trace",
            ])
            .output()
            .unwrap(),
    );
    success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        new_game{ room = "100" }
        trace_start("script.trace", { all_frames = true })
        press("confirm") tick(2)
        press("confirm") tick(4)
        "#,
        ),
        &[],
    ));
    assert!(
        std::fs::read(dir.join("cli.trace")).unwrap()
            == std::fs::read(dir.join("script.trace")).unwrap(),
        "a press holds for one tick"
    );
}

/// A bot that walks nowhere and ends its run on its fifth tick.
const FIVE_TICK_BOT: &[u8] = b"function on_bot_tick(bot)
    if bot:tick() >= 5 then
        bot:finish('five ticks')
    end
end
";

#[test]
fn bot_run_plays_the_packs_bot_until_it_finishes() {
    let dir = TempDir::new("bot-run");
    write_pack(&dir.join("game.akpak"), &[("bot/init.lua", FIVE_TICK_BOT)]);
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        local run = bot_run({ max_ticks = 50, stats = "stats.json", log = false })
        local finished, reason = bot_finished()
        print(run.outcome, run.finish_reason, run.ticks, run.score.completed, finished, reason)
        print(bot_stats().score.ticks, run.score.hits, run.score.deaths, run.score.room_loads)
        print(bot_story(1, 0x4B), #run.story.common)
        "#,
        ),
        &[],
    ));
    assert_eq!(
        out,
        "finished\tfive ticks\t5\tfalse\ttrue\tfive ticks\n5\t0\t0\t0\nfalse\t0\n"
    );
    let stats = std::fs::read_to_string(dir.join("stats.json")).unwrap();
    assert!(stats.contains("\"outcome\": \"finished\""), "{stats}");
}

#[test]
fn bot_start_attaches_the_bot_for_the_scripts_own_ticks() {
    let dir = TempDir::new("bot-start");
    write_pack(&dir.join("game.akpak"), &[("bot/init.lua", FIVE_TICK_BOT)]);
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        bot_start({ seed = 7, risk = 0.5, log = false })
        tick(2)
        print(bot_finished())
        local ticks = tick_until(function() return (bot_finished()) end, 20)
        print(ticks, select(2, bot_finished()))
        "#,
        ),
        &[],
    ));
    assert_eq!(out, "false\tnil\n3\tfive ticks\n");
}

#[test]
fn bot_run_stops_at_its_tick_budget() {
    let dir = TempDir::new("bot-budget");
    write_pack(&dir.join("game.akpak"), &[("bot/init.lua", FIVE_TICK_BOT)]);
    let out = success(&script(
        &dir,
        &lenient(
            r#"
        load_game_pack("game.akpak")
        load_room("100")
        local run = bot_run({ max_ticks = 3, log = false })
        print(run.outcome, run.ticks)
        "#,
        ),
        &[],
    ));
    assert_eq!(out, "max_ticks\t3\n");
}

#[test]
fn the_bot_calls_fail_with_a_message() {
    let dir = TempDir::new("bot-errors");
    write_pack(&dir.join("game.akpak"), &[("bot/init.lua", FIVE_TICK_BOT)]);
    write_pack(&dir.join("plain.akpak"), &[]);
    for (source, needle) in [
        ("bot_stats()", "no game pack is open"),
        (
            "load_game_pack('game.akpak') bot_start()",
            "no game session is running",
        ),
        (
            "load_game_pack('game.akpak') load_room('100') bot_stats()",
            "no bot is attached",
        ),
        (
            "load_game_pack('game.akpak') load_room('100') bot_run({ ticks = 3 })",
            "unknown option `ticks`",
        ),
        (
            "load_game_pack('game.akpak') load_room('100') bot_start({ risk = 2 })",
            "risk 2 is not within 0..=1",
        ),
        (
            "load_game_pack('game.akpak') load_room('100') bot_start({ params = { richard = 'leave' } })",
            "richard",
        ),
        (
            "load_game_pack('plain.akpak') load_room('100') bot_start()",
            "bot",
        ),
    ] {
        assert_failure(&script(&dir, &lenient(source), &[]), needle);
    }
}
