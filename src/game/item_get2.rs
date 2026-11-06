//! The art of RE2's get-item window (`0x4FD560`): the item's still picture
//! and its copy flying into the slot it takes. The window's steps are
//! [`super::Re2ItemGet`]'s; this is the drawing state its routines keep.

/// Where the window's art is placed: `0x691FAC` and `0x691FAE`, which the
/// window shares with CHECK's picture panel.
pub const ORIGIN: [i16; 2] = [7, 78];

/// The still picture (`0x4FFF70` with its argument set): 112x72 at
/// (`0x10`, `0x0A`) from [`ORIGIN`].
pub const PICTURE: [i16; 4] = [0x10, 0x0A, 0x70, 0x48];

/// Each slot's start of the fly (`0x53DCA8`, the personal slot last): the
/// picture's left and top, its step down a tick and how many ticks it
/// moves.
const FLY_STARTS: [[i8; 4]; 11] = [
    [16, 8, -1, 13],
    [14, 9, -1, 15],
    [16, 11, 1, 13],
    [14, 11, 1, 15],
    [17, 15, 3, 13],
    [14, 10, 3, 15],
    [18, 8, 6, 13],
    [14, 10, 5, 15],
    [17, 11, 8, 13],
    [14, 10, 7, 15],
    [14, 2, -3, 15],
];

/// How many ticks sub 2 runs (`0x692FBC` counts to `0x12`); the picture is
/// drawn on the tick sub 1 starts it and on each of these but the last.
pub const FLY_TICKS: u8 = 0x12;

/// The flying picture (`0x692FAE` to `0x692FBA`, `0x691F64`), from
/// [`ORIGIN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fly {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
    dy: i16,
    steps: u8,
}

impl Default for Fly {
    /// The window's start (`0x4FFF37`): the still picture's place and size.
    fn default() -> Self {
        Self {
            x: PICTURE[0],
            y: PICTURE[1],
            w: PICTURE[2],
            h: PICTURE[3],
            dy: 0,
            steps: 0,
        }
    }
}

impl Fly {
    /// Sub 1's start of the fly to `slot` (`0x4FD79A`; 10 is the personal
    /// slot). The size is what the window's start or the last fly left.
    pub fn start(&mut self, slot: usize) {
        let [x, y, dy, steps] = FLY_STARTS[slot.min(FLY_STARTS.len() - 1)];
        self.x = i16::from(x);
        self.y = i16::from(y);
        self.dy = i16::from(dy);
        self.steps = steps as u8;
    }

    /// One tick of sub 2 (`0x4FD7E6`): while steps are left, down by the
    /// step, right by eight more than the steps left, and smaller by 6 and
    /// 4 down to an icon's 45x30.
    pub fn step(&mut self) {
        if self.steps == 0 {
            return;
        }
        self.y += self.dy;
        self.x += i16::from(self.steps) + 8;
        self.w = (self.w - 6).max(0x2D);
        self.h = (self.h - 4).max(0x1E);
        self.steps -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fly_starts_on_the_picture() {
        let fly = Fly::default();
        assert_eq!([fly.x, fly.y, fly.w, fly.h], PICTURE);
    }

    #[test]
    fn the_fly_ends_on_its_slot() {
        // The grid's slots are 40x30 from (225, 73), two to a row.
        for slot in 0..10 {
            let mut fly = Fly::default();
            fly.start(slot);
            for _ in 0..FLY_TICKS {
                fly.step();
            }
            let x = ORIGIN[0] + fly.x;
            let y = ORIGIN[1] + fly.y;
            let column = slot as i16 % 2;
            let row = slot as i16 / 2;
            assert_eq!([fly.w, fly.h], [45, 30], "slot {slot}");
            assert!((x - (225 + 40 * column)).abs() <= 8, "slot {slot}: x {x}");
            assert!((y - (73 + 30 * row)).abs() <= 2, "slot {slot}: y {y}");
        }
    }

    #[test]
    fn the_fly_to_the_third_slot_steps_as_the_original() {
        let mut fly = Fly::default();
        fly.start(2);
        assert_eq!([fly.x, fly.y], [16, 11]);
        fly.step();
        assert_eq!([fly.x, fly.y, fly.w, fly.h], [37, 12, 106, 68]);
        for _ in 1..13 {
            fly.step();
        }
        assert_eq!([fly.x, fly.y, fly.w, fly.h], [211, 24, 45, 30]);
        fly.step();
        assert_eq!([fly.x, fly.y], [211, 24]);
    }
}
