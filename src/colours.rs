//! minetest_game's flowers, dyes and wool (mods/flowers, mods/dye,
//! mods/wool): eight flowers and two mushrooms growing wild, and a
//! waterlily on still water; a dye of each of 15 colours, four from a
//! flower (black from coal, violet from blueberries), and more from
//! mixing two; and wool of each colour, from any wool and a dye.

/// The 15 colours, as indexes in minetest_game's order (dye.dyes): white,
/// grey, dark grey, black, violet, blue, cyan, dark green, green, yellow,
/// brown, orange, red, magenta, pink.
pub const WHITE: usize = 0;
pub const GREY: usize = 1;
pub const DARK_GREY: usize = 2;
pub const BLACK: usize = 3;
pub const VIOLET: usize = 4;
pub const BLUE: usize = 5;
pub const CYAN: usize = 6;
pub const DARK_GREEN: usize = 7;
pub const GREEN: usize = 8;
pub const YELLOW: usize = 9;
pub const BROWN: usize = 10;
pub const ORANGE: usize = 11;
pub const RED: usize = 12;
pub const MAGENTA: usize = 13;
pub const PINK: usize = 14;

/// Node ids, after the building stairs: the flowers, the mushrooms, the
/// waterlily, then the coloured wool (white wool is farming's).
pub const FIRST_FLOWER: u16 = 332;
pub const MUSHROOM_RED: u16 = 340;
pub const MUSHROOM_BROWN: u16 = 341;
pub const WATERLILY: u16 = 342;
const FIRST_WOOL: u16 = 343;
/// Dye items.
const FIRST_DYE: u16 = 0x101E;

/// The flowers, with the colour of dye they give (flowers.datas).
pub const FLOWERS: [(&str, &str, usize); 8] = [
    ("rose", "Red Rose", RED),
    ("tulip", "Orange Tulip", ORANGE),
    ("dandelion_yellow", "Yellow Dandelion", YELLOW),
    ("chrysanthemum_green", "Green Chrysanthemum", GREEN),
    ("geranium", "Blue Geranium", BLUE),
    ("viola", "Viola", VIOLET),
    ("dandelion_white", "White Dandelion", WHITE),
    ("tulip_black", "Black Tulip", BLACK),
];

pub fn flower(i: usize) -> u16 {
    FIRST_FLOWER + i as u16
}

pub fn is_flower(id: u16) -> bool {
    (FIRST_FLOWER..FIRST_FLOWER + FLOWERS.len() as u16).contains(&id)
}

pub fn is_mushroom(id: u16) -> bool {
    matches!(id, MUSHROOM_RED | MUSHROOM_BROWN)
}

/// The dye of a colour.
pub const fn dye(colour: usize) -> u16 {
    FIRST_DYE + colour as u16
}

/// The wool of a colour (white is farming's).
pub const fn wool(colour: usize) -> u16 {
    if colour == WHITE { crate::farming::WOOL_WHITE } else { FIRST_WOOL + colour as u16 - 1 }
}

/// What eating a mushroom does to your health (item_eat): the brown one
/// heals 1, the red one is poisonous.
pub fn mushroom_hp(id: u16) -> Option<i32> {
    match id {
        MUSHROOM_BROWN => Some(1),
        MUSHROOM_RED => Some(-5),
        _ => None,
    }
}


/// The waterlily's boxes: a thin pad on the water (node box), and what
/// is pointed at.
pub const LILY_BOX: [f32; 6] = [-0.5, -31.0 / 64.0, -0.5, 0.5, -15.0 / 32.0, 0.5];
pub const LILY_SELECTION: [f32; 6] = [-7.0 / 16.0, -0.5, -7.0 / 16.0, 7.0 / 16.0, -15.0 / 32.0, 7.0 / 16.0];

/// A flower's or mushroom's selection box (flowers.datas).
pub fn selection_box(id: u16) -> [f32; 6] {
    const S: f32 = 1.0 / 16.0;
    let (r, top) = match id {
        _ if id == flower(0) => (2.0, 5.0),
        _ if id == flower(1) || id == flower(7) => (2.0, 3.0),
        _ if id == flower(2) => (4.0, -2.0),
        _ if id == flower(3) => (4.0, -1.0),
        _ if id == flower(4) => (2.0, 2.0),
        _ if id == flower(5) => (5.0, -1.0),
        _ if id == flower(6) => (5.0, -2.0),
        MUSHROOM_RED => (4.0, -1.0),
        _ => (3.0, -2.0),
    };
    [-r * S, -0.5, -r * S, r * S, top * S, r * S]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(flower(7) + 1, MUSHROOM_RED);
        assert_eq!(wool(GREY), FIRST_WOOL);
        assert_eq!(wool(PINK), 356);
        assert_eq!(wool(WHITE), crate::farming::WOOL_WHITE);
        assert_eq!(dye(PINK), 0x102C);
        assert!(is_flower(flower(3)) && !is_flower(MUSHROOM_RED));
        assert_eq!(mushroom_hp(MUSHROOM_RED), Some(-5));
    }
}
