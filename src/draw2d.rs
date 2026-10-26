//! The GPU backend's 2D pass: a frame's 2D drawing as a list of quads.
//!
//! While the GPU renderer draws a frame, a [`crate::render::Framebuffer`]
//! holds a [`DrawList`] and its 2D primitives (sprites, glyphs, rects, fades,
//! lines, backgrounds and image copies) append [`Item`]s to it instead of
//! writing pixels. The renderer draws the items in order over black into a
//! target at the scene's resolution ([`crate::gpu::GpuRenderer::present`]),
//! so the gameplay scene, the examine viewer's model and the pack's HD art
//! are all drawn where the frame's drawing puts them, under whatever is drawn
//! after them.
//!
//! A sprite's texels are decoded into the frame's [`Atlas`] (palette, tint
//! and brightness applied, as a GPU engine converts an indexed page into a
//! texture) and the GPU samples them through the item's [`Map`]: each target
//! pixel takes the texel of the frame pixel it lies in, with the integer
//! nearest mapping the software rasterizer uses, so at one target pixel per
//! frame pixel the pass draws the software frame's pixels. The blends are
//! the GPU's: a half blend or a fade rounds where the software path
//! truncates, one unit at most.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::htexture::{Model, QuadImage, Recoloured, Replacement};

/// The atlas's width in texels.
pub const ATLAS_WIDTH: u32 = 2048;
/// The most rows a frame's atlas holds: every Vulkan device samples a
/// texture this large.
pub const ATLAS_MAX_HEIGHT: u32 = 4096;

/// [`Map::flags`]: the source is mirrored horizontally.
pub const MIRROR_X: u32 = 1;
/// [`Map::flags`]: the source is mirrored vertically.
pub const MIRROR_Y: u32 = 2;
/// [`Map::flags`]: the source is sampled at the pixel centres and clamped to
/// its edges, as [`crate::render::Framebuffer::draw_background`] stretches a
/// background over the logical frame.
pub const CENTRED: u32 = 4;
/// [`Map::flags`]: an HD item is kept to the opaque texels of the original
/// art its map samples in the atlas.
pub const MASKED: u32 = 8;

/// [`Map::flags`]: a solid item that only marks the rect an HD item covers
/// whole ([`crate::render::Framebuffer::punch_rect`]); it draws nothing
/// itself.
pub const PUNCH: u32 = 16;

/// What an [`Item`] draws.
#[derive(Clone, Debug)]
pub enum Kind {
    /// The item's colour.
    Solid,
    /// The atlas's texels through the item's [`Map`], times its colour.
    Sprite,
    /// A pack's HD image of 2D art.
    Art(Arc<Replacement>),
    /// A pack's HD image of an indexed page, recoloured per texel by the
    /// original's index.
    Recoloured(Arc<Recoloured>),
    /// The examine viewer's model, rendered into a target of its own first.
    Model(Arc<Model>),
    /// The gameplay scene the frame recorded.
    Scene,
}

impl From<QuadImage> for Kind {
    fn from(image: QuadImage) -> Self {
        match image {
            QuadImage::Art(art) => Self::Art(art),
            QuadImage::Recoloured(recoloured) => Self::Recoloured(recoloured),
            QuadImage::Model(model) => Self::Model(model),
        }
    }
}

/// How a target pixel finds its texel in the atlas.
///
/// The frame pixel `(x, y)` a target pixel lies in samples source texel
/// `(x - dst.x) * src.w / dst.w` across (mirrored when the flag says so),
/// the same for rows, and the texel sits at that plus `atlas` in the atlas.
/// [`CENTRED`] samples `((x - dst.x) * 2 + 1) * src.w / (2 * dst.w)`,
/// clamped to the source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Map {
    /// `[x, y, w, h]` in frame pixels the source stretches over.
    pub dst: [i32; 4],
    /// The source's `[w, h]` in texels.
    pub src: [i32; 2],
    /// Added to a source texel to address the atlas.
    pub atlas: [i32; 2],
    pub flags: u32,
}

impl Map {
    /// The source texel frame pixel `(x, y)` samples, as the GPU computes
    /// it.
    pub fn texel(&self, x: i32, y: i32) -> [i32; 2] {
        let axis = |pixel: i32, axis: usize, mirror: u32| {
            let (start, extent, source) = (self.dst[axis], self.dst[axis + 2], self.src[axis]);
            let local = i64::from(pixel) - i64::from(start);
            let offset = if self.flags & CENTRED != 0 {
                ((local * 2 + 1).max(0) * i64::from(source) / (2 * i64::from(extent)))
                    .min(i64::from(source) - 1) as i32
            } else {
                (local * i64::from(source) / i64::from(extent)) as i32
            };
            if self.flags & mirror != 0 {
                source - 1 - offset
            } else {
                offset
            }
        };
        [axis(x, 0, MIRROR_X), axis(y, 1, MIRROR_Y)]
    }
}

/// One quad of the 2D pass, in frame pixels of the [`DrawList`]'s frame.
#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Kind,
    /// `[x0, y0, x1, y1]` the item covers.
    pub rect: [f32; 4],
    /// `[u0, v0, u1, v1]` at `rect`'s corners, normalised over the item's
    /// image (art, a model's target, the scene); a mirrored draw swaps a
    /// pair.
    pub uv: [f32; 4],
    /// `[x0, y0, x1, y1]` the item is cut to.
    pub clip: [i32; 4],
    /// The colour (a solid item's) or colour multiplier, and in alpha how
    /// much of what lies under the item it covers.
    pub colour: [f32; 4],
    /// Where a sprite's texels, or the mask of an HD item, are in the atlas.
    pub map: Map,
}

/// The sprite texels a frame draws, packed into rows of [`ATLAS_WIDTH`]
/// RGBA texels on shelves.
///
/// A block identical to one already stored this frame is stored once. The
/// texels outlive [`DrawList::rewind`], so a frame redrawing what the frame
/// before drew finds its blocks already in place and leaves the atlas (and
/// its [`Atlas::revision`]) as it was, and the GPU does not upload it again.
#[derive(Debug, Default)]
pub struct Atlas {
    /// `ATLAS_WIDTH` texels per row, at least `height` rows.
    pub texels: Vec<u8>,
    /// The rows in use.
    pub height: u32,
    /// The open shelf's `[x, y, height]`.
    shelf: [u32; 3],
    /// Each stored block's origin by its size and contents.
    stored: HashMap<u64, [u32; 2]>,
    /// Changes whenever any atlas's texels change: the GPU compares it with
    /// the revision it uploaded.
    pub revision: u64,
}

impl Clone for Atlas {
    fn clone(&self) -> Self {
        let mut atlas = Self::default();
        atlas.clone_from(self);
        atlas
    }

    /// Copies into the existing buffers: a session's frame is copied to
    /// the app's every frame.
    fn clone_from(&mut self, source: &Self) {
        if self.revision != source.revision || self.texels.len() != source.texels.len() {
            self.texels.clone_from(&source.texels);
        }
        self.height = source.height;
        self.shelf = source.shelf;
        self.stored.clone_from(&source.stored);
        self.revision = source.revision;
    }
}

impl Clone for DrawList {
    fn clone(&self) -> Self {
        let mut list = Self::default();
        list.clone_from(self);
        list
    }

    fn clone_from(&mut self, source: &Self) {
        self.size = source.size;
        self.items.clone_from(&source.items);
        self.atlas.clone_from(&source.atlas);
        self.offset = source.offset;
        self.replacing = source.replacing;
        self.mark = source.mark;
        self.error = source.error;
    }
}

/// The source of [`Atlas::revision`]s, unique across every atlas.
static REVISION: AtomicU64 = AtomicU64::new(1);

impl Atlas {
    /// Store the `width` x `height` block `texels` (RGBA rows) and return
    /// its origin, or `None` when the atlas is full.
    pub fn insert(&mut self, width: u32, height: u32, texels: &[u8]) -> Option<[u32; 2]> {
        debug_assert_eq!(texels.len(), width as usize * height as usize * 4);
        if width == 0 || height == 0 || width > ATLAS_WIDTH {
            return None;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (width, height).hash(&mut hasher);
        texels.hash(&mut hasher);
        let key = hasher.finish();
        if let Some(&origin) = self.stored.get(&key) {
            return Some(origin);
        }
        let [mut x, mut y, mut shelf_h] = self.shelf;
        if x + width > ATLAS_WIDTH {
            (x, y, shelf_h) = (0, y + shelf_h, 0);
        }
        if y + height > ATLAS_MAX_HEIGHT {
            return None;
        }
        shelf_h = shelf_h.max(height);
        self.shelf = [x + width, y, shelf_h];
        self.height = self.height.max(y + shelf_h);
        let stride = ATLAS_WIDTH as usize * 4;
        let needed = self.height as usize * stride;
        // New rows are new texels to the GPU, whatever they hold.
        let mut changed = self.texels.len() < needed;
        if changed {
            self.texels.resize(needed, 0);
        }
        let row_bytes = width as usize * 4;
        for (row, source) in texels.chunks_exact(row_bytes).enumerate() {
            let start = (y as usize + row) * stride + x as usize * 4;
            let target = &mut self.texels[start..start + row_bytes];
            if target != source {
                target.copy_from_slice(source);
                changed = true;
            }
        }
        if changed {
            self.revision = REVISION.fetch_add(1, Ordering::Relaxed);
        }
        self.stored.insert(key, [x, y]);
        Some([x, y])
    }

    /// The packing state [`DrawList::rewind`] returns to.
    fn cursor(&self) -> (u32, [u32; 3]) {
        (self.height, self.shelf)
    }

    /// Pack from `cursor` on again, keeping the texels.
    fn restore(&mut self, (height, shelf): (u32, [u32; 3])) {
        self.height = height;
        self.shelf = shelf;
        self.stored.clear();
    }
}

/// A frame's 2D drawing, in order (see the [module docs](self)).
#[derive(Debug, Default)]
pub struct DrawList {
    /// The frame's `[width, height]`.
    pub size: [u32; 2],
    pub items: Vec<Item>,
    pub atlas: Atlas,
    /// Where the framebuffer drawing into the list sits in the frame: a
    /// band or the 4:3 columns of a canvas draw at an offset.
    pub offset: [i32; 2],
    /// The first item of the draw being replaced by HD art
    /// ([`crate::render::Framebuffer::draw_replaced`]).
    pub replacing: Option<usize>,
    /// What [`DrawList::rewind`] keeps: the items and the atlas packing of
    /// the frame a frozen screen draws over.
    mark: Option<(usize, (u32, [u32; 3]))>,
    /// Why a draw could not be recorded (the atlas filled up); the GPU
    /// refuses to present the frame.
    pub error: Option<&'static str>,
}

impl DrawList {
    /// An empty `size` frame.
    pub fn new(size: [u32; 2]) -> Self {
        Self {
            size,
            ..Self::default()
        }
    }

    /// Start the frame over as black, keeping the atlas's texels.
    pub fn reset(&mut self) {
        self.items.clear();
        self.atlas.restore((0, [0; 3]));
        self.offset = [0, 0];
        self.replacing = None;
        self.mark = None;
        self.error = None;
    }

    /// Make the items drawn so far what [`DrawList::rewind`] returns to.
    pub fn mark(&mut self) {
        self.mark = Some((self.items.len(), self.atlas.cursor()));
    }

    /// Drop the items drawn since [`DrawList::mark`] (all of them without
    /// one), the way a frozen frame is drawn over again.
    pub fn rewind(&mut self) {
        match self.mark {
            Some((items, cursor)) => {
                self.items.truncate(items);
                self.atlas.restore(cursor);
                self.offset = [0, 0];
                self.replacing = None;
                self.error = None;
            }
            None => self.reset(),
        }
    }

    /// Append `item`, dropping it when its clip leaves nothing. An opaque
    /// solid item over the whole frame starts the list over: nothing drawn
    /// before it shows.
    pub fn push(&mut self, item: Item) {
        let [x0, y0, x1, y1] = item.clip;
        let [rx0, ry0, rx1, ry1] = item.rect;
        if x0 >= x1
            || y0 >= y1
            || rx0 >= rx1
            || ry0 >= ry1
            || rx1 <= x0 as f32
            || ry1 <= y0 as f32
            || rx0 >= x1 as f32
            || ry0 >= y1 as f32
        {
            return;
        }
        let [width, height] = self.size.map(|side| side as i32);
        let covers = x0 <= 0 && y0 <= 0 && x1 >= width && y1 >= height;
        let covers =
            covers && rx0 <= 0.0 && ry0 <= 0.0 && rx1 >= width as f32 && ry1 >= height as f32;
        let opaque = matches!(item.kind, Kind::Solid)
            && item.colour[3] >= 1.0
            && item.map.flags & PUNCH == 0;
        if covers && opaque && self.replacing.is_none() {
            self.items.clear();
            self.atlas.restore((0, [0; 3]));
            self.mark = None;
        }
        self.items.push(item);
    }

    /// Store `texels` in the atlas, noting a full atlas as the frame's
    /// error.
    pub fn store(&mut self, width: u32, height: u32, texels: &[u8]) -> Option<[u32; 2]> {
        let origin = self.atlas.insert(width, height, texels);
        if origin.is_none() && width > 0 && height > 0 {
            self.error = Some("the 2D atlas is full");
        }
        origin
    }

    /// Record a mapped draw: `map` stretches the source over frame pixels
    /// and `visible` (`[x0, y0, x1, y1]`, non-empty) is the part of it
    /// drawn. Only the texels those pixels sample are stored, each taken
    /// from `sample` by its source coordinates (`None` draws nothing there).
    pub fn sprite(
        &mut self,
        mut map: Map,
        visible: [i32; 4],
        colour: [f32; 4],
        mut sample: impl FnMut(i32, i32) -> Option<[u8; 4]>,
    ) {
        let [x0, y0, x1, y1] = visible;
        if x0 >= x1 || y0 >= y1 || map.dst[2] <= 0 || map.dst[3] <= 0 {
            return;
        }
        let [first_x, first_y] = map.texel(x0, y0);
        let [last_x, last_y] = map.texel(x1 - 1, y1 - 1);
        let (left, right) = (first_x.min(last_x), first_x.max(last_x));
        let (top, bottom) = (first_y.min(last_y), first_y.max(last_y));
        let (width, height) = ((right - left + 1) as u32, (bottom - top + 1) as u32);
        if width > ATLAS_WIDTH || height > ATLAS_MAX_HEIGHT {
            self.error = Some("a 2D draw is larger than the atlas");
            return;
        }
        let mut texels = vec![0; width as usize * height as usize * 4];
        for (row, line) in texels.chunks_exact_mut(width as usize * 4).enumerate() {
            for (column, texel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                if let Some(rgba) = sample(left + column as i32, top + row as i32) {
                    *texel = [rgba[0], rgba[1], rgba[2], 255];
                }
            }
        }
        let Some([atlas_x, atlas_y]) = self.store(width, height, &texels) else {
            return;
        };
        map.atlas = [atlas_x as i32 - left, atlas_y as i32 - top];
        self.push(Item {
            kind: Kind::Sprite,
            rect: visible.map(|side| side as f32),
            uv: [0.0; 4],
            clip: visible,
            colour,
            map,
        });
    }

    /// Whether the list holds a [`Kind::Scene`] item.
    pub fn shows_scene(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item.kind, Kind::Scene))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_samples_like_the_software_nearest_mapping() {
        let map = Map {
            dst: [10, 20, 8, 4],
            src: [4, 2],
            atlas: [0, 0],
            flags: 0,
        };
        assert_eq!(map.texel(10, 20), [0, 0]);
        assert_eq!(map.texel(11, 21), [0, 0]);
        assert_eq!(map.texel(12, 22), [1, 1]);
        assert_eq!(map.texel(17, 23), [3, 1]);
        let mirrored = Map {
            flags: MIRROR_X | MIRROR_Y,
            ..map
        };
        assert_eq!(mirrored.texel(10, 20), [3, 1]);
        assert_eq!(mirrored.texel(17, 23), [0, 0]);
    }

    #[test]
    fn a_centred_map_samples_pixel_centres_and_clamps() {
        let map = Map {
            dst: [-4, 0, 320, 240],
            src: [160, 120],
            atlas: [0, 0],
            flags: CENTRED,
        };
        assert_eq!(map.texel(-4, 0), [0, 0]);
        assert_eq!(map.texel(-10, 0), [0, 0]);
        assert_eq!(map.texel(-3, 1), [0, 0]);
        assert_eq!(map.texel(-2, 2), [1, 1]);
        assert_eq!(map.texel(400, 300), [159, 119]);
    }

    #[test]
    fn the_atlas_stores_an_identical_block_once() {
        let mut atlas = Atlas::default();
        let red = [255, 0, 0, 255].repeat(4);
        let green = [0, 255, 0, 255].repeat(4);
        let first = atlas.insert(2, 2, &red).unwrap();
        let second = atlas.insert(2, 2, &green).unwrap();
        assert_eq!(atlas.insert(2, 2, &red), Some(first));
        assert_ne!(first, second);
        assert_eq!(atlas.height, 2);
        // A wide block opens a new shelf.
        let wide = vec![0; ATLAS_WIDTH as usize * 4];
        assert_eq!(atlas.insert(ATLAS_WIDTH, 1, &wide), Some([0, 2]));
        assert_eq!(atlas.height, 3);
    }

    #[test]
    fn a_redrawn_frame_leaves_the_atlas_revision_alone() {
        let mut list = DrawList::new([320, 240]);
        let red = [255, 0, 0, 255].repeat(4);
        list.store(2, 2, &red).unwrap();
        list.mark();
        let green = [0, 255, 0, 255].repeat(4);
        let origin = list.store(2, 2, &green).unwrap();
        let revision = list.atlas.revision;
        list.rewind();
        assert_eq!(list.store(2, 2, &green), Some(origin));
        assert_eq!(list.atlas.revision, revision, "the same texels in place");
        list.rewind();
        let blue = [0, 0, 255, 255].repeat(4);
        assert_eq!(list.store(2, 2, &blue), Some(origin));
        assert_ne!(list.atlas.revision, revision);
    }

    #[test]
    fn a_full_atlas_is_the_frames_error() {
        let mut list = DrawList::new([320, 240]);
        let tall = vec![0; ATLAS_MAX_HEIGHT as usize * 4];
        assert!(list.store(1, ATLAS_MAX_HEIGHT, &tall).is_some());
        // A full-width row cannot share the tall block's shelf.
        let wide = vec![1; ATLAS_WIDTH as usize * 4];
        assert!(list.store(ATLAS_WIDTH, 1, &wide).is_none());
        assert!(list.error.is_some());
    }

    #[test]
    fn rewinding_keeps_the_marked_items() {
        let mut list = DrawList::new([320, 240]);
        let item = |x: f32| Item {
            kind: Kind::Solid,
            rect: [x, 0.0, x + 1.0, 1.0],
            uv: [0.0; 4],
            clip: [0, 0, 320, 240],
            colour: [1.0; 4],
            map: Map::default(),
        };
        list.push(item(0.0));
        list.mark();
        list.push(item(1.0));
        list.rewind();
        assert_eq!(list.items.len(), 1);
        // An item its clip hides is not kept.
        list.push(Item {
            clip: [5, 5, 5, 9],
            ..item(5.0)
        });
        assert_eq!(list.items.len(), 1);
        list.reset();
        list.rewind();
        assert!(list.items.is_empty());
    }
}
