//! The Lua method registrations, split out of the parent module (#266).
//! mlua turns each registered closure into generic glue that is costly to
//! compile, and moving these closures' spans recompiles all of it. In their
//! own file, edits to the parent module no longer move them.

use super::*;

/// Register the handle-based art and drawing methods.
pub(in crate::ui::lua_screen) fn add_methods<'a, M: UserDataMethods<Api<'a>>>(methods: &mut M) {
    // ---- layout --------------------------------------------------------
    methods.add_method("width", |_lua, api, ()| Ok(api.host.layout.0));
    methods.add_method("height", |_lua, api, ()| Ok(api.host.layout.1));

    // ---- art -----------------------------------------------------------
    methods.add_method("has_image", |_lua, api, stem: String| {
        Ok(api.cx.pack.has_asset(&stem, AssetClass::Image))
    });

    methods.add_method_mut(
        "picture",
        |_lua, api, (stem, key, geometry): (String, Option<Value>, Option<String>)| {
            let key = match key {
                Some(Value::Nil) | None => None,
                Some(value) => Some(colour_rgb(&value)?),
            };
            // "shipped" pins the art to the original sheet's size; the
            // default lets a replacement be authored at any size.
            let geometry = match geometry.as_deref() {
                None | Some("free") => Geometry::Free,
                Some("shipped") => Geometry::Shipped,
                Some(other) => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "unknown picture geometry {other:?}"
                    )));
                }
            };
            let pack: &Pack = api.cx.pack;
            let cache_key = format!("picture:{stem}:{key:?}:{geometry:?}");
            Ok(loaded(api.host.assets.load(cache_key, || {
                crate::ui::load_picture(pack, &stem, geometry, key).map(Asset::Picture)
            })))
        },
    );

    methods.add_method_mut("page", |_lua, api, stem: String| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("page:{stem}");
        Ok(loaded(api.host.assets.load(cache_key, || {
            crate::ui::load_page(pack, &stem).map(Asset::Page)
        })))
    });

    // `item_icon(item)`: the item's 40x30 inventory icon as a picture handle,
    // from its folder's `item.bmp`; nil and a message when it has none.
    //
    // The handle's HD names are registered so a named
    // `htexture/item/<id>/item` (or the CR `htexture/data/item_all/<row>`)
    // replacement wins over the content hash while the frame is captured.
    methods.add_method_mut("item_icon", |_lua, api, item: u8| {
        let pack: &Pack = api.cx.pack;
        let Some(entry) = crate::item_art::icon_entry(item) else {
            return Ok((None, Some(format!("item {item:#04x} has no icon"))));
        };
        let stem = crate::assets::stem(&entry).to_string();
        let cache_key = format!("icon:{item:02x}");
        if let Some(handle) = api.host.assets.handle(&cache_key) {
            return Ok((Some(handle), None));
        }
        let result = api.host.assets.load_icon(cache_key, || {
            crate::ui::load_picture(pack, &stem, Geometry::Shipped, Some([0, 0, 0]))
                .map(Asset::Picture)
        });
        if let Ok(handle) = result {
            api.host
                .assets
                .set_hd_names(handle, crate::item_art::icon_hd_names(item));
        }
        Ok(loaded(result))
    });

    // `indexed_page(stem)`: a page that is always indexed - a true-colour
    // replacement is re-indexed colour for colour - for the screens that
    // composite by texel index (`page_lut`).
    methods.add_method_mut("indexed_page", |_lua, api, stem: String| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("indexed:{stem}");
        Ok(loaded(api.host.assets.load(cache_key, || {
            let asset = pack
                .read_asset(&stem, AssetClass::Image)?
                .ok_or_else(|| anyhow::anyhow!("missing {stem}"))?;
            crate::assets::decode_texture(&asset, Geometry::Shipped)
                .map(|texture| Asset::Page(Artwork::Indexed(texture)))
                .map_err(|err| err.context(format!("invalid {}", asset.path())))
        })))
    });

    // `item_model(item)`: the item viewer's model for `item`; nil and a
    // message when the item has none or its file is missing.
    //
    // The model is the item folder's `model.tmd` with its `model.bmp` page.
    methods.add_method_mut("item_model", |_lua, api, item: u8| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("item-model:{item:02x}");
        let result = api.host.assets.load(cache_key, || {
            let Some(model) = crate::item_art::load_model(pack, item)? else {
                anyhow::bail!("item {item:#04x} has no model");
            };
            Ok(Asset::Model(model.ivm, model.bytes))
        });
        if let Ok(handle) = result {
            api.host
                .assets
                .set_hd_names(handle, crate::item_art::model_hd_names(item));
        }
        Ok(loaded(result))
    });

    // `model_objects(model)`: the number of objects a model carries.
    methods.add_method("model_objects", |_lua, api, handle: usize| {
        Ok(api.host.assets.model(handle)?.objects.len())
    });

    // `release(handle)`: drop art the screen no longer draws, so a screen
    // that pages through more art than the cache holds stays in budget.
    methods.add_method_mut("release", |_lua, api, handle: usize| {
        api.host.assets.release(handle);
        Ok(())
    });

    methods.add_method("art_size", |_lua, api, handle: usize| {
        Ok(api.host.assets.get(handle)?.size())
    });

    // `page_palette(page, row)`: an indexed page's CLUT row as a table of
    // `0xRRGGBB` colours keyed by texel index (`0..=255`); nil for an RGBA
    // page.
    methods.add_method(
        "page_palette",
        |lua, api, (handle, row): (usize, Option<usize>)| {
            let Artwork::Indexed(texture) = api.host.assets.page(handle)? else {
                return Ok(None);
            };
            let row = row.unwrap_or(0);
            let table = lua.create_table()?;
            for index in 0..=255u8 {
                let [r, g, b, _] = texture.palette(row, index);
                table.set(
                    index,
                    (i64::from(r) << 16) | (i64::from(g) << 8) | i64::from(b),
                )?;
            }
            Ok(Some(table))
        },
    );

    // `page_stp(page, row)`: the indices of an indexed page's CLUT row whose
    // colour carries the PSX semi-transparency (STP) bit, as a set
    // (`stp[index] == true`); nil for an RGBA page.
    methods.add_method(
        "page_stp",
        |lua, api, (handle, row): (usize, Option<usize>)| {
            let Artwork::Indexed(texture) = api.host.assets.page(handle)? else {
                return Ok(None);
            };
            let row = row.unwrap_or(0);
            let table = lua.create_table()?;
            for index in 0..=255u8 {
                if texture.palette_stp(row, index) {
                    table.set(index, true)?;
                }
            }
            Ok(Some(table))
        },
    );

    // ---- primitives ----------------------------------------------------
    methods.add_method_mut("clear", |_lua, api, ()| {
        api.framebuffer.as_deref_mut().ok_or_else(no_draw)?.clear();
        Ok(())
    });

    methods.add_method_mut(
        "fill_rect",
        |_lua, api, (x, y, w, h, r, g, b, a): (i32, i32, i32, i32, i64, i64, i64, Option<i64>)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.fill_rect(
                rect(x, y, w, h),
                [byte(r), byte(g), byte(b), byte(a.unwrap_or(255))],
            );
            Ok(())
        },
    );

    // `line(x0, y0, x1, y1, r, g, b)`: a one-pixel line as the original's
    // GPU primitive draws it - a half-pixel quad, so the far end of each axis
    // is excluded, and each colour byte through the primitive's
    // `byte / 256 * 255` conversion (0xD8 draws as 0xD7).
    methods.add_method_mut(
        "line",
        |_lua, api, (x0, y0, x1, y1, r, g, b): (i32, i32, i32, i32, i64, i64, i64)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.draw_line(
                [coord(x0), coord(y0)],
                [coord(x1), coord(y1)],
                [
                    line_colour(byte(r)),
                    line_colour(byte(g)),
                    line_colour(byte(b)),
                ],
            );
            Ok(())
        },
    );

    // `draw_ivm(model, {joints = {...}, objects = {{...}, ...}, camera =
    // {from, to, fov, screen = {x, y}}, light})`: object `i` takes the
    // joint chain `objects[i]`, and every object past the list the one
    // composed `joints` chain. `screen` offsets the projection centre,
    // which sits at the layout's centre (see [`band_centre`]).
    // While the GPU renderer draws the frame, the model renders on the GPU,
    // cut to the clip rect ([`Framebuffer::draw_ivm_captured`]).
    methods.add_method_mut("draw_ivm", |_lua, api, (handle, spec): (usize, Table)| {
        let Api {
            host, framebuffer, ..
        } = api;
        let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
        let model = host.assets.model(handle)?;
        let joint = joint_matrix(spec.get("joints")?)?;
        let camera: Table = spec.get("camera")?;
        let screen = match camera.get::<Option<Table>>("screen")? {
            Some(offset) => [offset.get(1)?, offset.get(2)?],
            None => [0, 0],
        };
        let mut camera = Camera::from_points(
            triple(&camera, "from")?,
            triple(&camera, "to")?,
            field(&camera, "fov")?,
        );
        camera.screen = band_centre(framebuffer, screen);
        let light = match spec.get::<Option<Table>>("light")? {
            Some(table) => Some(light(&table)?),
            None => None,
        };
        let mut joints = vec![joint; model.objects.len()];
        if let Some(objects) = spec.get::<Option<Table>>("objects")? {
            for (slot, chain) in joints.iter_mut().zip(objects.sequence_values::<Table>()) {
                *slot = joint_matrix(Some(chain?))?;
            }
        }
        let page = hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0);
        framebuffer.draw_ivm_captured(model, &joints, &camera, light.as_ref(), page);
        Ok(())
    });

    // `draw_actor(name, {from, to, fov})`: an engine-owned actor through
    // the screen's camera; false when the engine has no such actor.
    methods.add_method_mut(
        "draw_actor",
        |_lua, api, (name, camera): (String, Table)| {
            let mut camera = Camera::from_points(
                triple(&camera, "from")?,
                triple(&camera, "to")?,
                field(&camera, "fov")?,
            );
            let Api {
                framebuffer,
                actors,
                ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            camera.screen = band_centre(framebuffer, [0, 0]);
            Ok(actors
                .as_deref_mut()
                .is_some_and(|draw| draw(&name, &camera, framebuffer)))
        },
    );

    // `grid_text(x, y, scale, bytes)`: raw font-sheet cells, see
    // [`draw_grid_text`]. Without a font nothing draws.
    methods.add_method_mut(
        "grid_text",
        |_lua, api, (x, y, scale, text): (i32, i32, i64, mlua::String)| {
            let Some(font) = api.cx.font else {
                return Ok(());
            };
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            draw_grid_text(
                framebuffer,
                font,
                [coord(x), coord(y)],
                byte(scale),
                &text.as_bytes(),
            );
            Ok(())
        },
    );

    // `clip(x, y, w, h)`: clip the rest of this draw to a screen rect;
    // `clip()` lifts it.
    methods.add_method_mut(
        "clip",
        |_lua, api, (x, y, w, h): (Option<i32>, Option<i32>, Option<i32>, Option<i32>)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.set_clip_rect(match (x, y, w, h) {
                (Some(x), Some(y), Some(w), Some(h)) => Some(rect(x, y, w, h)),
                _ => None,
            });
            Ok(())
        },
    );

    // `darken(alpha)`: blend the whole frame towards black by `alpha`/255
    // (the screens' black fade rect).
    methods.add_method_mut("darken", |_lua, api, alpha: i64| {
        let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
        framebuffer.fade_to_black_weighted(byte(alpha));
        Ok(())
    });

    // `page_ramp(page, sx, sy, w, h, dx, dy, ramp)`: an unscaled page rect
    // modulated by `2 * ramp` and half-blended over the frame (the FILE
    // reader's backdrop pieces).
    methods.add_method_mut(
        "page_ramp",
        |_lua,
         api,
         (handle, sx, sy, w, h, dx, dy, ramp): (usize, i32, i32, i32, i32, i32, i32, i64)| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let art = host.assets.page(handle)?;
            let (src, dst, ramp) = (rect(sx, sy, w, h), rect(dx, dy, w, h), byte(ramp));
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_page_sprite_ramp(art, src, dst, 0, ramp);
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0) {
                Some(hd) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, [false; 2]);
                    // `2 * ramp`, unclamped like the software modulation.
                    let colour = [f32::from(ramp) * 2.0 / 255.0; 3];
                    framebuffer.draw_replaced(hd, corners, uv, colour, draw);
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    methods.add_method_mut(
        "blend_black_rect",
        |_lua, api, (x, y, w, h, alpha): (i32, i32, i32, i32, i64)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.blend_black_rect(rect(x, y, w, h), byte(alpha));
            Ok(())
        },
    );

    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "picture_sprite_scaled",
        |_lua,
         api,
         (handle, sx, sy, sw, sh, dx, dy, dw, dh, scale, cell): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i64,
            Option<Table>,
        )| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let image = host.assets.picture(handle)?;
            let (src, dst, scale) = (rect(sx, sy, sw, sh), rect(dx, dy, dw, dh), byte(scale));
            let cell = match cell {
                Some(cell) => Some(rect(cell.get(1)?, cell.get(2)?, cell.get(3)?, cell.get(4)?)),
                None => None,
            };
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_rgba_sprite_scaled(image, src, dst, scale);
            };
            // A cell is its own upload: its replacement covers the cell, so
            // the source rect is taken relative to the cell's corner.
            let hd = match cell {
                Some(cell) if framebuffer.records() => host
                    .assets
                    .cell_replacement(api.cx.pack, handle, cell)
                    .map(|hd| (hd, [src[0] - cell[0], src[1] - cell[1], src[2], src[3]])),
                Some(_) => None,
                None => {
                    hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0).map(|hd| (hd, src))
                }
            };
            match hd {
                Some((hd, src)) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, [false; 2]);
                    let colour = sprite_colour(Tint::White, scale);
                    framebuffer.draw_replaced(hd, corners, uv, colour, |framebuffer| {
                        // A cell is drawn as one quad, so its HD art covers
                        // the cell's keyed-clear texels too.
                        if cell.is_some() {
                            framebuffer.punch_rect(dst);
                        }
                        draw(framebuffer);
                    });
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    methods.add_method_mut(
        "draw_background",
        |_lua, api, (handle, x, y, r, g, b): (usize, i32, i32, Option<i64>, Option<i64>, Option<i64>)| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let image = host.assets.picture(handle)?;
            let colour = [
                byte(r.unwrap_or(255)),
                byte(g.unwrap_or(255)),
                byte(b.unwrap_or(255)),
            ];
            let origin = [coord(x), coord(y)];
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_background(image, origin, colour);
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0) {
                Some(hd) => {
                    let (corners, uv) = background_quad(&hd, image, framebuffer, origin);
                    let colour = colour.map(|channel| f32::from(channel) / 255.0);
                    // A background ignores the clip, so its quad must too.
                    let clip = framebuffer.clip.take();
                    framebuffer.draw_replaced(hd, corners, uv, colour, draw);
                    framebuffer.clip = clip;
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    // `page_lut(page, sx, sy, w, h, dx, dy, lut[, whole[, options]])`: an
    // indexed-page rect drawn through `lut`, a table of `0xRRGGBB` colours
    // keyed by texel index; an index without an entry is transparent, index
    // 0 included. The screens composite remapped palettes with it (the map's
    // tinted and room-highlighted plans). With `whole`, a lookup that tints
    // every index alike lets an HD replacement cover the whole rect, as
    // `page_sprite_mirrored`'s does, for art the HD page redraws.
    // `options.dw`/`options.dh` stretch the rect over a destination of that
    // size (nearest sampling), and `options.blend` mixes it in: `"half"`
    // averages it with the frame, `"add"` adds it; opaque by default.
    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "page_lut",
        |_lua,
         api,
         (handle, sx, sy, w, h, dx, dy, lut, whole, options): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            Table,
            Option<bool>,
            Option<Table>,
        )| {
            let (size, blend) = match &options {
                Some(options) => (
                    [
                        options.get::<Option<i32>>("dw")?,
                        options.get::<Option<i32>>("dh")?,
                    ],
                    lut_blend(options.get::<Option<String>>("blend")?.as_deref())?,
                ),
                None => ([None; 2], SpriteBlend::Opaque),
            };
            let Api {
                host, framebuffer, ..
            } = api;
            let Artwork::Indexed(texture) = host.assets.page(handle)? else {
                return Err(mlua::Error::RuntimeError(format!(
                    "art handle {handle} is not an indexed page"
                )));
            };
            let mut colours = [None; 256];
            for (index, colour) in colours.iter_mut().enumerate() {
                if let Some(rgb) = lut.get::<Option<i64>>(index)? {
                    *colour = Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
                }
            }
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let [dw, dh] = [size[0].unwrap_or(w), size[1].unwrap_or(h)];
            let (src, dst) = (rect(sx, sy, w, h), rect(dx, dy, dw, dh));
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_indexed_sprite_lut(texture, src, dst, &colours, blend);
            };
            // The HD page through CLUT row 0, recoloured on the GPU by the
            // original's indices, so the lookup applies at its resolution.
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0) {
                Some(art) => {
                    let (corners, uv) = sprite_quad(&art, src, dst, [false; 2]);
                    let palette = std::array::from_fn(|index| {
                        let [r, g, b, _] = texture.palette(0, index as u8);
                        [r, g, b]
                    });
                    let whole = match whole {
                        Some(true) => htexture::Recolour::whole(&palette, &colours),
                        _ => None,
                    };
                    let (lut, whole) = match whole {
                        Some(lut) => (lut, true),
                        None => (htexture::Recolour::lut(&palette, &colours), false),
                    };
                    let draw = |framebuffer: &mut Framebuffer| {
                        if whole {
                            framebuffer.punch_rect(dst);
                        }
                        draw(framebuffer);
                    };
                    let recoloured = htexture::Recoloured {
                        art,
                        indices: host.assets.indices(handle)?,
                        lut,
                    };
                    framebuffer.draw_replaced(
                        htexture::QuadImage::Recoloured(Arc::new(recoloured)),
                        corners,
                        uv,
                        [1.0; 3],
                        draw,
                    );
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    // `page_sprite_mirrored(page, sx, sy, sw, sh, dx, dy, dw, dh, clut,
    // brightness, mirror_u, mirror_v[, whole])`: a sheet region flipped per
    // axis. With `whole`, an HD replacement covers the region's keyed-clear
    // texels too and its own alpha says what shows. That suits art the HD
    // page redraws rather than upscales: cut to the original's drawn texels,
    // it would come out in pieces.
    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "page_sprite_mirrored",
        |_lua,
         api,
         (handle, sx, sy, sw, sh, dx, dy, dw, dh, clut, brightness, mirror_u, mirror_v, whole): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            usize,
            i64,
            bool,
            bool,
            Option<bool>,
        )| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let art = host.assets.page(handle)?;
            let (src, dst, row) = (rect(sx, sy, sw, sh), rect(dx, dy, dw, dh), clut.min(255));
            let (brightness, mirror) = (byte(brightness), [mirror_u, mirror_v]);
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_page_sprite_mirrored(
                    art,
                    src,
                    dst,
                    row,
                    brightness,
                    Tint::White,
                    mirror,
                );
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, row) {
                Some(hd) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, mirror);
                    let scale = crate::render::brightness_scale(brightness);
                    let colour = sprite_colour(Tint::White, scale);
                    framebuffer.draw_replaced(hd, corners, uv, colour, |framebuffer| {
                        if whole == Some(true) {
                            framebuffer.punch_rect(dst);
                        }
                        draw(framebuffer);
                    });
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "page_sprite_scaled",
        |_lua,
         api,
         (handle, sx, sy, sw, sh, dx, dy, dw, dh, clut, scale, tint): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            usize,
            i64,
            Option<Value>,
        )| {
            let tint = match tint {
                Some(Value::Nil) | None => crate::font::Tint::White,
                Some(value) => colour_tint(&value)?,
            };
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let art = host.assets.page(handle)?;
            let (src, dst, row) = (rect(sx, sy, sw, sh), rect(dx, dy, dw, dh), clut.min(255));
            let scale = byte(scale);
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_page_sprite_scaled(art, src, dst, row, scale, tint);
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, row) {
                Some(hd) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, [false; 2]);
                    let colour = sprite_colour(tint, scale);
                    framebuffer.draw_replaced(hd, corners, uv, colour, draw);
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );
}
