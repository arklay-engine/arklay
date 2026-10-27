use super::*;

use crate::pack::Pack;
use crate::save;
use crate::save::SaveFile;
use crate::text::Text;

/// Self-deleting temporary directory unique to this process and label.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("arklay-lua-screen-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A 4x4 RGBA image whose top-left pixel is red, encoded as a BMP.
fn test_bmp() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(4 * 4 * 4);
    for index in 0..16 {
        if index == 0 {
            rgba.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            rgba.extend_from_slice(&[0, 255, 0, 255]);
        }
    }
    crate::bmp::encode_to_vec(&crate::state::Image {
        width: 4,
        height: 4,
        rgba,
    })
    .unwrap()
}

/// A 320x240 RGBA PNG, blue with one half-transparent pixel, so the
/// screen's exact-size background blit can be told from the stretch path
/// that forces opaque alpha.
#[cfg(feature = "hd")]
fn full_png() -> Vec<u8> {
    let (width, height) = (320u32, 240u32);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[0, 0, 255, 255]);
    }
    let index = (100 * width as usize + 100) * 4;
    rgba[index + 3] = 128;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }
    bytes
}

/// A 4x4 4bpp TIM whose palette index 1 is blue.
fn test_tim() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x10u32.to_le_bytes());
    bytes.extend_from_slice(&0x08u32.to_le_bytes());
    bytes.extend_from_slice(&44u32.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    for entry in 0..16u16 {
        let word = if entry == 1 { 0x7C00u16 } else { 0x0000 };
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&28u32.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    for _ in 0..8u32 {
        bytes.extend_from_slice(&0x1111u16.to_le_bytes());
    }
    bytes
}

/// Build a pack with `screen/save.lua` (the source), a language file and
/// the two test images.
fn pack(source: &str) -> &'static Pack {
    use crate::pack::PackWriter;

    let mut writer = PackWriter::new();
    writer
        .add("screen/save.lua", source.as_bytes().to_vec())
        .unwrap();
    writer.add("art/bg.bmp", test_bmp()).unwrap();
    writer.add("art/dot.tim", test_tim()).unwrap();
    #[cfg(feature = "hd")]
    writer.add("art/full.png", full_png()).unwrap();
    writer
        .add(
            "language/en-US.txt",
            b"UI_SAVE_CR_SAVE = \"Save\"\nROOM_100_NAME_SAVE = \"M. Room 1F\"\n".to_vec(),
        )
        .unwrap();
    let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
    Box::leak(Box::new(pack))
}

/// A synthetic font sheet: every cell is a solid block, so a drawn glyph
/// is measurable without shipping art in the test pack.
fn test_font() -> crate::font::Font {
    let (width, height) = (256u32, 224u32);
    let texture = crate::model::Texture8 {
        width,
        height,
        indices: vec![1; (width * height) as usize],
        palettes: (0..256).map(|index| [index as u8, 0, 0, 255]).collect(),
        stp: vec![false; 256],
    };
    crate::font::Font::new(texture)
}

/// The shared test setup: a pack, the text tables, a font and a temp save
/// directory.
struct Fixture {
    pack: &'static Pack,
    text: Text,
    font: crate::font::Font,
    dir: TempDir,
}

impl Fixture {
    fn new(label: &str, source: &str) -> Self {
        let pack = pack(source);
        let text = Text::load(pack, Some("en-US"));
        Self {
            pack,
            text,
            font: test_font(),
            dir: TempDir::new(label),
        }
    }

    fn context(&self) -> crate::ui::UiContext<'_> {
        crate::ui::UiContext {
            authentic: false,
            pack: self.pack,
            save_dir: &self.dir.path,
            font: Some(&self.font),
            text: Some(&self.text),
            ticks: 0,
            cues: std::cell::RefCell::new(Vec::new()),
            game: None,
        }
    }

    fn load(&self) -> LuaScreen {
        let mut cx = self.context();
        LuaScreen::open_load("screen/save.lua", &mut cx).unwrap()
    }

    fn save(&self, file: SaveFile, ink_ribbon: bool) -> LuaScreen {
        let mut cx = self.context();
        LuaScreen::open_save("screen/save.lua", &mut cx, file, ink_ribbon).unwrap()
    }
}

use crate::ui::{Screen, ScreenAction, ScreenResult, UiInput};

fn draw(screen: &mut LuaScreen, cx: &crate::ui::UiContext<'_>) -> crate::render::Framebuffer {
    let mut framebuffer = crate::render::Framebuffer::new();
    screen.draw(cx, &mut framebuffer);
    framebuffer
}

#[test]
fn the_script_draws_rects_and_text() {
    let fixture = Fixture::new(
        "draw",
        r##"
function update(api, input) end

function draw(api)
api:rect(0, 0, 320, 240, 0x204060)
api:rect(10, 10, 20, 20, "#FF0000", 128)
api:text(8, 8, "Hello " .. api:string("UI_SAVE_CR_SAVE"))
width = api:text_width("Hello")
api:fade(64)
end
"##,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let framebuffer = draw(&mut screen, &cx);
    assert_eq!(&framebuffer.rgba[..4], &[0x20, 0x40, 0x60, 255]);
    let pixel = |x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        [
            framebuffer.rgba[index],
            framebuffer.rgba[index + 1],
            framebuffer.rgba[index + 2],
        ]
    };
    assert_ne!(pixel(15, 15), pixel(60, 60), "the translucent rect blended");
    let text_pixels = (0..30)
        .flat_map(|y| (0..160).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(x, y) != [0x20, 0x40, 0x60])
        .count();
    assert!(text_pixels > 20, "the text drew {text_pixels} pixels");
    assert!(
        screen.test_global::<i32>("width") > 0,
        "text_width measures"
    );
    assert_eq!(screen.fade(), 64);
}

#[test]
fn sprites_and_backgrounds_draw_from_pack_images() {
    let fixture = Fixture::new(
        "sprites",
        r##"
function update(api, input) end

function draw(api)
api:background("art/bg.bmp")
api:sprite("art/dot.tim", 10, 20)
api:sprite_region("art/dot.tim", 0, 0, 2, 2, 40, 40, 8, 8)
size_w, size_h = api:image_size("art/dot.tim")
api:sprite("art/none.bmp", 0, 0)
end
"##,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let framebuffer = draw(&mut screen, &cx);
    let pixel = |x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        [
            framebuffer.rgba[index],
            framebuffer.rgba[index + 1],
            framebuffer.rgba[index + 2],
        ]
    };
    // The 4x4 BMP stretched over the screen keeps its top-left red.
    assert_eq!(pixel(0, 0), [255, 0, 0]);
    // The TIM sprite drew its blue palette at (10, 20).
    assert_eq!(pixel(11, 21), [0, 0, 255]);
    // The stretched background's second column is green there.
    assert_eq!(pixel(100, 100), [0, 255, 0]);
    // The scaled sub-region drew blue at (40, 40) and (47, 47).
    assert_eq!(pixel(40, 40), [0, 0, 255]);
    assert_eq!(pixel(47, 47), [0, 0, 255]);
    assert_eq!(screen.test_global::<u32>("size_w"), 8);
    assert_eq!(screen.test_global::<u32>("size_h"), 4);
}

#[test]
fn a_missing_image_leaves_the_screen() {
    let fixture = Fixture::new(
        "no-image",
        r#"
function update(api, input) end
function draw(api)
api:sprite("art/none.bmp", 0, 0)
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let _ = draw(&mut screen, &cx);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::Title)
    );
}

#[test]
fn saves_lists_the_directory_with_parsed_metadata() {
    let fixture = Fixture::new(
        "list",
        r#"
function update(api)
local list = api:saves()
count = #list
first = list[1]
second = list[2]
end
"#,
    );
    let file = SaveFile {
        character: 1,
        saves: 7,
        stage: 0,
        room: 0x00,
        ..SaveFile::default()
    };
    save::save_to_path(&fixture.dir.path.join("beta.dat"), &file).unwrap();
    std::fs::write(fixture.dir.path.join("notes.txt"), b"junk").unwrap();

    let mut screen = fixture.load();
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    assert_eq!(screen.test_global::<i32>("count"), 2);
    let first: mlua::Table = screen.test_global("first");
    assert_eq!(first.get::<String>("name").unwrap(), "beta.dat");
    assert_eq!(first.get::<String>("base").unwrap(), "beta");
    assert!(first.get::<bool>("loadable").unwrap());
    assert_eq!(first.get::<u8>("character").unwrap(), 1);
    assert_eq!(first.get::<u8>("saves").unwrap(), 7);
    assert_eq!(first.get::<u8>("stage").unwrap(), 0);
    assert_eq!(first.get::<String>("location").unwrap(), "M. Room 1F");
    assert!(first.get::<f64>("timestamp").unwrap() > 0.0);
    assert_eq!(first.get::<Option<usize>>("autosave").unwrap(), None);
    let second: mlua::Table = screen.test_global("second");
    assert_eq!(second.get::<String>("name").unwrap(), "notes.txt");
    assert!(!second.get::<bool>("loadable").unwrap());
}

#[test]
fn save_writes_the_pending_block_and_records_the_outcome() {
    let fixture = Fixture::new(
        "save",
        r#"function update(api) ok, err = api:save("new_01.dat") end"#,
    );
    let mut file = SaveFile {
        character: 0,
        saves: 3,
        total_held: 1,
        ..SaveFile::default()
    };
    file.player_slots[0] = crate::game::InventoryItem {
        id: crate::items::ITEM_INK_RIBBONS,
        quantity: 2,
    };

    let mut screen = fixture.save(file, true);
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    assert!(screen.test_global::<bool>("ok"));
    assert_eq!(screen.test_global::<Option<String>>("err"), None);

    // The written block consumed one ribbon.
    let written = save::load_file(&fixture.dir.path.join("new_01.dat")).unwrap();
    assert_eq!(written.player_slots[0].quantity, 1);
    assert_eq!(written.used_item, crate::items::ITEM_INK_RIBBONS);
    assert_eq!(
        screen.take_outcome(),
        Some(crate::ui::save_load::SaveOutcome {
            slot: None,
            ink_ribbon: true
        })
    );

    // The scan saw the new file.
    let names: Vec<String> = fixture
        .dir
        .path
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, ["new_01.dat"]);
}

#[test]
fn save_rejects_bad_names_and_the_wrong_mode() {
    let fixture = Fixture::new(
        "save-bad",
        r#"
function update(api)
ok1, err1 = api:save("../escape.dat")
ok2, err2 = api:save("")
end
"#,
    );
    let mut screen = fixture.save(SaveFile::default(), false);
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    assert!(!screen.test_global::<bool>("ok1"));
    assert!(screen.test_global::<Option<String>>("err1").is_some());
    assert!(!screen.test_global::<bool>("ok2"));
    assert!(screen.take_outcome().is_none());

    // Load mode refuses a save.
    let fixture = Fixture::new(
        "save-mode",
        r#"function update(api) ok = api:save("x.dat") end"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    assert!(!screen.test_global::<bool>("ok"));
}

#[test]
fn load_requests_the_chosen_file_and_exit_leaves() {
    let fixture = Fixture::new(
        "load",
        r#"
function update(api, input)
if input.confirm then
    ok = api:load("slot.dat")
elseif input.cancel then
    api:exit()
end
end
"#,
    );
    save::save_to_path(&fixture.dir.path.join("slot.dat"), &SaveFile::default()).unwrap();
    let mut screen = fixture.load();
    let cx = fixture.context();

    let result = screen.update(
        &cx,
        UiInput {
            confirm: true,
            ..UiInput::default()
        },
    );
    assert_eq!(
        result,
        ScreenResult::Done(ScreenAction::LoadFile(fixture.dir.path.join("slot.dat")))
    );

    // A second screen exits back to the title in load mode.
    let mut screen = fixture.load();
    let result = screen.update(
        &cx,
        UiInput {
            cancel: true,
            ..UiInput::default()
        },
    );
    assert_eq!(result, ScreenResult::Done(ScreenAction::Title));
}

#[test]
fn save_mode_exit_resumes_and_remove_deletes() {
    let fixture = Fixture::new(
        "remove",
        r#"
function update(api, input)
if input.confirm then
    ok = api:remove("old.dat")
    api:exit()
end
end
"#,
    );
    save::save_to_path(&fixture.dir.path.join("old.dat"), &SaveFile::default()).unwrap();
    let mut screen = fixture.save(SaveFile::default(), false);
    let cx = fixture.context();
    let result = screen.update(
        &cx,
        UiInput {
            confirm: true,
            ..UiInput::default()
        },
    );
    assert_eq!(result, ScreenResult::Done(ScreenAction::Resume));
    assert!(screen.test_global::<bool>("ok"));
    assert!(!fixture.dir.path.join("old.dat").exists());
}

#[test]
fn input_edges_reach_the_script() {
    let fixture = Fixture::new(
        "input",
        r#"
function update(api, input)
pad = input.pad
up = input.up
confirm = input.confirm
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    screen.update(
        &cx,
        UiInput {
            up: true,
            confirm: true,
            ..UiInput::default()
        },
    );
    assert_eq!(screen.test_global::<u16>("pad"), 0x0011);
    assert!(screen.test_global::<bool>("up"));
    assert!(screen.test_global::<bool>("confirm"));
}

#[test]
fn on_open_sounds_queue_before_the_first_tick() {
    let fixture = Fixture::new(
        "on-open-sound",
        r#"
function on_open(api)
api:sound("save_confirm")
end

function update(api, input) end
"#,
    );
    let mut cx = fixture.context();
    let _screen = LuaScreen::open_load("screen/save.lua", &mut cx).unwrap();
    assert_eq!(
        cx.cues.borrow().as_slice(),
        &[crate::ui::UiCue::SaveConfirm]
    );
}

#[test]
fn sounds_queue_the_screen_cues() {
    let fixture = Fixture::new(
        "sounds",
        r#"
function update(api, input)
if input.up then api:sound("save_cursor") end
if input.confirm then api:sound("save_confirm") end
if input.cancel then api:sound("error") end
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    for input in [
        UiInput {
            up: true,
            ..UiInput::default()
        },
        UiInput {
            confirm: true,
            ..UiInput::default()
        },
        UiInput {
            cancel: true,
            ..UiInput::default()
        },
    ] {
        screen.update(&cx, input);
    }
    assert_eq!(
        cx.cues.borrow().as_slice(),
        &[
            crate::ui::UiCue::SaveCursor,
            crate::ui::UiCue::SaveConfirm,
            crate::ui::UiCue::Cancel
        ]
    );
}

#[test]
fn a_failing_update_leaves_the_screen() {
    let fixture = Fixture::new("fail", r#"function update(api) error("boom") end"#);
    let mut screen = fixture.load();
    let cx = fixture.context();
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::Title)
    );
}

#[test]
fn a_failing_draw_marks_the_screen_and_it_leaves_next_tick() {
    let fixture = Fixture::new(
        "draw-fail",
        r#"function update(api, input) end
function draw(api) error("boom") end"#,
    );
    let mut screen = fixture.save(SaveFile::default(), false);
    let cx = fixture.context();
    let _ = draw(&mut screen, &cx);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::Resume)
    );
}

#[test]
fn the_sandbox_has_no_io_or_os() {
    let fixture = Fixture::new(
        "sandbox",
        r#"
function update(api)
io_absent = io == nil
os_absent = os == nil
api:log("sandbox ok")
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    assert!(screen.test_global::<bool>("io_absent"));
    assert!(screen.test_global::<bool>("os_absent"));
}

#[test]
fn a_missing_script_is_an_error() {
    let fixture = Fixture::new("missing", "local x = 1");
    let mut cx = fixture.context();
    assert!(LuaScreen::open_load("screen/none.lua", &mut cx).is_err());
}

#[test]
fn a_script_without_update_is_an_error() {
    let fixture = Fixture::new("no-update", r#"function draw(api) end"#);
    let mut cx = fixture.context();
    let Err(err) = LuaScreen::open_load("screen/save.lua", &mut cx) else {
        panic!("a script without update must be rejected");
    };
    let message = format!("{err:#}");
    assert!(message.contains("update"), "{message}");
}

#[test]
fn a_broken_script_is_an_error() {
    let fixture = Fixture::new("broken", "function update(api) return");
    let mut cx = fixture.context();
    assert!(LuaScreen::open_load("screen/save.lua", &mut cx).is_err());
}

#[test]
fn pad_words_match_the_original_layout() {
    assert_eq!(super::pad_word(UiInput::default()), 0);
    assert_eq!(
        super::pad_word(UiInput {
            up: true,
            down: true,
            confirm: true,
            cancel: true,
            start: true,
            page_left: true,
            page_right: true,
            ..UiInput::default()
        }),
        0x0C3B
    );
    assert_eq!(
        super::held_word(UiInput {
            held_up: true,
            held_right: true,
            ..UiInput::default()
        }),
        0x0090
    );
    assert_eq!(
        super::held_word(UiInput {
            held_page_left: true,
            held_page_right: true,
            ..UiInput::default()
        }),
        0x0C00
    );
}

#[test]
fn colour_parsing_covers_names_numbers_and_hex() {
    use super::{colour_rgb, colour_tint};
    use crate::font::Tint;
    let lua = mlua::Lua::new();
    assert_eq!(colour_tint(&mlua::Value::Integer(1)).unwrap(), Tint::Green);
    assert_eq!(
        colour_tint(&mlua::Value::String(lua.create_string("grey").unwrap())).unwrap(),
        Tint::Grey
    );
    assert_eq!(
        colour_rgb(&mlua::Value::Integer(0x123456)).unwrap(),
        [0x12, 0x34, 0x56]
    );
    assert_eq!(
        colour_rgb(&mlua::Value::String(lua.create_string("#A0B0C0").unwrap())).unwrap(),
        [0xA0, 0xB0, 0xC0]
    );
    assert_eq!(
        colour_tint(&mlua::Value::String(lua.create_string("#3F7DFA").unwrap())).unwrap(),
        Tint::Rgb(0x3F, 0x7D, 0xFA)
    );
    assert!(colour_rgb(&mlua::Value::String(lua.create_string("#zz").unwrap())).is_err());
    // An eight-byte string whose `é` straddles the bytes a slice would
    // split: rejected, not a panic.
    assert!(colour_rgb(&mlua::Value::String(lua.create_string("#0é0000").unwrap())).is_err());
}

#[test]
fn hex_colour_parsing_never_slices_a_multibyte_string() {
    use super::parse_hex_colour;
    // Six bytes with `é` occupying bytes 1..3: a `str` slice at byte 2
    // would split the character and panic.
    assert_eq!(parse_hex_colour("0é0000"), None);
    assert_eq!(parse_hex_colour("123456"), Some([0x12, 0x34, 0x56]));
    // The alpha byte of the eight-digit form is ignored.
    assert_eq!(parse_hex_colour("12345678"), Some([0x12, 0x34, 0x56]));
    assert_eq!(parse_hex_colour("12345"), None);
}

#[test]
fn extreme_coordinates_clamp_instead_of_overflowing() {
    let fixture = Fixture::new(
        "coords",
        r#"
function update(api, input) end
function draw(api)
api:rect(0, 0, 2147483647, 2147483647, 0x101010)
api:text(2147483647, -2147483648, "edge")
api:sprite("art/bg.bmp", 2147483647, -2147483648)
api:sprite_region("art/dot.tim", 2147483647, -2147483648, 2147483647, -2147483648,
    2147483647, -2147483648, 2147483647, 2147483647)
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    // Before the clamp, the sprite and font cursors overflowed on these
    // values and the process panicked; the screen must keep running.
    let _ = draw(&mut screen, &cx);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    );
}

#[test]
fn a_pack_without_a_font_draws_no_text_and_keeps_running() {
    let fixture = Fixture::new(
        "no-font",
        r#"
function update(api, input) end
function draw(api)
api:text(8, 8, "Hello")
width = api:text_width("Hello")
end
"#,
    );
    let mut screen = fixture.load();
    // The engine passes no font for a pack that carries none; the screen
    // must degrade to drawing no text rather than leave.
    let mut cx = fixture.context();
    cx.font = None;
    let _ = draw(&mut screen, &cx);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    );
    assert_eq!(screen.test_global::<i32>("width"), 0);
}

#[test]
fn the_blend_option_mixes_a_sprite_with_the_backdrop() {
    let fixture = Fixture::new(
        "blend",
        r#"
function update(api, input) end
function draw(api)
api:rect(0, 0, 320, 240, 0x808080)
api:sprite("art/bg.bmp", 10, 10, {blend = "half"})
api:sprite("art/bg.bmp", 20, 10)
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let framebuffer = draw(&mut screen, &cx);
    let pixel = |x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        [
            framebuffer.rgba[index],
            framebuffer.rgba[index + 1],
            framebuffer.rgba[index + 2],
        ]
    };
    // The BMP's top-left is red: "half" averages it with the grey
    // backdrop per channel, the default blend overwrites it.
    assert_eq!(pixel(10, 10), [191, 64, 64]);
    assert_eq!(pixel(20, 10), [255, 0, 0]);
}

#[cfg(feature = "hd")]
#[test]
fn an_exact_size_background_is_blitted_with_its_alpha() {
    let fixture = Fixture::new(
        "blit",
        r#"
function update(api, input) end
function draw(api)
api:background("art/full.png")
end
"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let framebuffer = draw(&mut screen, &cx);
    let pixel = |x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        framebuffer.rgba[index..index + 4].to_vec()
    };
    assert_eq!(pixel(10, 10), [0, 0, 255, 255]);
    // The stretch path forces opaque alpha; the 320x240 blit keeps it.
    assert_eq!(pixel(100, 100), [0, 0, 255, 128]);
}

#[test]
fn replaced_art_is_recorded_as_an_hd_item_where_it_draws() {
    use crate::pack::PackWriter;

    let source = r#"
function update(api, input) end

function draw(api)
local picture = api:picture("art/bg")
api:picture_sprite_scaled(picture, 0, 0, 4, 4, 10, 20, 40, 40, 255)
end
"#;
    let build = |hd: Option<(u32, Vec<u8>)>| {
        let mut writer = PackWriter::new();
        writer
            .add("screen/save.lua", source.as_bytes().to_vec())
            .unwrap();
        writer.add("art/bg.bmp", test_bmp()).unwrap();
        if let Some((hash, bytes)) = hd {
            writer
                .add(&format!("htexture/misc/{hash:08X}.bmp"), bytes)
                .unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    };
    let picture =
        crate::ui::load_picture(&build(None), "art/bg", crate::assets::Geometry::Free, None)
            .unwrap();
    let candidate = crate::htexture::image_candidates(&picture)[0];
    let hd = crate::bmp::encode_to_vec(&crate::state::Image {
        width: 8,
        height: 8,
        rgba: [0, 0, 255, 255].repeat(64),
    })
    .unwrap();
    let pack: &'static Pack = Box::leak(Box::new(build(Some((candidate.hash, hd)))));
    let fixture = Fixture {
        pack,
        text: Text::load(pack, Some("en-US")),
        font: test_font(),
        dir: TempDir::new("hd-art"),
    };
    let mut screen = fixture.load();
    let cx = fixture.context();
    let pixel = |framebuffer: &crate::render::Framebuffer, x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        [
            framebuffer.rgba[index],
            framebuffer.rgba[index + 1],
            framebuffer.rgba[index + 2],
        ]
    };

    // Without a list the original art draws.
    let plain = draw(&mut screen, &cx);
    assert_eq!(pixel(&plain, 10, 20), [255, 0, 0]);

    {
        let mut framebuffer = crate::render::Framebuffer::new();
        framebuffer.record();
        screen.draw(&cx, &mut framebuffer);
        assert!(hd_covers(&framebuffer, 10, 20));
        assert!(hd_covers(&framebuffer, 49, 59));
        assert!(!hd_covers(&framebuffer, 50, 60));
        let quads = hd_quads(&framebuffer);
        assert_eq!(quads.len(), 1);
        let [fw, fh] = candidate.frame.map(|v| v as f32);
        assert_eq!(quads[0].dst, [10.0, 20.0, 50.0, 60.0]);
        assert_eq!(quads[0].uv, [0.0, 0.0, 4.0 / fw, 4.0 / fh]);
        assert_eq!(quads[0].colour, [1.0; 3]);
        assert_eq!(quads[0].clip, [10, 20, 40, 40]);
        let crate::draw2d::Kind::Art(image) = &quads[0].image else {
            panic!("a model quad");
        };
        assert_eq!(image.image.width, 8);
    }
}

/// The frame the `draw` body records on a `size` framebuffer, with `art/bg.bmp` (`art`) replaced by an 8x8 blue BMP, and the
/// replaced art's frame.
fn capture_entry_art(
    body: &str,
    art: Vec<u8>,
    size: [u32; 2],
) -> (crate::render::Framebuffer, [u32; 2]) {
    use crate::pack::PackWriter;

    let source = format!("function update(api, input) end\nfunction draw(api)\n{body}\nend\n");
    let build = |hd: Option<(u32, Vec<u8>)>| {
        let mut writer = PackWriter::new();
        writer
            .add("screen/save.lua", source.as_bytes().to_vec())
            .unwrap();
        writer.add("art/bg.bmp", art.clone()).unwrap();
        if let Some((hash, bytes)) = hd {
            writer
                .add(&format!("htexture/misc/{hash:08X}.bmp"), bytes)
                .unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    };
    let picture =
        crate::ui::load_picture(&build(None), "art/bg", crate::assets::Geometry::Free, None)
            .unwrap();
    let candidate = crate::htexture::image_candidates(&picture)[0];
    let hd = crate::bmp::encode_to_vec(&crate::state::Image {
        width: 8,
        height: 8,
        rgba: [0, 0, 255, 255].repeat(64),
    })
    .unwrap();
    let pack: &'static Pack = Box::leak(Box::new(build(Some((candidate.hash, hd)))));
    let fixture = Fixture {
        pack,
        text: Text::load(pack, Some("en-US")),
        font: test_font(),
        dir: TempDir::new("hd-entry-art"),
    };
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::with_size(size[0], size[1]);
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    (framebuffer, candidate.frame)
}

/// [`test_tim`]'s page with only its top-left texel blue; the rest are
/// colour `0x0000`, the keyed-clear texel.
fn keyed_tim() -> Vec<u8> {
    let mut bytes = test_tim();
    let pixels = bytes.len() - 16;
    bytes[pixels..].fill(0);
    bytes[pixels] = 0x01;
    bytes
}

#[test]
fn page_sprite_mirrored_whole_shows_its_art_over_the_keyed_clear_texels_too() {
    use crate::pack::PackWriter;

    let build = |source: &str, hd: Option<(u32, Vec<u8>)>| {
        let mut writer = PackWriter::new();
        writer
            .add("screen/save.lua", source.as_bytes().to_vec())
            .unwrap();
        writer.add("art/keyed.tim", keyed_tim()).unwrap();
        if let Some((hash, bytes)) = hd {
            writer
                .add(&format!("htexture/misc/{hash:08X}.bmp"), bytes)
                .unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    };
    let crate::assets::Artwork::Indexed(page) =
        crate::ui::load_page(&build("", None), "art/keyed").unwrap()
    else {
        panic!("an indexed page");
    };
    let candidate = crate::htexture::page_candidates(&page, 0)[0];
    let hd = crate::bmp::encode_to_vec(&crate::state::Image {
        width: 8,
        height: 8,
        rgba: [0, 255, 0, 255].repeat(64),
    })
    .unwrap();
    // The texels of the 4x4 region drawn at (10, 20) the art shows on, with and
    // without `whole`.
    let punched = |whole: &str| {
        let source = format!(
            "function update(api, input) end\nfunction draw(api)\n\
             api:page_sprite_mirrored(api:page(\"art/keyed\"), 0, 0, 4, 4, 10, 20, 4, 4, 0, 0, \
             false, false{whole})\nend\n"
        );
        let pack: &'static Pack =
            Box::leak(Box::new(build(&source, Some((candidate.hash, hd.clone())))));
        let fixture = Fixture {
            pack,
            text: Text::load(pack, Some("en-US")),
            font: test_font(),
            dir: TempDir::new("hd-whole"),
        };
        let mut screen = fixture.load();
        let cx = fixture.context();
        let mut framebuffer = crate::render::Framebuffer::new();
        framebuffer.record();
        screen.draw(&cx, &mut framebuffer);
        assert_eq!(hd_quads(&framebuffer).len(), 1);
        assert_eq!(hd_quads(&framebuffer)[0].dst, [10.0, 20.0, 14.0, 24.0]);
        let mut texels = Vec::new();
        for y in 20..24 {
            for x in 10..14 {
                if hd_covers(&framebuffer, x, y) {
                    texels.push((x, y));
                }
            }
        }
        texels
    };
    let rect: Vec<_> = (20..24)
        .flat_map(|y| (10..14).map(move |x| (x, y)))
        .collect();
    // A plain draw shows its art on its one drawn texel, a whole draw on
    // every texel of the region, so redrawn HD art isn't cut to the
    // original's shape.
    assert_eq!(punched(""), [(10, 20)]);
    assert_eq!(punched(", false"), [(10, 20)]);
    assert_eq!(punched(", true"), rect);
}

#[test]
fn page_lut_recolours_its_hd_page_by_the_originals_indices() {
    use crate::pack::PackWriter;

    // [`test_tim`]'s 8x4 page of index 1 (blue), its first four texels
    // indices 1, 0, 2, 0.
    let mut tim = test_tim();
    let pixels = tim.len() - 16;
    tim[pixels..pixels + 2].copy_from_slice(&0x0201u16.to_le_bytes());
    let source = r#"
function update(api, input) end

function draw(api)
api:page_lut(api:indexed_page("art/plan"), 0, 0, 8, 4, 10, 20, { [1] = 0x800000 })
end
"#;
    let build = |hd: Option<(u32, Vec<u8>)>| {
        let mut writer = PackWriter::new();
        writer
            .add("screen/save.lua", source.as_bytes().to_vec())
            .unwrap();
        writer.add("art/plan.tim", tim.clone()).unwrap();
        if let Some((hash, bytes)) = hd {
            writer
                .add(&format!("htexture/misc/{hash:08X}.bmp"), bytes)
                .unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    };
    let crate::assets::Artwork::Indexed(page) =
        crate::ui::load_page(&build(None), "art/plan").unwrap()
    else {
        panic!("an indexed page");
    };
    let candidate = crate::htexture::page_candidates(&page, 0)[0];
    let hd = crate::bmp::encode_to_vec(&crate::state::Image {
        width: 32,
        height: 16,
        rgba: [0, 0, 255, 255].repeat(32 * 16),
    })
    .unwrap();
    let pack: &'static Pack = Box::leak(Box::new(build(Some((candidate.hash, hd)))));
    let fixture = Fixture {
        pack,
        text: Text::load(pack, Some("en-US")),
        font: test_font(),
        dir: TempDir::new("hd-page-lut"),
    };
    let mut screen = fixture.load();
    let cx = fixture.context();
    let pixel = |framebuffer: &crate::render::Framebuffer, x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        [
            framebuffer.rgba[index],
            framebuffer.rgba[index + 1],
            framebuffer.rgba[index + 2],
        ]
    };

    // Without a list the lookup draws in software.
    let plain = draw(&mut screen, &cx);
    assert_eq!(pixel(&plain, 10, 20), [0x80, 0, 0]);
    assert_eq!(pixel(&plain, 11, 20), [0; 3]);

    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    // The art shows only on the texels the lookup draws.
    assert!(hd_covers(&framebuffer, 10, 20));
    assert!(!hd_covers(&framebuffer, 11, 20));
    assert!(!hd_covers(&framebuffer, 12, 20));
    assert!(hd_covers(&framebuffer, 17, 23));
    let quads = hd_quads(&framebuffer);
    assert_eq!(quads.len(), 1);
    let [fw, fh] = candidate.frame.map(|v| v as f32);
    assert_eq!(quads[0].dst, [10.0, 20.0, 18.0, 24.0]);
    assert_eq!(quads[0].uv, [0.0, 0.0, 8.0 / fw, 4.0 / fh]);
    assert_eq!(quads[0].colour, [1.0; 3]);
    let crate::draw2d::Kind::Recoloured(recoloured) = &quads[0].image else {
        panic!("not a recoloured quad");
    };
    assert_eq!(recoloured.art.image.width, 32);
    assert_eq!(recoloured.indices.texels[..4], [1, 0, 2, 0]);
    assert!(recoloured.lut[0].is_none() && recoloured.lut[2].is_none());
    // The page's blue draws as the lookup's red.
    let red = recoloured.lut[1].unwrap().apply([0.0, 0.0, 1.0]);
    assert!((red[0] - 128.0 / 255.0).abs() < 0.01 && red[1] == 0.0 && red[2] == 0.0);
}

#[test]
fn replaced_entry_art_is_recorded_as_an_hd_item_where_it_draws() {
    let full = crate::render::brightness_scale(2);
    let sprite_colour = [f32::from(full) / 255.0; 3];
    {
        // A region stretched over a larger rect.
        let (framebuffer, frame) = capture_entry_art(
            r#"api:sprite_region("art/bg.bmp", 0, 0, 4, 4, 10, 20, 40, 40)"#,
            test_bmp(),
            [320, 240],
        );
        let [fw, fh] = frame.map(|v| v as f32);
        assert!(hd_covers(&framebuffer, 10, 20));
        assert!(hd_covers(&framebuffer, 49, 59));
        assert!(!hd_covers(&framebuffer, 50, 60));
        let quads = hd_quads(&framebuffer);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].dst, [10.0, 20.0, 50.0, 60.0]);
        assert_eq!(quads[0].uv, [0.0, 0.0, 4.0 / fw, 4.0 / fh]);
        assert_eq!(quads[0].colour, sprite_colour);
        assert_eq!(quads[0].clip, [10, 20, 40, 40]);
        let crate::draw2d::Kind::Art(image) = &quads[0].image else {
            panic!("a model quad");
        };
        assert_eq!(image.image.width, 8);

        // A whole sprite at its own size.
        let (framebuffer, _) = capture_entry_art(
            r#"api:sprite("art/bg.bmp", 10, 20)"#,
            test_bmp(),
            [320, 240],
        );
        assert!(hd_covers(&framebuffer, 13, 23));
        assert!(!hd_covers(&framebuffer, 14, 24));
        let quads = hd_quads(&framebuffer);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].dst, [10.0, 20.0, 14.0, 24.0]);
        assert_eq!(quads[0].uv, [0.0, 0.0, 4.0 / fw, 4.0 / fh]);
        assert_eq!(quads[0].colour, sprite_colour);

        // A background stretched over the frame, ignoring the clip; it is
        // always opaque, so a blend does not keep the HD art off it.
        let (framebuffer, _) = capture_entry_art(
            r##"api:clip(0, 0, 10, 10)
api:background("art/bg.bmp", {colour = "#FF8000", blend = "half"})"##,
            test_bmp(),
            [320, 240],
        );
        assert!(hd_covers(&framebuffer, 0, 0));
        assert!(hd_covers(&framebuffer, 319, 239));
        let quads = hd_quads(&framebuffer);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].dst, [0.0, 0.0, 320.0, 240.0]);
        assert_eq!(quads[0].uv, [0.0, 0.0, 4.0 / fw, 4.0 / fh]);
        assert_eq!(quads[0].colour, [1.0, 128.0 / 255.0, 0.0]);
        assert_eq!(quads[0].clip, [0, 0, 320, 240]);

        // A blended draw has no replacement: the quad cannot blend.
        let (framebuffer, _) = capture_entry_art(
            r#"api:sprite("art/bg.bmp", 10, 20, {blend = "half"})"#,
            test_bmp(),
            [320, 240],
        );
        assert!(hd_quads(&framebuffer).is_empty());
        assert!(!hd_covers(&framebuffer, 10, 20));

        // A frame-sized background is copied one to one, on a wide frame too.
        let wide = crate::bmp::encode_to_vec(&crate::state::Image {
            width: 400,
            height: 240,
            rgba: [255, 0, 0, 255].repeat(400 * 240),
        })
        .unwrap();
        let (framebuffer, frame) =
            capture_entry_art(r#"api:background("art/bg.bmp")"#, wide, [400, 240]);
        let [fw, fh] = frame.map(|v| v as f32);
        assert!(hd_covers(&framebuffer, 0, 0));
        assert!(hd_covers(&framebuffer, 399, 239));
        let quads = hd_quads(&framebuffer);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].dst, [0.0, 0.0, 400.0, 240.0]);
        assert_eq!(quads[0].uv, [0.0, 0.0, 400.0 / fw, 240.0 / fh]);
        assert_eq!(quads[0].colour, [1.0; 3]);
    }
}

/// A solid `width` x `height` BMP in one colour.
fn solid_bmp(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    crate::bmp::encode_to_vec(&crate::state::Image {
        width,
        height,
        rgba,
    })
    .unwrap()
}

/// The sample IVM's TMD with a non-degenerate triangle, so the viewer's
/// back-face test keeps it.
fn sample_item_tmd() -> Vec<u8> {
    let ivm = crate::ivm::sample_ivm();
    let (_, tmd) = crate::ivm::split(&ivm).unwrap();
    let mut tmd = tmd.to_vec();
    let vertices = [[0i16, -1000, -1000], [0, -1000, 1000], [0, 1000, 0]];
    for (index, vertex) in vertices.iter().enumerate() {
        let at = 12 + 28 + index * 8;
        for (slot, value) in vertex.iter().enumerate() {
            tmd[at + slot * 2..at + slot * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    tmd
}

/// An HD item a recorded frame drew, as these tests compare it.
#[derive(Debug, Clone)]
struct HdQuad {
    image: crate::draw2d::Kind,
    /// `[x0, y0, x1, y1]`.
    dst: [f32; 4],
    uv: [f32; 4],
    colour: [f32; 3],
    /// `[x, y, w, h]`.
    clip: [i32; 4],
}

/// The HD art, recoloured pages and models `framebuffer` recorded, in
/// order.
fn hd_quads(framebuffer: &crate::render::Framebuffer) -> Vec<HdQuad> {
    use crate::draw2d::Kind;
    let Some(list) = framebuffer.list() else {
        return Vec::new();
    };
    list.items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                Kind::Art(_) | Kind::Recoloured(_) | Kind::Model(_)
            )
        })
        .map(|item| {
            let [x0, y0, x1, y1] = item.clip;
            let [r, g, b, _] = item.colour;
            HdQuad {
                image: item.kind.clone(),
                dst: item.rect,
                uv: item.uv,
                colour: [r, g, b],
                clip: [x0, y0, x1 - x0, y1 - y0],
            }
        })
        .collect()
}

/// Whether the HD items `framebuffer` recorded draw at frame pixel
/// `(x, y)`, as `draw2d.frag` keeps them: within the clip and, for a
/// masked item, on an opaque texel of the original art.
fn hd_covers(framebuffer: &crate::render::Framebuffer, x: i32, y: i32) -> bool {
    use crate::draw2d::{Kind, MASKED};
    let Some(list) = framebuffer.list() else {
        return false;
    };
    list.items.iter().any(|item| {
        let [x0, y0, x1, y1] = item.clip;
        let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
        let hd = matches!(
            item.kind,
            Kind::Art(_) | Kind::Recoloured(_) | Kind::Model(_)
        );
        if !inside || !hd {
            return false;
        }
        if item.map.flags & MASKED == 0 {
            return true;
        }
        let [u, v] = item.map.texel(x, y);
        let [ax, ay] = [u + item.map.atlas[0], v + item.map.atlas[1]];
        let offset = (ay as usize * crate::draw2d::ATLAS_WIDTH as usize + ax as usize) * 4 + 3;
        list.atlas.texels.get(offset) == Some(&255)
    })
}

/// Build a pack of art entries and a test screen, leaked for the
/// fixture's `'static` borrow.
fn art_pack(label: &str, source: &str, entries: Vec<(&str, Vec<u8>)>) -> Fixture {
    use crate::pack::PackWriter;
    let mut writer = PackWriter::new();
    writer
        .add("screen/save.lua", source.as_bytes().to_vec())
        .unwrap();
    for (path, data) in entries {
        writer.add(path, data).unwrap();
    }
    let pack: &'static Pack = Box::leak(Box::new(
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap(),
    ));
    Fixture {
        pack,
        text: Text::load(pack, Some("en-US")),
        font: test_font(),
        dir: TempDir::new(label),
    }
}

fn one_quad(framebuffer: &crate::render::Framebuffer) -> HdQuad {
    let quads = hd_quads(framebuffer);
    assert_eq!(quads.len(), 1, "expected one HD quad, got {quads:?}");
    quads[0].clone()
}

#[test]
fn item_icons_prefer_the_per_item_file_and_its_named_hd_image() {
    let source = r#"
function update(api, input) end
function draw(api)
local icon = api:item_icon(0x0B)
if icon ~= nil then
    api:picture_sprite_scaled(icon, 0, 0, 40, 30, 10, 20, 40, 30, 255)
end
end
"#;
    // The shared atlas is green; the per-item icon is red; the named HD
    // image is blue. The original icon's content hash is given a third
    // colour so the named key can be shown to win.
    let per_item_pack = {
        use crate::pack::PackWriter;
        let mut writer = PackWriter::new();
        writer
            .add("item/0b.bmp", solid_bmp(40, 30, [255, 0, 0]))
            .unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    };
    let per_item = crate::ui::load_picture(
        &per_item_pack,
        "item/0b",
        crate::assets::Geometry::Shipped,
        Some([0, 0, 0]),
    )
    .unwrap();
    let hash = crate::htexture::image_candidates(&per_item)[0].hash;
    let hash_entry = Box::leak(format!("htexture/misc/{hash:08X}.bmp").into_boxed_str());
    let fixture = art_pack(
        "item-icon-hd",
        source,
        vec![
            ("item/item_all.bmp", solid_bmp(40, 30, [0, 255, 0])),
            ("item/0b.bmp", solid_bmp(40, 30, [255, 0, 0])),
            (hash_entry, solid_bmp(40, 30, [0, 255, 0])),
            ("htexture/item/0b.bmp", solid_bmp(80, 60, [0, 0, 255])),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let plain = draw(&mut screen, &cx);
    let pixel = |framebuffer: &crate::render::Framebuffer, x: usize, y: usize| {
        let index = (y * 320 + x) * 4;
        [
            framebuffer.rgba[index],
            framebuffer.rgba[index + 1],
            framebuffer.rgba[index + 2],
        ]
    };
    // Without a capture the per-item picture draws, not the atlas.
    assert_eq!(pixel(&plain, 11, 21), [255, 0, 0]);

    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 11, 21), [0, 0, 0]);
    let quad = one_quad(&framebuffer);
    assert_eq!(quad.dst, [10.0, 20.0, 50.0, 50.0]);
    assert_eq!(quad.uv, [0.0, 0.0, 1.0, 1.0]);
    let crate::draw2d::Kind::Art(image) = &quad.image else {
        panic!("an icon quad");
    };
    assert_eq!((image.image.width, image.image.height), (80, 60));
    assert_eq!(image.image.rgba[0..3], [0, 0, 255]);
    assert_eq!(image.frame, [40, 30]);
}

#[test]
fn legacy_item_all_hd_name_replaces_the_atlas_cell() {
    let source = r#"
function update(api, input) end
function draw(api)
local icon = api:item_icon(0x01)
if icon ~= nil then
    api:picture_sprite_scaled(icon, 0, 0, 40, 30, 10, 20, 40, 30, 255)
end
end
"#;
    // Only the shared atlas exists, so the icon is its row-0 crop. The
    // legacy HD name (`hires/Data/ITEM_ALL/00.webp`) wins over the hash.
    let fixture = art_pack(
        "item-icon-legacy-hd",
        source,
        vec![
            ("item/item_all.bmp", solid_bmp(40, 30, [0, 255, 0])),
            (
                "htexture/data/item_all/00.bmp",
                solid_bmp(80, 60, [0, 0, 255]),
            ),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Art(image) = &quad.image else {
        panic!("an icon quad");
    };
    assert_eq!((image.image.width, image.image.height), (80, 60));
    assert_eq!(image.frame, [40, 30]);
}

#[test]
fn an_old_pack_hash_still_replaces_the_shared_atlas_icon() {
    let source = r#"
function update(api, input) end
function draw(api)
local icon = api:item_icon(0x01)
if icon ~= nil then
    api:picture_sprite_scaled(icon, 0, 0, 40, 30, 10, 20, 40, 30, 255)
end
end
"#;
    // No per-item and no named file: the icon is the atlas crop, whose
    // content hash an older HD pack still names.
    let atlas_only = crate::pack::Pack::from_bytes({
        use crate::pack::PackWriter;
        let mut writer = PackWriter::new();
        writer
            .add("item/item_all.bmp", solid_bmp(40, 30, [0, 255, 0]))
            .unwrap();
        writer.to_bytes().unwrap()
    })
    .unwrap();
    let icon = crate::ui::load_picture(
        &atlas_only,
        "item/item_all",
        crate::assets::Geometry::Shipped,
        Some([0, 0, 0]),
    )
    .unwrap();
    let hash = crate::htexture::image_candidates(&icon)[0].hash;
    let hash_entry = Box::leak(format!("htexture/misc/{hash:08X}.bmp").into_boxed_str());
    let fixture = art_pack(
        "item-icon-atlas-hash",
        source,
        vec![
            ("item/item_all.bmp", solid_bmp(40, 30, [0, 255, 0])),
            (hash_entry, solid_bmp(80, 60, [0, 0, 255])),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Art(image) = &quad.image else {
        panic!("an icon quad");
    };
    assert_eq!((image.image.width, image.image.height), (80, 60));
}

#[test]
fn a_page_sprite_keeps_its_content_hash_replacement() {
    // The machinegun specials draw cells of the shared `ui/staitem` page, so
    // the page hash must still reach those draws.
    let source = r#"
function update(api, input) end
function draw(api)
api:page_sprite_mirrored(api:page("art/dot"), 0, 0, 4, 4, 10, 20, 8, 8, 0, 0, false, false)
end
"#;
    let base = art_pack(
        "page-hash-base",
        "function update(api, input) end function draw(api) end",
        vec![("art/dot.tim", test_tim())],
    );
    let hash = {
        let page = crate::ui::load_page(base.pack, "art/dot").unwrap();
        let crate::assets::Artwork::Indexed(texture) = page else {
            panic!("the dot page is indexed");
        };
        crate::htexture::page_candidates(&texture, 0)[0].hash
    };
    let hash_entry = Box::leak(format!("htexture/misc/{hash:08X}.bmp").into_boxed_str());
    let fixture = art_pack(
        "page-hash",
        source,
        vec![
            ("art/dot.tim", test_tim()),
            (hash_entry, solid_bmp(8, 8, [0, 0, 255])),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Art(image) = &quad.image else {
        panic!("a page quad");
    };
    assert_eq!((image.image.width, image.image.height), (8, 8));
}

#[test]
fn item_icon_hd_prefers_the_per_item_name_then_the_legacy_name() {
    let source = r#"
function update(api, input) end
function draw(api)
local icon = api:item_icon(0x01)
if icon ~= nil then
    api:picture_sprite_scaled(icon, 0, 0, 40, 30, 10, 20, 40, 30, 255)
end
end
"#;
    let atlas = || ("item/item_all.bmp", solid_bmp(40, 30, [0, 255, 0]));
    let legacy = || {
        (
            "htexture/data/item_all/00.bmp",
            solid_bmp(100, 60, [255, 255, 0]),
        )
    };
    let primary = || ("htexture/item/01.bmp", solid_bmp(120, 60, [0, 0, 255]));
    let hd_size = |label: &str, entries: Vec<(&str, Vec<u8>)>| {
        let fixture = art_pack(label, source, entries);
        let mut screen = fixture.load();
        let cx = fixture.context();
        let mut framebuffer = crate::render::Framebuffer::new();
        framebuffer.record();
        screen.draw(&cx, &mut framebuffer);
        let quad = one_quad(&framebuffer);
        let crate::draw2d::Kind::Art(image) = &quad.image else {
            panic!("an icon quad");
        };
        (image.image.width, image.image.height)
    };
    // The legacy name wins over the hash (none is present here).
    assert_eq!(
        hd_size("precedence-legacy", vec![atlas(), legacy()]),
        (100, 60)
    );
    // The per-item name wins over the legacy name.
    assert_eq!(
        hd_size("precedence-primary", vec![atlas(), legacy(), primary()]),
        (120, 60)
    );
}

#[test]
fn a_broken_per_item_icon_falls_back_to_the_shared_atlas() {
    let source = r#"
function update(api, input) end
function draw(api)
local icon = api:item_icon(0x01)
if icon ~= nil then
    api:picture_sprite_scaled(icon, 0, 0, 40, 30, 10, 20, 40, 30, 255)
end
end
"#;
    let fixture = art_pack(
        "item-icon-broken",
        source,
        vec![
            ("item/01.bmp", b"not a BMP".to_vec()),
            ("item/item_all.bmp", solid_bmp(40, 30, [255, 0, 0])),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let framebuffer = draw(&mut screen, &cx);
    let index = (20 * 320 + 10) * 4;
    assert_eq!(
        &framebuffer.rgba[index..index + 3],
        &[255, 0, 0],
        "the atlas row still draws"
    );
}

#[test]
fn the_item_viewer_uses_a_named_model_texture_replacement() {
    let tmd = sample_item_tmd();
    let ivm = crate::ivm::sample_ivm();
    let (texture, _) = crate::ivm::split(&ivm).unwrap();
    let page =
        crate::bmp::encode_indexed_page(&crate::tim::decode_8bpp(texture).unwrap(), 0).unwrap();
    let source = r#"
function update(api, input) end
function draw(api)
local viewport = { 50, 50, 100, 80 }
local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
api:fill_rect(ox + viewport[1], oy + viewport[2], viewport[3], viewport[4], 0, 0, 32, 255)
api:clip(ox + viewport[1], oy + viewport[2], viewport[3], viewport[4])
local model = api:item_model(0x02)
if model ~= nil then
    api:draw_ivm(model, {
        joints = { { rx = 0, ry = 0, rz = 0, tx = 0x1980 } },
        camera = {
            from = { 15000, 0, 0 },
            to = { 0, 0, 0 },
            fov = 0xC0,
            screen = { 112 - 160, 76 - 120 },
        },
    })
end
api:clip()
end
"#;
    let fixture = art_pack(
        "item-model-hd",
        source,
        vec![
            ("item/02_model.tmd", tmd.clone()),
            ("item/02_model.bmp", page),
            (
                "htexture/item/02_model.bmp",
                solid_bmp(512, 512, [0, 0, 255]),
            ),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Model(model) = &quad.image else {
        panic!("a model quad");
    };
    let page = model.page.as_ref().expect("the named page replacement");
    assert_eq!((page.image.width, page.image.height), (512, 512));
    assert_eq!(page.frame, [256, 256]);
    assert!(!model.vertices.is_empty());
    assert_eq!(quad.clip, [50, 50, 100, 80]);
}

#[test]
fn every_item_icon_loads_without_exhausting_the_screen_budget() {
    // A converted pack has one icon per item; the item box can show 48 at
    // once and the pause screen keeps them loaded, so the icons must not
    // consume the script art count budget.
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for item in crate::items::art_items() {
        if let Some(entry) = crate::item_art::icon_entry(item) {
            entries.push((entry, solid_bmp(40, 30, [10, 20, 30])));
        }
    }
    let mut writer = crate::pack::PackWriter::new();
    writer
        .add(
            "screen/save.lua",
            br#"
function update(api, input) end
function draw(api)
    local count = 0
    for item = 1, 0x70 do
        if api:item_icon(item) ~= nil then
            count = count + 1
        end
    end
    api:set("count", count)
end
"#
            .to_vec(),
        )
        .unwrap();
    for (path, data) in entries {
        writer.add(&path, data).unwrap();
    }
    let pack: &'static Pack = Box::leak(Box::new(
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap(),
    ));
    let fixture = Fixture {
        pack,
        text: Text::load(pack, Some("en-US")),
        font: test_font(),
        dir: TempDir::new("item-icon-budget"),
    };
    let mut screen = fixture.load();
    let cx = fixture.context();
    let _ = draw(&mut screen, &cx);
    assert_eq!(screen.value("count"), 92, "every item with an icon loads");
}

#[test]
fn content_hashes_still_replace_item_icons_and_model_pages() {
    // A pack made before the named keys existed only has hashes. The
    // icon hashes its 40x30 cell and the model page its 256x256 upload.
    let icon_source = r#"
function update(api, input) end
function draw(api)
local icon = api:item_icon(0x01)
if icon ~= nil then
    api:picture_sprite_scaled(icon, 0, 0, 40, 30, 10, 20, 40, 30, 255)
end
end
"#;
    let per_item_pack = {
        use crate::pack::PackWriter;
        let mut writer = PackWriter::new();
        writer
            .add("item/01.bmp", solid_bmp(40, 30, [255, 0, 0]))
            .unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    };
    let icon = crate::ui::load_picture(
        &per_item_pack,
        "item/01",
        crate::assets::Geometry::Shipped,
        Some([0, 0, 0]),
    )
    .unwrap();
    let icon_hash = crate::htexture::image_candidates(&icon)[0].hash;
    let icon_entry = Box::leak(format!("htexture/misc/{icon_hash:08X}.bmp").into_boxed_str());
    let fixture = art_pack(
        "item-icon-hash",
        icon_source,
        vec![
            ("item/01.bmp", solid_bmp(40, 30, [255, 0, 0])),
            (icon_entry, solid_bmp(80, 60, [0, 0, 255])),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Art(image) = &quad.image else {
        panic!("an icon quad");
    };
    assert_eq!((image.image.width, image.image.height), (80, 60));

    // The model page's hash route.
    let ivm = crate::ivm::sample_ivm();
    let (texture, _) = crate::ivm::split(&ivm).unwrap();
    let texture = crate::tim::decode_8bpp(texture).unwrap();
    let page_hash = crate::htexture::page_candidates(&texture, 0)[0].hash;
    let page_entry = Box::leak(format!("htexture/misc/{page_hash:08X}.bmp").into_boxed_str());
    let page = crate::bmp::encode_indexed_page(&texture, 0).unwrap();
    let source = r#"
function update(api, input) end
function draw(api)
local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
api:fill_rect(ox + 50, oy + 50, 100, 80, 0, 0, 32, 255)
api:clip(ox + 50, oy + 50, 100, 80)
local model = api:item_model(0x02)
if model ~= nil then
    api:draw_ivm(model, {
        joints = { { rx = 0, ry = 0, rz = 0, tx = 0x1980 } },
        camera = {
            from = { 15000, 0, 0 },
            to = { 0, 0, 0 },
            fov = 0xC0,
            screen = { 112 - 160, 76 - 120 },
        },
    })
end
api:clip()
end
"#;
    let fixture = art_pack(
        "item-model-hash",
        source,
        vec![
            ("item/02_model.tmd", sample_item_tmd()),
            ("item/02_model.bmp", page),
            (page_entry, solid_bmp(512, 512, [0, 0, 255])),
        ],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Model(model) = &quad.image else {
        panic!("a model quad");
    };
    let page = model.page.as_ref().expect("the hash page replacement");
    assert_eq!((page.image.width, page.image.height), (512, 512));
    assert_eq!(page.frame, [256, 256]);
}

#[test]
fn a_tmd_only_item_model_draws_untextured() {
    let tmd = sample_item_tmd();
    let source = r#"
function update(api, input) end
function draw(api)
local viewport = { 50, 50, 100, 80 }
local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
api:fill_rect(ox + viewport[1], oy + viewport[2], viewport[3], viewport[4], 0, 0, 32, 255)
api:clip(ox + viewport[1], oy + viewport[2], viewport[3], viewport[4])
local model = api:item_model(0x01)
if model ~= nil then
    api:draw_ivm(model, {
        joints = { { rx = 0, ry = 0, rz = 0, tx = 0x1980 } },
        camera = {
            from = { 15000, 0, 0 },
            to = { 0, 0, 0 },
            fov = 0xC0,
            screen = { 112 - 160, 76 - 120 },
        },
    })
end
api:clip()
end
"#;
    let fixture = art_pack(
        "item-model-tmd-only",
        source,
        vec![("item/01_model.tmd", tmd)],
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.record();
    screen.draw(&cx, &mut framebuffer);
    let quad = one_quad(&framebuffer);
    let crate::draw2d::Kind::Model(model) = &quad.image else {
        panic!("a model quad");
    };
    assert!(model.page.is_none(), "no texture means no HD page");
    assert!(!model.vertices.is_empty());
}

#[test]
fn pending_local_time_and_tagged_strings_reach_the_script() {
    let fixture = Fixture::new(
        "pending",
        r#"
function update(api)
pending = api:pending()
time = api:local_time()
chris = api:string("UI_SAVE_CHARACTER_0_NAME", "en-US")
missing = api:string("UI_SAVE_CHARACTER_0_NAME", "xx-XX")
end
"#,
    );
    let file = SaveFile {
        character: 1,
        saves: 12,
        stage: 2,
        room: 5,
        ..SaveFile::default()
    };
    let mut screen = fixture.save(file, false);
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    let pending: mlua::Table = screen.test_global("pending");
    assert_eq!(pending.get::<u8>("character").unwrap(), 1);
    assert_eq!(pending.get::<u8>("saves").unwrap(), 12);
    assert_eq!(pending.get::<u8>("stage").unwrap(), 2);
    assert_eq!(pending.get::<u8>("room").unwrap(), 5);
    let time: Option<mlua::Table> = screen.test_global("time");
    if let Some(time) = time {
        assert!((1..=12).contains(&time.get::<u8>("month").unwrap()));
    }
    assert_eq!(
        screen.test_global::<Option<String>>("chris").as_deref(),
        Some("Chris")
    );
    assert_eq!(screen.test_global::<Option<String>>("missing"), None);

    // Load mode has no pending block.
    let fixture = Fixture::new(
        "pending-load",
        r#"function update(api) has = api:pending() ~= nil end"#,
    );
    let mut screen = fixture.load();
    let cx = fixture.context();
    screen.update(&cx, UiInput::default());
    assert!(!screen.test_global::<bool>("has"));
}

#[test]
fn the_cr_mod_names_a_new_save_like_classic_rebirth() {
    let fixture = Fixture::new(
        "cr-name",
        include_str!("../../../mods/crloadsave/script/loadsave.lua"),
    );
    let file = SaveFile {
        character: 1,
        saves: 4,
        ..SaveFile::default()
    };
    let mut screen = fixture.save(file, false);
    let cx = fixture.context();
    // The cursor opens on `Create New`.
    screen.update(
        &cx,
        UiInput {
            confirm: true,
            ..UiInput::default()
        },
    );
    let names: Vec<String> = fixture
        .dir
        .path
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let name = &names[0];
    if crate::local_time::now().is_some() {
        // `Jill_04_YYYY-MM-DD HH^MM.SS.dat`
        let rest = name.strip_prefix("Jill_04_").expect(name);
        let date = rest.strip_suffix(".dat").expect(name);
        let bytes = date.as_bytes();
        assert_eq!(bytes.len(), 19, "{name}");
        assert_eq!(
            (bytes[4], bytes[7], bytes[10], bytes[13], bytes[16]),
            (b'-', b'-', b' ', b'^', b'.'),
            "{name}"
        );
    } else {
        assert_eq!(name, "save_01.dat");
    }
}
