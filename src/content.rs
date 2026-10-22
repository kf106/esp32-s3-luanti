//! The game content sent to clients: item definitions, node definitions and
//! media (textures).
//!
//! The serialisation follows Luanti 5.17 (protocol 53): ItemDefinition and
//! ContentFeatures in src/itemdef.cpp and src/nodedef.cpp. Both formats end
//! with optional sections that the client only reads if present, so we write
//! up to the last section we need and let the client fill in defaults.

use std::fs;

use log::{info, warn};

use crate::messages::Writer;
use crate::sha1::sha1;
use crate::beds;
use crate::doors;
use crate::farming;
use crate::stairs;

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

pub struct Node {
    pub id: u16,
    pub name: &'static str,
    pub description: &'static str,
    /// Textures in Luanti order: +Y (top), -Y (bottom), +X, -X, +Z, -Z.
    pub tiles: [&'static str; 6],
    pub groups: &'static [(&'static str, i16)],
    /// What digging it gives (minetest_game: grass gives dirt), and how many.
    pub drop: u16,
    pub drop_count: u16,
    pub kind: Kind,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// An ordinary full cube.
    Solid,
    /// A torch: a small 3D model mounted on the floor, ceiling or a wall
    /// (param2 says which way), gives light, can be walked through, and
    /// falls off when what it hangs on is dug. As in minetest_game there
    /// is one node per model; the item is always the floor one.
    Torch,
    /// Leaves and needles: solid to walk on, drawn with see-through gaps
    /// (Luanti's allfaces_optional), letting light through but casting shade
    /// (light_propagates, not sunlight_propagates, as in minetest_game).
    Leaves,
    /// A furnace: a full cube turned to face the player who placed it
    /// (facedir), with its own inventory window on right-click.
    Furnace,
    /// A sapling: a small plant (plantlike, two crossed pictures) that can
    /// be walked through, stands on the node below and grows into a tree.
    Sapling,
    /// An apple hanging in a tree (plantlike, walked through, eaten).
    Apple,
    /// Where an apple was picked: invisible, and an apple grows there again
    /// (minetest_game's apple_mark: airlike, can't be pointed at, and
    /// anything placed there replaces it).
    Mark,
    /// A chest: a cube turned to face whoever placed it, opened with a
    /// right-click.
    Chest,
    /// A chest while someone has it open: minetest_game's open-lid model.
    ChestOpen,
    /// Glass: see-through, framed (minetest_game's glasslike_framed_optional:
    /// connected frames if the player has "connected glass" on).
    Glass,
    /// A ladder: a flat picture against a wall (signlike, wallmounted),
    /// walked through and climbed.
    Ladder,
    /// A stair or slab (stairs.rs): boxes (nodebox) turned by facedir, with
    /// light passing into it as minetest_game's paramtype "light" lets it.
    Shaped,
    /// The bottom of a door (doors.rs): a model two nodes tall, turned by
    /// facedir, opened and closed with a right-click.
    Door,
    /// The invisible node above a door's bottom, keeping its top half free.
    DoorHidden,
    /// A trapdoor (doors.rs), closed or open: a panel (nodebox) turned by
    /// facedir, opened and closed with a right-click.
    Trapdoor,
    /// Water: see-through, swum in, built into, not pointed at (except
    /// with a bucket). Light passes, dimming, as through leaves.
    Liquid,
    /// Tall grass, wild cotton and crops (farming.rs): plantlike, walked
    /// through, built over, and standing on the node below.
    Plant,
    /// A planted seed: a flat picture on the soil (signlike), standing on
    /// it.
    Seed,
    /// A bed's bottom half (beds.rs), the item: boxes turned by facedir,
    /// slept in with a right-click.
    Bed,
    /// A bed's top half: it drops nothing, and goes with the bottom.
    BedTop,
}

pub const CONTENT_STONE: u16 = 0;
pub const CONTENT_DIRT: u16 = 1;
pub const CONTENT_GRASS: u16 = 2;
pub const CONTENT_SAND: u16 = 3;
pub const CONTENT_SANDSTONE: u16 = 4;
pub const CONTENT_SNOW: u16 = 5;
pub const CONTENT_COAL: u16 = 6;
pub const CONTENT_IRON: u16 = 7;
pub const CONTENT_TORCH: u16 = 8;
pub const CONTENT_TORCH_WALL: u16 = 9;
pub const CONTENT_TORCH_CEILING: u16 = 10;
pub const CONTENT_BIRCH_TREE: u16 = 11;
pub const CONTENT_BIRCH_LEAVES: u16 = 12;
pub const CONTENT_FIR_TREE: u16 = 13;
pub const CONTENT_FIR_NEEDLES: u16 = 14;
pub const CONTENT_BIRCH_WOOD: u16 = 15;
pub const CONTENT_FIR_WOOD: u16 = 16;
pub const CONTENT_COBBLE: u16 = 17;
pub const CONTENT_FURNACE: u16 = 18;
pub const CONTENT_FURNACE_ACTIVE: u16 = 19;
pub const CONTENT_BIRCH_SAPLING: u16 = 20;
pub const CONTENT_FIR_SAPLING: u16 = 21;
pub const CONTENT_APPLE_TREE: u16 = 22;
pub const CONTENT_APPLE_LEAVES: u16 = 23;
pub const CONTENT_APPLE: u16 = 24;
pub const CONTENT_APPLE_MARK: u16 = 25;
pub const CONTENT_APPLE_SAPLING: u16 = 26;
pub const CONTENT_APPLE_WOOD: u16 = 27;
pub const CONTENT_CHEST: u16 = 28;
pub const CONTENT_CHEST_OPEN: u16 = 29;
pub const CONTENT_CHEST_LOCKED: u16 = 30;
pub const CONTENT_CHEST_LOCKED_OPEN: u16 = 31;
pub const CONTENT_STONE_BRICK: u16 = 32;
pub const CONTENT_STONE_BLOCK: u16 = 33;
pub const CONTENT_SANDSTONE_BRICK: u16 = 34;
pub const CONTENT_SANDSTONE_BLOCK: u16 = 35;
pub const CONTENT_COAL_BLOCK: u16 = 36;
pub const CONTENT_STEEL_BLOCK: u16 = 37;
pub const CONTENT_GLASS: u16 = 38;
pub const CONTENT_LADDER_WOOD: u16 = 39;
pub const CONTENT_LADDER_STEEL: u16 = 40;
/// Water (minetest_game's water_source and water_flowing), after the
/// stairs, doors and trapdoors.
pub const CONTENT_WATER: u16 = 98;
pub const CONTENT_WATER_FLOWING: u16 = 99;
/// minetest_game's lava, and the obsidian it cools into, after the beds.
pub const CONTENT_LAVA: u16 = 135;
pub const CONTENT_LAVA_FLOWING: u16 = 136;
pub const CONTENT_OBSIDIAN: u16 = 137;
/// Light lava glows with (minetest_game's LIGHT_MAX - 1). Only the client
/// uses it, to draw lava bright: the server spreads no light from lava.
const LAVA_GLOW: u8 = 13;
/// Damage per second in lava (minetest_game's 4 * 2).
pub const LAVA_DAMAGE: u16 = 8;
/// Health an apple gives (minetest_game's item_eat(2)).
pub const APPLE_HP: u16 = 2;
/// Leaves give a sapling one time in this many (minetest_game's rarity).
pub const SAPLING_RARITY: u32 = 20;
/// Light a burning furnace gives (minetest_game's furnace_active).
pub const FURNACE_LIGHT: u8 = 8;

/// Items that aren't nodes (minetest_game's craftitems) have ids from
/// 0x1000 up, well clear of node content ids.
pub const ITEM_STICK: u16 = 0x1000;
pub const ITEM_COAL_LUMP: u16 = 0x1001;
pub const ITEM_IRON_LUMP: u16 = 0x1002;
pub const ITEM_STEEL_INGOT: u16 = 0x1003;
/// Door items (doors.rs): minetest_game's are craftitems that place the
/// door's two nodes.
pub const ITEM_DOOR_WOOD: u16 = 0x1004;
pub const ITEM_DOOR_STEEL: u16 = 0x1005;
pub const ITEM_DOOR_GLASS: u16 = 0x1006;
/// minetest_game's buckets.
pub const ITEM_BUCKET_EMPTY: u16 = 0x1007;
pub const ITEM_BUCKET_WATER: u16 = 0x1008;
/// Farming's harvests and what is made of them (farming.rs).
pub const ITEM_WHEAT: u16 = 0x1009;
pub const ITEM_FLOUR: u16 = 0x100A;
pub const ITEM_BREAD: u16 = 0x100B;
pub const ITEM_COTTON: u16 = 0x100C;
pub const ITEM_STRING: u16 = 0x100D;
pub const ITEM_BUCKET_LAVA: u16 = 0x100E;
/// Health bread gives (minetest_game's item_eat(5)).
pub const BREAD_HP: u16 = 5;
/// Tools have ids from 0x2000 up.
pub const TOOL_PICK_WOOD: u16 = 0x2000;
pub const TOOL_PICK_STONE: u16 = 0x2001;
pub const TOOL_SHOVEL_WOOD: u16 = 0x2002;
pub const TOOL_SHOVEL_STONE: u16 = 0x2003;
pub const TOOL_AXE_WOOD: u16 = 0x2004;
pub const TOOL_AXE_STONE: u16 = 0x2005;
pub const TOOL_SWORD_WOOD: u16 = 0x2006;
pub const TOOL_SWORD_STONE: u16 = 0x2007;
pub const TOOL_PICK_STEEL: u16 = 0x2008;
pub const TOOL_SHOVEL_STEEL: u16 = 0x2009;
pub const TOOL_AXE_STEEL: u16 = 0x200A;
pub const TOOL_SWORD_STEEL: u16 = 0x200B;
pub const TOOL_HOE_WOOD: u16 = 0x200C;
pub const TOOL_HOE_STONE: u16 = 0x200D;
pub const TOOL_HOE_STEEL: u16 = 0x200E;
/// Light given off by a torch (minetest_game: LIGHT_MAX - 1 = 13; 12 is
/// slightly gentler and keeps the lit area, and the work, smaller).
pub const TORCH_LIGHT: u8 = 12;

/// A stair or slab: it drops itself.
macro_rules! shaped {
    ($id:expr, $name:literal, $description:literal, $tile:literal, $groups:expr) => {
        Node {
            id: $id,
            name: concat!("esp:", $name),
            description: $description,
            tiles: [$tile; 6],
            groups: $groups,
            drop: $id,
            drop_count: 1,
            kind: Kind::Shaped,
        }
    };
}

/// A door's bottom node: it drops the door item.
macro_rules! door {
    ($id:expr, $name:literal, $description:literal, $tile:literal, $groups:expr, $item:expr) => {
        Node {
            id: $id,
            name: concat!("esp:", $name),
            description: $description,
            tiles: [$tile; 6],
            groups: $groups,
            drop: $item,
            drop_count: 1,
            kind: Kind::Door,
        }
    };
}

/// A plant (Kind::Plant): what it drops is farming::drops.
macro_rules! plant {
    ($id:expr, $name:literal, $description:literal, $tile:literal, $groups:expr) => {
        Node {
            id: $id,
            name: concat!("esp:", $name),
            description: $description,
            tiles: [$tile; 6],
            groups: $groups,
            drop: $id,
            drop_count: 0,
            kind: Kind::Plant,
        }
    };
}

/// A planted seed: it gives itself back.
macro_rules! seed {
    ($id:expr, $name:literal, $description:literal, $tile:literal) => {
        Node {
            id: $id,
            name: concat!("esp:", $name),
            description: $description,
            tiles: [$tile; 6],
            groups: &[("seed", 1), ("snappy", 3), ("attached_node", 1), ("flammable", 2), ("grassland", 1)],
            drop: $id,
            drop_count: 1,
            kind: Kind::Seed,
        }
    };
}

/// Content IDs 125, 126 and 127 are reserved for unknown, air and ignore.
/// Listed in id order, without gaps except those three, so that an id
/// finds its node at once.
pub const NODES: &[Node] = &[
    Node {
        id: CONTENT_STONE,
        name: "esp:stone",
        description: "Stone",
        tiles: ["esp_stone.png"; 6],
        groups: &[("cracky", 3), ("stone", 1)],
        // minetest_game: stone gives cobblestone.
        drop: CONTENT_COBBLE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_DIRT,
        name: "esp:dirt",
        description: "Dirt",
        tiles: ["esp_dirt.png"; 6],
        groups: &[("crumbly", 3), ("soil", 1)],
        drop: CONTENT_DIRT,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_GRASS,
        name: "esp:dirt_with_grass",
        description: "Dirt with Grass",
        tiles: [
            "esp_grass.png",
            "esp_dirt.png",
            "esp_grass_side.png",
            "esp_grass_side.png",
            "esp_grass_side.png",
            "esp_grass_side.png",
        ],
        groups: &[("crumbly", 3), ("soil", 1)],
        drop: CONTENT_DIRT,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_SAND,
        name: "esp:sand",
        description: "Sand",
        tiles: ["esp_sand.png"; 6],
        groups: &[("crumbly", 3), ("sand", 1)],
        drop: CONTENT_SAND,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_SANDSTONE,
        name: "esp:sandstone",
        description: "Sandstone",
        tiles: ["esp_sandstone.png"; 6],
        groups: &[("crumbly", 1), ("cracky", 3)],
        drop: CONTENT_SANDSTONE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_SNOW,
        name: "esp:dirt_with_snow",
        description: "Dirt with Snow",
        tiles: [
            "esp_snow.png",
            "esp_dirt.png",
            "esp_snow_side.png",
            "esp_snow_side.png",
            "esp_snow_side.png",
            "esp_snow_side.png",
        ],
        groups: &[("crumbly", 3), ("soil", 1)],
        drop: CONTENT_DIRT,
        drop_count: 1,
        kind: Kind::Solid,
    },
    // Ores give lumps, as in minetest_game.
    Node {
        id: CONTENT_COAL,
        name: "esp:stone_with_coal",
        description: "Coal Ore",
        tiles: ["esp_stone_with_coal.png"; 6],
        groups: &[("cracky", 3)],
        drop: ITEM_COAL_LUMP,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_IRON,
        name: "esp:stone_with_iron",
        description: "Iron Ore",
        tiles: ["esp_stone_with_iron.png"; 6],
        // Needs a stone pickaxe or better.
        groups: &[("cracky", 2)],
        drop: ITEM_IRON_LUMP,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_TORCH,
        name: "esp:torch",
        description: "Torch",
        tiles: [TORCH_TILE; 6],
        groups: &[("dig_immediate", 3), ("attached_node", 1), ("torch", 1)],
        drop: CONTENT_TORCH,
        drop_count: 1,
        kind: Kind::Torch,
    },
    Node {
        id: CONTENT_TORCH_WALL,
        name: "esp:torch_wall",
        description: "Torch",
        tiles: [TORCH_TILE; 6],
        groups: &[("dig_immediate", 3), ("not_in_creative_inventory", 1), ("attached_node", 1), ("torch", 1)],
        drop: CONTENT_TORCH,
        drop_count: 1,
        kind: Kind::Torch,
    },
    Node {
        id: CONTENT_TORCH_CEILING,
        name: "esp:torch_ceiling",
        description: "Torch",
        tiles: [TORCH_TILE; 6],
        groups: &[("dig_immediate", 3), ("not_in_creative_inventory", 1), ("attached_node", 1), ("torch", 1)],
        drop: CONTENT_TORCH,
        drop_count: 1,
        kind: Kind::Torch,
    },
    // Trees, as minetest_game's: trunks are dug (slowly) by hand through
    // oddly_breakable_by_hand, leaves quickly through snappy.
    Node {
        id: CONTENT_BIRCH_TREE,
        name: "esp:birch_tree",
        description: "Birch Tree",
        tiles: [
            "esp_birch_tree_top.png",
            "esp_birch_tree_top.png",
            "esp_birch_tree.png",
            "esp_birch_tree.png",
            "esp_birch_tree.png",
            "esp_birch_tree.png",
        ],
        groups: &[("tree", 1), ("choppy", 3), ("oddly_breakable_by_hand", 1), ("flammable", 2)],
        drop: CONTENT_BIRCH_TREE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_BIRCH_LEAVES,
        name: "esp:birch_leaves",
        description: "Birch Leaves",
        tiles: ["esp_birch_leaves.png"; 6],
        groups: &[("snappy", 3), ("leaves", 1), ("flammable", 2)],
        drop: CONTENT_BIRCH_LEAVES,
        drop_count: 1,
        kind: Kind::Leaves,
    },
    Node {
        id: CONTENT_FIR_TREE,
        name: "esp:fir_tree",
        description: "Fir Tree",
        tiles: [
            "esp_fir_tree_top.png",
            "esp_fir_tree_top.png",
            "esp_fir_tree.png",
            "esp_fir_tree.png",
            "esp_fir_tree.png",
            "esp_fir_tree.png",
        ],
        groups: &[("tree", 1), ("choppy", 3), ("oddly_breakable_by_hand", 1), ("flammable", 2)],
        drop: CONTENT_FIR_TREE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_FIR_NEEDLES,
        name: "esp:fir_needles",
        description: "Fir Needles",
        tiles: ["esp_fir_needles.png"; 6],
        groups: &[("snappy", 3), ("leaves", 1), ("flammable", 2)],
        drop: CONTENT_FIR_NEEDLES,
        drop_count: 1,
        kind: Kind::Leaves,
    },
    // Planks, as minetest_game's aspen and pine wood: four from a trunk.
    Node {
        id: CONTENT_BIRCH_WOOD,
        name: "esp:birch_wood",
        description: "Birch Wood Planks",
        tiles: ["esp_birch_wood.png"; 6],
        groups: &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("wood", 1)],
        drop: CONTENT_BIRCH_WOOD,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_FIR_WOOD,
        name: "esp:fir_wood",
        description: "Fir Wood Planks",
        tiles: ["esp_fir_wood.png"; 6],
        groups: &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("wood", 1)],
        drop: CONTENT_FIR_WOOD,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_COBBLE,
        name: "esp:cobble",
        description: "Cobblestone",
        tiles: ["esp_cobble.png"; 6],
        groups: &[("cracky", 3), ("stone", 2)],
        drop: CONTENT_COBBLE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    // minetest_game's furnace and its textures (CC BY-SA 3.0, see
    // media/LICENSE-items.txt). Cracky 2: a stone pickaxe digs it.
    Node {
        id: CONTENT_FURNACE,
        name: "esp:furnace",
        description: "Furnace",
        tiles: [
            "default_furnace_top.png",
            "default_furnace_bottom.png",
            "default_furnace_side.png",
            "default_furnace_side.png",
            "default_furnace_side.png",
            "default_furnace_front.png",
        ],
        groups: &[("cracky", 2)],
        drop: CONTENT_FURNACE,
        drop_count: 1,
        kind: Kind::Furnace,
    },
    // A furnace while it burns: the fire shows at the front, it glows,
    // and digging it gives an ordinary furnace.
    Node {
        id: CONTENT_FURNACE_ACTIVE,
        name: "esp:furnace_active",
        description: "Furnace",
        tiles: [
            "default_furnace_top.png",
            "default_furnace_bottom.png",
            "default_furnace_side.png",
            "default_furnace_side.png",
            "default_furnace_side.png",
            FURNACE_ACTIVE_TILE,
        ],
        groups: &[("cracky", 2), ("not_in_creative_inventory", 1)],
        drop: CONTENT_FURNACE,
        drop_count: 1,
        kind: Kind::Furnace,
    },
    Node {
        id: CONTENT_BIRCH_SAPLING,
        name: "esp:birch_sapling",
        description: "Birch Sapling",
        tiles: ["default_aspen_sapling.png"; 6],
        groups: SAPLING_GROUPS,
        drop: CONTENT_BIRCH_SAPLING,
        drop_count: 1,
        kind: Kind::Sapling,
    },
    Node {
        id: CONTENT_FIR_SAPLING,
        name: "esp:fir_sapling",
        description: "Fir Sapling",
        tiles: ["default_pine_sapling.png"; 6],
        groups: SAPLING_GROUPS,
        drop: CONTENT_FIR_SAPLING,
        drop_count: 1,
        kind: Kind::Sapling,
    },
    Node {
        id: CONTENT_APPLE_TREE,
        name: "esp:apple_tree",
        description: "Apple Tree",
        tiles: [
            "esp_apple_tree_top.png",
            "esp_apple_tree_top.png",
            "esp_apple_tree.png",
            "esp_apple_tree.png",
            "esp_apple_tree.png",
            "esp_apple_tree.png",
        ],
        groups: &[("tree", 1), ("choppy", 2), ("oddly_breakable_by_hand", 1), ("flammable", 2)],
        drop: CONTENT_APPLE_TREE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_APPLE_LEAVES,
        name: "esp:apple_leaves",
        description: "Apple Tree Leaves",
        tiles: ["esp_apple_leaves.png"; 6],
        groups: &[("snappy", 3), ("leafdecay", 3), ("flammable", 2), ("leaves", 1)],
        drop: CONTENT_APPLE_LEAVES,
        drop_count: 1,
        kind: Kind::Leaves,
    },
    // minetest_game's apple and its picture (CC BY-SA 3.0).
    Node {
        id: CONTENT_APPLE,
        name: "esp:apple",
        description: "Apple",
        tiles: ["default_apple.png"; 6],
        groups: &[("fleshy", 3), ("dig_immediate", 3), ("flammable", 2), ("leafdecay", 3), ("leafdecay_drop", 1), ("food_apple", 1)],
        drop: CONTENT_APPLE,
        drop_count: 1,
        kind: Kind::Apple,
    },
    Node {
        id: CONTENT_APPLE_MARK,
        name: "esp:apple_mark",
        description: "Apple Marker",
        tiles: [""; 6],
        groups: &[("not_in_creative_inventory", 1)],
        drop: CONTENT_APPLE_MARK,
        drop_count: 0,
        kind: Kind::Mark,
    },
    Node {
        id: CONTENT_APPLE_SAPLING,
        name: "esp:apple_sapling",
        description: "Apple Tree Sapling",
        tiles: ["default_sapling.png"; 6],
        groups: SAPLING_GROUPS,
        drop: CONTENT_APPLE_SAPLING,
        drop_count: 1,
        kind: Kind::Sapling,
    },
    Node {
        id: CONTENT_APPLE_WOOD,
        name: "esp:apple_wood",
        description: "Apple Wood Planks",
        tiles: ["esp_apple_wood.png"; 6],
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("wood", 1)],
        drop: CONTENT_APPLE_WOOD,
        drop_count: 1,
        kind: Kind::Solid,
    },
    // minetest_game's chests, with its textures and open-lid model
    // (CC BY-SA 3.0, see media/LICENSE-items.txt). Closed, the faces are
    // arranged as minetest_game does for its plain cube: the front at -Z
    // (towards whoever placed it), the side texture mirrored at +X.
    Node {
        id: CONTENT_CHEST,
        name: "esp:chest",
        description: "Chest",
        tiles: CHEST_CLOSED_TILES,
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2)],
        drop: CONTENT_CHEST,
        drop_count: 1,
        kind: Kind::Chest,
    },
    Node {
        id: CONTENT_CHEST_OPEN,
        name: "esp:chest_open",
        description: "Chest",
        tiles: CHEST_OPEN_TILES,
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("not_in_creative_inventory", 1)],
        drop: CONTENT_CHEST,
        drop_count: 1,
        kind: Kind::ChestOpen,
    },
    Node {
        id: CONTENT_CHEST_LOCKED,
        name: "esp:chest_locked",
        description: "Locked Chest",
        tiles: CHEST_LOCKED_CLOSED_TILES,
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2)],
        drop: CONTENT_CHEST_LOCKED,
        drop_count: 1,
        kind: Kind::Chest,
    },
    Node {
        id: CONTENT_CHEST_LOCKED_OPEN,
        name: "esp:chest_locked_open",
        description: "Locked Chest",
        tiles: CHEST_LOCKED_OPEN_TILES,
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("not_in_creative_inventory", 1)],
        drop: CONTENT_CHEST_LOCKED,
        drop_count: 1,
        kind: Kind::ChestOpen,
    },
    // Building blocks from minetest_game's recipes. The stone and sandstone
    // ones are drawn to match our stone and sandstone; the others use
    // minetest_game's textures (CC BY-SA 3.0, see media/LICENSE-items.txt).
    Node {
        id: CONTENT_STONE_BRICK,
        name: "esp:stonebrick",
        description: "Stone Brick",
        tiles: ["esp_stone_brick.png"; 6],
        groups: &[("cracky", 2), ("stone", 1)],
        drop: CONTENT_STONE_BRICK,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_STONE_BLOCK,
        name: "esp:stone_block",
        description: "Stone Block",
        tiles: ["esp_stone_block.png"; 6],
        groups: &[("cracky", 2), ("stone", 1)],
        drop: CONTENT_STONE_BLOCK,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_SANDSTONE_BRICK,
        name: "esp:sandstonebrick",
        description: "Sandstone Brick",
        tiles: ["esp_sandstone_brick.png"; 6],
        groups: &[("cracky", 2)],
        drop: CONTENT_SANDSTONE_BRICK,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_SANDSTONE_BLOCK,
        name: "esp:sandstone_block",
        description: "Sandstone Block",
        tiles: ["esp_sandstone_block.png"; 6],
        groups: &[("cracky", 2)],
        drop: CONTENT_SANDSTONE_BLOCK,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_COAL_BLOCK,
        name: "esp:coalblock",
        description: "Coal Block",
        tiles: ["default_coal_block.png"; 6],
        groups: &[("cracky", 3)],
        drop: CONTENT_COAL_BLOCK,
        drop_count: 1,
        kind: Kind::Solid,
    },
    // Level 2: only a steel pickaxe digs it.
    Node {
        id: CONTENT_STEEL_BLOCK,
        name: "esp:steelblock",
        description: "Steel Block",
        tiles: ["default_steel_block.png"; 6],
        groups: &[("cracky", 1), ("level", 2)],
        drop: CONTENT_STEEL_BLOCK,
        drop_count: 1,
        kind: Kind::Solid,
    },
    // The frame picture, then the detail, as minetest_game's two tiles
    // fill the six.
    Node {
        id: CONTENT_GLASS,
        name: "esp:glass",
        description: "Glass",
        tiles: [
            "default_glass.png",
            "default_glass_detail.png",
            "default_glass_detail.png",
            "default_glass_detail.png",
            "default_glass_detail.png",
            "default_glass_detail.png",
        ],
        groups: &[("cracky", 3), ("oddly_breakable_by_hand", 3)],
        drop: CONTENT_GLASS,
        drop_count: 1,
        kind: Kind::Glass,
    },
    Node {
        id: CONTENT_LADDER_WOOD,
        name: "esp:ladder_wood",
        description: "Wooden Ladder",
        tiles: ["default_ladder_wood.png"; 6],
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 3), ("flammable", 2)],
        drop: CONTENT_LADDER_WOOD,
        drop_count: 1,
        kind: Kind::Ladder,
    },
    Node {
        id: CONTENT_LADDER_STEEL,
        name: "esp:ladder_steel",
        description: "Steel Ladder",
        tiles: ["default_ladder_steel.png"; 6],
        groups: &[("cracky", 2)],
        drop: CONTENT_LADDER_STEEL,
        drop_count: 1,
        kind: Kind::Ladder,
    },
    // Stairs, slabs and corner stairs (stairs.rs), in its MATERIALS order: minetest_game's
    // groups for each, without "wood" or "stone" (so they don't craft as
    // planks or stone), and the block's own picture on every side.
    shaped!(stairs::FIRST_STAIR + 0, "stair_birch_wood", "Birch Wood Stair", "esp_birch_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 1, "stair_fir_wood", "Fir Wood Stair", "esp_fir_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 2, "stair_apple_wood", "Apple Wood Stair", "esp_apple_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 3, "stair_stone", "Stone Stair", "esp_stone.png", &[("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 4, "stair_cobble", "Cobblestone Stair", "esp_cobble.png", &[("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 5, "stair_stonebrick", "Stone Brick Stair", "esp_stone_brick.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 6, "stair_stone_block", "Stone Block Stair", "esp_stone_block.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 7, "stair_sandstone", "Sandstone Stair", "esp_sandstone.png", &[("crumbly", 1), ("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 8, "stair_sandstonebrick", "Sandstone Brick Stair", "esp_sandstone_brick.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_STAIR + 9, "stair_sandstone_block", "Sandstone Block Stair", "esp_sandstone_block.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_SLAB + 0, "slab_birch_wood", "Birch Wood Slab", "esp_birch_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 1, "slab_fir_wood", "Fir Wood Slab", "esp_fir_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 2, "slab_apple_wood", "Apple Wood Slab", "esp_apple_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 3, "slab_stone", "Stone Slab", "esp_stone.png", &[("cracky", 3), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 4, "slab_cobble", "Cobblestone Slab", "esp_cobble.png", &[("cracky", 3), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 5, "slab_stonebrick", "Stone Brick Slab", "esp_stone_brick.png", &[("cracky", 2), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 6, "slab_stone_block", "Stone Block Slab", "esp_stone_block.png", &[("cracky", 2), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 7, "slab_sandstone", "Sandstone Slab", "esp_sandstone.png", &[("crumbly", 1), ("cracky", 3), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 8, "slab_sandstonebrick", "Sandstone Brick Slab", "esp_sandstone_brick.png", &[("cracky", 2), ("slab", 1)]),
    shaped!(stairs::FIRST_SLAB + 9, "slab_sandstone_block", "Sandstone Block Slab", "esp_sandstone_block.png", &[("cracky", 2), ("slab", 1)]),
    shaped!(stairs::FIRST_INNER + 0, "stair_inner_birch_wood", "Inner Birch Wood Stair", "esp_birch_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 1, "stair_inner_fir_wood", "Inner Fir Wood Stair", "esp_fir_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 2, "stair_inner_apple_wood", "Inner Apple Wood Stair", "esp_apple_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 3, "stair_inner_stone", "Inner Stone Stair", "esp_stone.png", &[("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 4, "stair_inner_cobble", "Inner Cobblestone Stair", "esp_cobble.png", &[("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 5, "stair_inner_stonebrick", "Inner Stone Brick Stair", "esp_stone_brick.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 6, "stair_inner_stone_block", "Inner Stone Block Stair", "esp_stone_block.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 7, "stair_inner_sandstone", "Inner Sandstone Stair", "esp_sandstone.png", &[("crumbly", 1), ("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 8, "stair_inner_sandstonebrick", "Inner Sandstone Brick Stair", "esp_sandstone_brick.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_INNER + 9, "stair_inner_sandstone_block", "Inner Sandstone Block Stair", "esp_sandstone_block.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 0, "stair_outer_birch_wood", "Outer Birch Wood Stair", "esp_birch_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 1, "stair_outer_fir_wood", "Outer Fir Wood Stair", "esp_fir_wood.png", &[("choppy", 3), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 2, "stair_outer_apple_wood", "Outer Apple Wood Stair", "esp_apple_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 3, "stair_outer_stone", "Outer Stone Stair", "esp_stone.png", &[("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 4, "stair_outer_cobble", "Outer Cobblestone Stair", "esp_cobble.png", &[("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 5, "stair_outer_stonebrick", "Outer Stone Brick Stair", "esp_stone_brick.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 6, "stair_outer_stone_block", "Outer Stone Block Stair", "esp_stone_block.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 7, "stair_outer_sandstone", "Outer Sandstone Stair", "esp_sandstone.png", &[("crumbly", 1), ("cracky", 3), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 8, "stair_outer_sandstonebrick", "Outer Sandstone Brick Stair", "esp_sandstone_brick.png", &[("cracky", 2), ("stair", 1)]),
    shaped!(stairs::FIRST_OUTER + 9, "stair_outer_sandstone_block", "Outer Sandstone Block Stair", "esp_sandstone_block.png", &[("cracky", 2), ("stair", 1)]),
    // Doors (doors.rs): wooden, steel and glass, each as nodes a, b, c and d,
    // then the hidden node above them all.
    door!(doors::FIRST_DOOR + 0, "door_wood_a", "Wooden Door", "doors_door_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("door", 1)], ITEM_DOOR_WOOD),
    door!(doors::FIRST_DOOR + 1, "door_wood_b", "Wooden Door", "doors_door_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("door", 1)], ITEM_DOOR_WOOD),
    door!(doors::FIRST_DOOR + 2, "door_wood_c", "Wooden Door", "doors_door_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("door", 1)], ITEM_DOOR_WOOD),
    door!(doors::FIRST_DOOR + 3, "door_wood_d", "Wooden Door", "doors_door_wood.png", &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("door", 1)], ITEM_DOOR_WOOD),
    door!(doors::FIRST_DOOR + 4, "door_steel_a", "Steel Door", "doors_door_steel.png", &[("cracky", 1), ("level", 2), ("door", 1)], ITEM_DOOR_STEEL),
    door!(doors::FIRST_DOOR + 5, "door_steel_b", "Steel Door", "doors_door_steel.png", &[("cracky", 1), ("level", 2), ("door", 1)], ITEM_DOOR_STEEL),
    door!(doors::FIRST_DOOR + 6, "door_steel_c", "Steel Door", "doors_door_steel.png", &[("cracky", 1), ("level", 2), ("door", 1)], ITEM_DOOR_STEEL),
    door!(doors::FIRST_DOOR + 7, "door_steel_d", "Steel Door", "doors_door_steel.png", &[("cracky", 1), ("level", 2), ("door", 1)], ITEM_DOOR_STEEL),
    door!(doors::FIRST_DOOR + 8, "door_glass_a", "Glass Door", "doors_door_glass.png", &[("cracky", 3), ("oddly_breakable_by_hand", 3), ("door", 1)], ITEM_DOOR_GLASS),
    door!(doors::FIRST_DOOR + 9, "door_glass_b", "Glass Door", "doors_door_glass.png", &[("cracky", 3), ("oddly_breakable_by_hand", 3), ("door", 1)], ITEM_DOOR_GLASS),
    door!(doors::FIRST_DOOR + 10, "door_glass_c", "Glass Door", "doors_door_glass.png", &[("cracky", 3), ("oddly_breakable_by_hand", 3), ("door", 1)], ITEM_DOOR_GLASS),
    door!(doors::FIRST_DOOR + 11, "door_glass_d", "Glass Door", "doors_door_glass.png", &[("cracky", 3), ("oddly_breakable_by_hand", 3), ("door", 1)], ITEM_DOOR_GLASS),
    Node {
        id: doors::HIDDEN,
        name: "esp:door_hidden",
        description: "Hidden Door Segment",
        tiles: [""; 6],
        groups: &[("not_in_creative_inventory", 1)],
        drop: doors::HIDDEN,
        drop_count: 0,
        kind: Kind::DoorHidden,
    },
    // Trapdoors, closed then open, with minetest_game's pictures turned
    // as it turns them for each side (CC BY-SA 3.0, see
    // media/LICENSE-doors.txt). The open one gives the closed one.
    Node {
        id: doors::FIRST_TRAPDOOR,
        name: "esp:trapdoor",
        description: "Wooden Trapdoor",
        tiles: TRAPDOOR_WOOD_CLOSED,
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("door", 1)],
        drop: doors::FIRST_TRAPDOOR,
        drop_count: 1,
        kind: Kind::Trapdoor,
    },
    Node {
        id: doors::FIRST_TRAPDOOR + 1,
        name: "esp:trapdoor_open",
        description: "Wooden Trapdoor",
        tiles: TRAPDOOR_WOOD_OPEN,
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2), ("door", 1), ("not_in_creative_inventory", 1)],
        drop: doors::FIRST_TRAPDOOR,
        drop_count: 1,
        kind: Kind::Trapdoor,
    },
    Node {
        id: doors::FIRST_TRAPDOOR + 2,
        name: "esp:trapdoor_steel",
        description: "Steel Trapdoor",
        tiles: TRAPDOOR_STEEL_CLOSED,
        groups: &[("cracky", 1), ("level", 2), ("door", 1)],
        drop: doors::FIRST_TRAPDOOR + 2,
        drop_count: 1,
        kind: Kind::Trapdoor,
    },
    Node {
        id: doors::FIRST_TRAPDOOR + 3,
        name: "esp:trapdoor_steel_open",
        description: "Steel Trapdoor",
        tiles: TRAPDOOR_STEEL_OPEN,
        groups: &[("cracky", 1), ("level", 2), ("door", 1), ("not_in_creative_inventory", 1)],
        drop: doors::FIRST_TRAPDOOR + 2,
        drop_count: 1,
        kind: Kind::Trapdoor,
    },
    // minetest_game's water (CC BY-SA 3.0, see media/LICENSE-items.txt).
    // Still water only, so far: the flowing node is there for the client,
    // which draws a source's edges by it.
    Node {
        id: CONTENT_WATER,
        name: "esp:water_source",
        description: "Water Source",
        tiles: [WATER_SOURCE_TILE; 6],
        groups: &[("water", 3), ("liquid", 3), ("cools_lava", 1)],
        drop: CONTENT_WATER,
        drop_count: 0,
        kind: Kind::Liquid,
    },
    Node {
        id: CONTENT_WATER_FLOWING,
        name: "esp:water_flowing",
        description: "Flowing Water",
        tiles: ["default_water.png"; 6],
        groups: &[("water", 3), ("liquid", 3), ("not_in_creative_inventory", 1), ("cools_lava", 1)],
        drop: CONTENT_WATER_FLOWING,
        drop_count: 0,
        kind: Kind::Liquid,
    },
    // Farming (farming.rs), with minetest_game's pictures (see
    // media/LICENSE-farming.txt). What digging gives is farming::drops.
    plant!(farming::GRASS[0], "grass_1", "Grass", "default_grass_1.png", GRASS_GROUPS),
    plant!(farming::GRASS[1], "grass_2", "Grass", "default_grass_2.png", GRASS_GROUPS),
    plant!(farming::GRASS[2], "grass_3", "Grass", "default_grass_3.png", GRASS_GROUPS),
    plant!(farming::GRASS[3], "grass_4", "Grass", "default_grass_4.png", GRASS_GROUPS),
    plant!(farming::GRASS[4], "grass_5", "Grass", "default_grass_5.png", GRASS_GROUPS),
    plant!(farming::COTTON_WILD, "cotton_wild", "Wild Cotton", "farming_cotton_wild.png", &[("snappy", 3), ("attached_node", 1), ("flammable", 4)]),
    Node {
        id: farming::SOIL,
        name: "esp:soil",
        description: "Soil",
        tiles: ["esp_dirt.png^farming_soil.png", "esp_dirt.png", "esp_dirt.png", "esp_dirt.png", "esp_dirt.png", "esp_dirt.png"],
        groups: &[("crumbly", 3), ("not_in_creative_inventory", 1), ("soil", 2), ("grassland", 1), ("field", 1)],
        drop: CONTENT_DIRT,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: farming::SOIL_WET,
        name: "esp:soil_wet",
        description: "Wet Soil",
        tiles: [
            "esp_dirt.png^farming_soil_wet.png",
            "esp_dirt.png",
            "esp_dirt.png^farming_soil_wet_side.png",
            "esp_dirt.png^farming_soil_wet_side.png",
            "esp_dirt.png^farming_soil_wet_side.png",
            "esp_dirt.png^farming_soil_wet_side.png",
        ],
        groups: &[("crumbly", 3), ("not_in_creative_inventory", 1), ("soil", 3), ("wet", 1), ("grassland", 1), ("field", 1)],
        drop: CONTENT_DIRT,
        drop_count: 1,
        kind: Kind::Solid,
    },
    seed!(farming::SEED_WHEAT, "seed_wheat", "Wheat Seed", "farming_wheat_seed.png"),
    seed!(farming::SEED_COTTON, "seed_cotton", "Cotton Seed", "farming_cotton_seed.png"),
    plant!(farming::WHEAT_STAGES[0], "wheat_1", "Wheat", "farming_wheat_1.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 1)]),
    plant!(farming::WHEAT_STAGES[1], "wheat_2", "Wheat", "farming_wheat_2.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 2)]),
    plant!(farming::WHEAT_STAGES[2], "wheat_3", "Wheat", "farming_wheat_3.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 3)]),
    plant!(farming::WHEAT_STAGES[3], "wheat_4", "Wheat", "farming_wheat_4.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 4)]),
    plant!(farming::WHEAT_STAGES[4], "wheat_5", "Wheat", "farming_wheat_5.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 5)]),
    plant!(farming::WHEAT_STAGES[5], "wheat_6", "Wheat", "farming_wheat_6.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 6)]),
    plant!(farming::WHEAT_STAGES[6], "wheat_7", "Wheat", "farming_wheat_7.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 7)]),
    plant!(farming::WHEAT_STAGES[7], "wheat_8", "Wheat", "farming_wheat_8.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("wheat", 8)]),
    Node {
        id: farming::STRAW,
        name: "esp:straw",
        description: "Straw",
        tiles: ["farming_straw.png"; 6],
        groups: &[("snappy", 3), ("flammable", 4), ("fall_damage_add_percent", -30)],
        drop: farming::STRAW,
        drop_count: 1,
        kind: Kind::Solid,
    },
    Node {
        id: farming::WOOL_WHITE,
        name: "esp:wool_white",
        description: "White Wool",
        tiles: ["wool_white.png"; 6],
        groups: &[("snappy", 2), ("choppy", 2), ("oddly_breakable_by_hand", 3), ("flammable", 3), ("wool", 1), ("color_white", 1)],
        drop: farming::WOOL_WHITE,
        drop_count: 1,
        kind: Kind::Solid,
    },
    plant!(farming::COTTON_STAGES[0], "cotton_1", "Cotton", "farming_cotton_1.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 1)]),
    plant!(farming::COTTON_STAGES[1], "cotton_2", "Cotton", "farming_cotton_2.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 2)]),
    plant!(farming::COTTON_STAGES[2], "cotton_3", "Cotton", "farming_cotton_3.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 3)]),
    plant!(farming::COTTON_STAGES[3], "cotton_4", "Cotton", "farming_cotton_4.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 4)]),
    plant!(farming::COTTON_STAGES[4], "cotton_5", "Cotton", "farming_cotton_5.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 5)]),
    // Luanti's own content ids 125 to 127 come here.
    plant!(farming::COTTON_STAGES[5], "cotton_6", "Cotton", "farming_cotton_6.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 6)]),
    plant!(farming::COTTON_STAGES[6], "cotton_7", "Cotton", "farming_cotton_7.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 7)]),
    plant!(farming::COTTON_STAGES[7], "cotton_8", "Cotton", "farming_cotton_8.png", &[("snappy", 3), ("flammable", 2), ("plant", 1), ("not_in_creative_inventory", 1), ("attached_node", 1), ("cotton", 8)]),
    // Beds (beds.rs), with minetest_game's textures (CC BY-SA 3.0, see
    // media/LICENSE-beds.txt). The top drops nothing.
    Node {
        id: beds::BED_BOTTOM,
        name: "esp:bed_bottom",
        description: "Simple Bed",
        tiles: [
            "beds_bed_top_bottom.png^[transformR90",
            "beds_bed_under.png",
            "beds_bed_side_bottom_r.png",
            "beds_bed_side_bottom_r.png^[transformFX",
            "blank.png",
            "beds_bed_side_bottom.png",
        ],
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("bed", 1)],
        drop: beds::BED_BOTTOM,
        drop_count: 1,
        kind: Kind::Bed,
    },
    Node {
        id: beds::BED_TOP,
        name: "esp:bed_top",
        description: "Simple Bed",
        tiles: [
            "beds_bed_top_top.png^[transformR90",
            "beds_bed_under.png",
            "beds_bed_side_top_r.png",
            "beds_bed_side_top_r.png^[transformFX",
            "beds_bed_side_top.png",
            "blank.png",
        ],
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("bed", 2), ("not_in_creative_inventory", 1)],
        drop: beds::BED_TOP,
        drop_count: 0,
        kind: Kind::BedTop,
    },
    Node {
        id: beds::FANCY_BOTTOM,
        name: "esp:fancy_bed_bottom",
        description: "Fancy Bed",
        tiles: [
            "beds_bed_top1.png",
            "beds_bed_under.png",
            "beds_bed_side1.png",
            "beds_bed_side1.png^[transformFX",
            "beds_bed_foot.png",
            "beds_bed_foot.png",
        ],
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("bed", 1)],
        drop: beds::FANCY_BOTTOM,
        drop_count: 1,
        kind: Kind::Bed,
    },
    Node {
        id: beds::FANCY_TOP,
        name: "esp:fancy_bed_top",
        description: "Fancy Bed",
        tiles: [
            "beds_bed_top2.png",
            "beds_bed_under.png",
            "beds_bed_side2.png",
            "beds_bed_side2.png^[transformFX",
            "beds_bed_head.png",
            "beds_bed_head.png",
        ],
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 3), ("bed", 2), ("not_in_creative_inventory", 1)],
        drop: beds::FANCY_TOP,
        drop_count: 0,
        kind: Kind::BedTop,
    },
    // minetest_game's lava and obsidian (CC BY-SA 3.0, see
    // media/LICENSE-items.txt).
    Node {
        id: CONTENT_LAVA,
        name: "esp:lava_source",
        description: "Lava Source",
        tiles: [LAVA_SOURCE_TILE; 6],
        groups: &[("lava", 3), ("liquid", 2), ("igniter", 1)],
        drop: CONTENT_LAVA,
        drop_count: 0,
        kind: Kind::Liquid,
    },
    Node {
        id: CONTENT_LAVA_FLOWING,
        name: "esp:lava_flowing",
        description: "Flowing Lava",
        tiles: ["default_lava.png"; 6],
        groups: &[("lava", 3), ("liquid", 2), ("igniter", 1), ("not_in_creative_inventory", 1)],
        drop: CONTENT_LAVA_FLOWING,
        drop_count: 0,
        kind: Kind::Liquid,
    },
    Node {
        id: CONTENT_OBSIDIAN,
        name: "esp:obsidian",
        description: "Obsidian",
        tiles: ["default_obsidian.png"; 6],
        groups: &[("cracky", 1), ("level", 2)],
        drop: CONTENT_OBSIDIAN,
        drop_count: 1,
        kind: Kind::Solid,
    },
];

/// minetest_game's tall grass groups.
const GRASS_GROUPS: &[(&str, i16)] = &[("snappy", 3), ("flora", 1), ("attached_node", 1), ("grass", 1), ("normal_grass", 1), ("flammable", 1)];

/// minetest_game's water textures: 16 frames one above another.
const WATER_SOURCE_TILE: &str = "default_water_source_animated.png";
const WATER_FLOWING_TILE: &str = "default_water_flowing_animated.png";
const LAVA_SOURCE_TILE: &str = "default_lava_source_animated.png";
const LAVA_FLOWING_TILE: &str = "default_lava_flowing_animated.png";

// Saplings, from leaves (one time in SAPLING_RARITY), as minetest_game's
// aspen and pine saplings, with their pictures (CC BY-SA 3.0, see
// media/LICENSE-items.txt).
const SAPLING_GROUPS: &[(&str, i16)] =
    &[("snappy", 2), ("dig_immediate", 3), ("flammable", 3), ("attached_node", 1), ("sapling", 1)];

const CHEST_CLOSED_TILES: [&str; 6] = [
    "default_chest_top.png",
    "default_chest_top.png",
    "default_chest_side.png^[transformFX",
    "default_chest_side.png",
    "default_chest_side.png",
    "default_chest_front.png",
];
const CHEST_LOCKED_CLOSED_TILES: [&str; 6] = [
    "default_chest_top.png",
    "default_chest_top.png",
    "default_chest_side.png^[transformFX",
    "default_chest_side.png",
    "default_chest_side.png",
    "default_chest_lock.png",
];
/// The open model's materials, in minetest_game's order.
const CHEST_OPEN_TILES: [&str; 6] = [
    "default_chest_top.png",
    "default_chest_top.png",
    "default_chest_side.png",
    "default_chest_side.png",
    "default_chest_front.png",
    "default_chest_inside.png",
];
const CHEST_LOCKED_OPEN_TILES: [&str; 6] = [
    "default_chest_top.png",
    "default_chest_top.png",
    "default_chest_side.png",
    "default_chest_side.png",
    "default_chest_lock.png",
    "default_chest_inside.png",
];

/// minetest_game's trapdoor tiles: closed, the picture on top and flipped
/// underneath, the edge picture round the sides; open, turned to stand up.
const TRAPDOOR_WOOD_CLOSED: [&str; 6] = [
    "doors_trapdoor.png",
    "doors_trapdoor.png^[transformFY",
    "doors_trapdoor_side.png",
    "doors_trapdoor_side.png",
    "doors_trapdoor_side.png",
    "doors_trapdoor_side.png",
];
const TRAPDOOR_WOOD_OPEN: [&str; 6] = [
    "doors_trapdoor_side.png",
    "doors_trapdoor_side.png^[transform2",
    "doors_trapdoor_side.png^[transform3",
    "doors_trapdoor_side.png^[transform1",
    "doors_trapdoor.png^[transform46",
    "doors_trapdoor.png^[transform6",
];
const TRAPDOOR_STEEL_CLOSED: [&str; 6] = [
    "doors_trapdoor_steel.png",
    "doors_trapdoor_steel.png^[transformFY",
    "doors_trapdoor_steel_side.png",
    "doors_trapdoor_steel_side.png",
    "doors_trapdoor_steel_side.png",
    "doors_trapdoor_steel_side.png",
];
const TRAPDOOR_STEEL_OPEN: [&str; 6] = [
    "doors_trapdoor_steel_side.png",
    "doors_trapdoor_steel_side.png^[transform2",
    "doors_trapdoor_steel_side.png^[transform3",
    "doors_trapdoor_steel_side.png^[transform1",
    "doors_trapdoor_steel.png^[transform46",
    "doors_trapdoor_steel.png^[transform6",
];

/// The open and closed nodes of a chest: (closed, open).
pub fn chest_pair(id: u16) -> Option<(u16, u16)> {
    match id {
        CONTENT_CHEST | CONTENT_CHEST_OPEN => Some((CONTENT_CHEST, CONTENT_CHEST_OPEN)),
        CONTENT_CHEST_LOCKED | CONTENT_CHEST_LOCKED_OPEN => Some((CONTENT_CHEST_LOCKED, CONTENT_CHEST_LOCKED_OPEN)),
        _ => None,
    }
}

pub fn is_chest(id: u16) -> bool {
    chest_pair(id).is_some()
}

pub fn is_locked_chest(id: u16) -> bool {
    matches!(id, CONTENT_CHEST_LOCKED | CONTENT_CHEST_LOCKED_OPEN)
}

/// minetest_game's chest_lid_obstructed: the lid can't open under a block
/// (air, torches and the like are fine).
pub fn lid_obstructed(above: u16) -> bool {
    !(above == crate::world::CONTENT_AIR || is_torch(above) || def(above).is_some_and(|n| n.kind == Kind::Mark))
}

/// minetest_game's burning furnace front: 8 frames of fire, one above
/// another, 1.5 seconds for the cycle.
const FURNACE_ACTIVE_TILE: &str = "default_furnace_front_active.png";

/// An item that is only an item, never a node: crafting material.
pub struct CraftItem {
    pub id: u16,
    pub name: &'static str,
    pub description: &'static str,
    pub image: &'static str,
    pub groups: &'static [(&'static str, i16)],
}

/// minetest_game's craftitems, with its pictures (CC BY-SA 3.0, see
/// media/LICENSE-items.txt).
pub const CRAFTITEMS: &[CraftItem] = &[
    CraftItem {
        id: ITEM_STICK,
        name: "esp:stick",
        description: "Stick",
        image: "default_stick.png",
        groups: &[("stick", 1), ("flammable", 2)],
    },
    CraftItem {
        id: ITEM_COAL_LUMP,
        name: "esp:coal_lump",
        description: "Coal Lump",
        image: "default_coal_lump.png",
        groups: &[("coal", 1), ("flammable", 1)],
    },
    CraftItem {
        id: ITEM_IRON_LUMP,
        name: "esp:iron_lump",
        description: "Iron Lump",
        image: "default_iron_lump.png",
        groups: &[],
    },
    CraftItem {
        id: ITEM_STEEL_INGOT,
        name: "esp:steel_ingot",
        description: "Steel Ingot",
        image: "default_steel_ingot.png",
        groups: &[],
    },
    // minetest_game's doors, with their pictures (CC BY-SA 3.0, see
    // media/LICENSE-doors.txt).
    CraftItem {
        id: ITEM_DOOR_WOOD,
        name: "esp:door_wood",
        description: "Wooden Door",
        image: "doors_item_wood.png",
        groups: &[("choppy", 2), ("oddly_breakable_by_hand", 2), ("flammable", 2)],
    },
    CraftItem {
        id: ITEM_DOOR_STEEL,
        name: "esp:door_steel",
        description: "Steel Door",
        image: "doors_item_steel.png",
        groups: &[("cracky", 1), ("level", 2)],
    },
    CraftItem {
        id: ITEM_DOOR_GLASS,
        name: "esp:door_glass",
        description: "Glass Door",
        image: "doors_item_glass.png",
        groups: &[("cracky", 3), ("oddly_breakable_by_hand", 3)],
    },
    // minetest_game's buckets (bucket mod; CC BY-SA 3.0, see
    // media/LICENSE-items.txt).
    CraftItem {
        id: ITEM_BUCKET_EMPTY,
        name: "esp:bucket_empty",
        description: "Empty Bucket",
        image: "bucket.png",
        groups: &[("tool", 1)],
    },
    CraftItem {
        id: ITEM_BUCKET_WATER,
        name: "esp:bucket_water",
        description: "Water Bucket",
        image: "bucket_water.png",
        groups: &[("tool", 1), ("water_bucket", 1)],
    },
    CraftItem {
        id: ITEM_BUCKET_LAVA,
        name: "esp:bucket_lava",
        description: "Lava Bucket",
        image: "bucket_lava.png",
        groups: &[("tool", 1)],
    },
    // farming's (see media/LICENSE-farming.txt).
    CraftItem {
        id: ITEM_WHEAT,
        name: "esp:wheat",
        description: "Wheat",
        image: "farming_wheat.png",
        groups: &[("food_wheat", 1), ("flammable", 4)],
    },
    CraftItem {
        id: ITEM_FLOUR,
        name: "esp:flour",
        description: "Flour",
        image: "farming_flour.png",
        groups: &[("food_flour", 1), ("flammable", 1)],
    },
    CraftItem {
        id: ITEM_BREAD,
        name: "esp:bread",
        description: "Bread",
        image: "farming_bread.png",
        groups: &[("food_bread", 1), ("flammable", 2)],
    },
    CraftItem {
        id: ITEM_COTTON,
        name: "esp:cotton",
        description: "Cotton",
        image: "farming_cotton.png",
        groups: &[("flammable", 4)],
    },
    CraftItem {
        id: ITEM_STRING,
        name: "esp:string",
        description: "String",
        image: "farming_string.png",
        groups: &[("flammable", 2)],
    },
];

fn craftitem(id: u16) -> Option<&'static CraftItem> {
    CRAFTITEMS.iter().find(|c| c.id == id)
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// What a tool (or the hand) can dig in one group: the time for each
/// rating of the group, how many digs it lasts (0: for ever), and the
/// highest node "level" it can dig (Luanti's ToolGroupCap).
pub struct GroupCap {
    pub group: &'static str,
    pub times: &'static [(i16, f32)],
    pub uses: u32,
    pub maxlevel: i16,
}

/// Luanti's ToolCapabilities.
pub struct ToolCaps {
    pub full_punch_interval: f32,
    pub max_drop_level: i16,
    pub groupcaps: &'static [GroupCap],
    /// Damage to the "fleshy" armour group when punching.
    pub fleshy: i16,
}

/// minetest_game's hand (tools.lua): dirt and sand, leaves, and trunks and
/// planks slowly, but no stone. It never wears out.
pub const HAND: ToolCaps = ToolCaps {
    full_punch_interval: 0.9,
    max_drop_level: 0,
    groupcaps: &[
        GroupCap { group: "crumbly", times: &[(2, 3.0), (3, 0.7)], uses: 0, maxlevel: 1 },
        GroupCap { group: "snappy", times: &[(3, 0.4)], uses: 0, maxlevel: 1 },
        GroupCap { group: "oddly_breakable_by_hand", times: &[(1, 3.5), (2, 2.0), (3, 0.7)], uses: 0, maxlevel: 1 },
    ],
    fleshy: 1,
};

pub struct Tool {
    pub id: u16,
    pub name: &'static str,
    pub description: &'static str,
    pub image: &'static str,
    pub wield_image: &'static str,
    pub caps: ToolCaps,
    pub groups: &'static [(&'static str, i16)],
}

/// minetest_game's wood and stone tools (tools.lua), with its pictures
/// (CC BY-SA 3.0, see media/LICENSE-items.txt). They break with
/// "default_tool_breaks".
pub const TOOLS: &[Tool] = &[
    Tool {
        id: TOOL_PICK_WOOD,
        name: "esp:pick_wood",
        description: "Wooden Pickaxe",
        image: "default_tool_woodpick.png",
        wield_image: "default_tool_woodpick.png",
        caps: ToolCaps {
            full_punch_interval: 1.2,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "cracky", times: &[(3, 1.6)], uses: 10, maxlevel: 1 }],
            fleshy: 2,
        },
        groups: &[("pickaxe", 1), ("flammable", 2)],
    },
    Tool {
        id: TOOL_PICK_STONE,
        name: "esp:pick_stone",
        description: "Stone Pickaxe",
        image: "default_tool_stonepick.png",
        wield_image: "default_tool_stonepick.png",
        caps: ToolCaps {
            full_punch_interval: 1.3,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "cracky", times: &[(2, 2.0), (3, 1.0)], uses: 20, maxlevel: 1 }],
            fleshy: 3,
        },
        groups: &[("pickaxe", 1)],
    },
    Tool {
        id: TOOL_SHOVEL_WOOD,
        name: "esp:shovel_wood",
        description: "Wooden Shovel",
        image: "default_tool_woodshovel.png",
        wield_image: "default_tool_woodshovel.png^[transformR90",
        caps: ToolCaps {
            full_punch_interval: 1.2,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "crumbly", times: &[(1, 3.0), (2, 1.6), (3, 0.6)], uses: 10, maxlevel: 1 }],
            fleshy: 2,
        },
        groups: &[("shovel", 1), ("flammable", 2)],
    },
    Tool {
        id: TOOL_SHOVEL_STONE,
        name: "esp:shovel_stone",
        description: "Stone Shovel",
        image: "default_tool_stoneshovel.png",
        wield_image: "default_tool_stoneshovel.png^[transformR90",
        caps: ToolCaps {
            full_punch_interval: 1.4,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "crumbly", times: &[(1, 1.8), (2, 1.2), (3, 0.5)], uses: 20, maxlevel: 1 }],
            fleshy: 2,
        },
        groups: &[("shovel", 1)],
    },
    Tool {
        id: TOOL_AXE_WOOD,
        name: "esp:axe_wood",
        description: "Wooden Axe",
        image: "default_tool_woodaxe.png",
        wield_image: "default_tool_woodaxe.png",
        caps: ToolCaps {
            full_punch_interval: 1.0,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "choppy", times: &[(2, 3.0), (3, 1.6)], uses: 10, maxlevel: 1 }],
            fleshy: 2,
        },
        groups: &[("axe", 1), ("flammable", 2)],
    },
    Tool {
        id: TOOL_AXE_STONE,
        name: "esp:axe_stone",
        description: "Stone Axe",
        image: "default_tool_stoneaxe.png",
        wield_image: "default_tool_stoneaxe.png",
        caps: ToolCaps {
            full_punch_interval: 1.2,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "choppy", times: &[(1, 3.0), (2, 2.0), (3, 1.3)], uses: 20, maxlevel: 1 }],
            fleshy: 3,
        },
        groups: &[("axe", 1)],
    },
    Tool {
        id: TOOL_SWORD_WOOD,
        name: "esp:sword_wood",
        description: "Wooden Sword",
        image: "default_tool_woodsword.png",
        wield_image: "default_tool_woodsword.png",
        caps: ToolCaps {
            full_punch_interval: 1.0,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "snappy", times: &[(2, 1.6), (3, 0.4)], uses: 10, maxlevel: 1 }],
            fleshy: 2,
        },
        groups: &[("sword", 1), ("flammable", 2)],
    },
    Tool {
        id: TOOL_SWORD_STONE,
        name: "esp:sword_stone",
        description: "Stone Sword",
        image: "default_tool_stonesword.png",
        wield_image: "default_tool_stonesword.png",
        caps: ToolCaps {
            full_punch_interval: 1.2,
            max_drop_level: 0,
            groupcaps: &[GroupCap { group: "snappy", times: &[(2, 1.4), (3, 0.4)], uses: 20, maxlevel: 1 }],
            fleshy: 4,
        },
        groups: &[("sword", 1)],
    },
    Tool {
        id: TOOL_PICK_STEEL,
        name: "esp:pick_steel",
        description: "Steel Pickaxe",
        image: "default_tool_steelpick.png",
        wield_image: "default_tool_steelpick.png",
        caps: ToolCaps {
            full_punch_interval: 1.0,
            max_drop_level: 1,
            groupcaps: &[GroupCap { group: "cracky", times: &[(1, 4.0), (2, 1.6), (3, 0.8)], uses: 20, maxlevel: 2 }],
            fleshy: 4,
        },
        groups: &[("pickaxe", 1)],
    },
    Tool {
        id: TOOL_SHOVEL_STEEL,
        name: "esp:shovel_steel",
        description: "Steel Shovel",
        image: "default_tool_steelshovel.png",
        wield_image: "default_tool_steelshovel.png^[transformR90",
        caps: ToolCaps {
            full_punch_interval: 1.1,
            max_drop_level: 1,
            groupcaps: &[GroupCap { group: "crumbly", times: &[(1, 1.5), (2, 0.9), (3, 0.4)], uses: 30, maxlevel: 2 }],
            fleshy: 3,
        },
        groups: &[("shovel", 1)],
    },
    Tool {
        id: TOOL_AXE_STEEL,
        name: "esp:axe_steel",
        description: "Steel Axe",
        image: "default_tool_steelaxe.png",
        wield_image: "default_tool_steelaxe.png",
        caps: ToolCaps {
            full_punch_interval: 1.0,
            max_drop_level: 1,
            groupcaps: &[GroupCap { group: "choppy", times: &[(1, 2.5), (2, 1.4), (3, 1.0)], uses: 20, maxlevel: 2 }],
            fleshy: 4,
        },
        groups: &[("axe", 1)],
    },
    Tool {
        id: TOOL_SWORD_STEEL,
        name: "esp:sword_steel",
        description: "Steel Sword",
        image: "default_tool_steelsword.png",
        wield_image: "default_tool_steelsword.png",
        caps: ToolCaps {
            full_punch_interval: 0.8,
            max_drop_level: 1,
            groupcaps: &[GroupCap { group: "snappy", times: &[(1, 2.5), (2, 1.2), (3, 0.35)], uses: 30, maxlevel: 2 }],
            fleshy: 6,
        },
        groups: &[("sword", 1)],
    },
    // farming's hoes (hoes.lua). They have no digging of their own: a
    // left-click uses them, to turn dirt into soil (farming.rs).
    Tool {
        id: TOOL_HOE_WOOD,
        name: "esp:hoe_wood",
        description: "Wooden Hoe",
        image: "farming_tool_woodhoe.png",
        wield_image: "farming_tool_woodhoe.png",
        caps: HAND,
        groups: &[("hoe", 1), ("flammable", 2)],
    },
    Tool {
        id: TOOL_HOE_STONE,
        name: "esp:hoe_stone",
        description: "Stone Hoe",
        image: "farming_tool_stonehoe.png",
        wield_image: "farming_tool_stonehoe.png",
        caps: HAND,
        groups: &[("hoe", 1)],
    },
    Tool {
        id: TOOL_HOE_STEEL,
        name: "esp:hoe_steel",
        description: "Steel Hoe",
        image: "farming_tool_steelhoe.png",
        wield_image: "farming_tool_steelhoe.png",
        caps: HAND,
        groups: &[("hoe", 1)],
    },
];

/// The sound a tool makes when it wears out (minetest_game's sound.breaks).
/// The steel tools are minetest_game's too: maxlevel 2 against our level 0
/// blocks, so Luanti halves their times and gives them nine times the uses.
pub const TOOL_BREAKS: &str = "default_tool_breaks";

fn tool(id: u16) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

pub fn is_tool(id: u16) -> bool {
    tool(id).is_some()
}

/// The capabilities an item digs with: a tool's own, otherwise the
/// hand's (ItemStack::getToolCapabilities).
pub fn caps_of(id: Option<u16>) -> &'static ToolCaps {
    id.and_then(tool).map_or(&HAND, |t| &t.caps)
}

/// The wear one use adds to a tool that lasts `uses` uses, spread so that
/// it breaks after exactly that many (Luanti's calculateResultWear).
pub fn use_wear(uses: u32, wear: u16) -> u32 {
    if uses == 0 {
        return 0;
    }
    let normal = 65536 / uses;
    let oversize = 65536 % uses;
    let extra = if oversize > 0 && wear as u32 >= (uses - oversize) * normal { 1 } else { 0 };
    normal + extra
}

/// Whether `caps` can dig a node, and if so how long it takes and how much
/// wear it adds (Luanti's getDigParams).
pub fn dig_params(node: u16, caps: &ToolCaps, wear: u16) -> Option<(f32, u32)> {
    // dig_immediate: fixed times, no wear, whatever the tool.
    if !caps.groupcaps.iter().any(|c| c.group == "dig_immediate") {
        match group_of(node, "dig_immediate") {
            2 => return Some((0.5, 0)),
            3 => return Some((0.0, 0)),
            _ => {}
        }
    }
    let level = group_of(node, "level");
    let mut best: Option<(f32, u32)> = None;
    for cap in caps.groupcaps {
        let leveldiff = cap.maxlevel - level;
        if leveldiff < 0 {
            continue;
        }
        let rating = group_of(node, cap.group);
        let Some(&(_, mut time)) = cap.times.iter().find(|(r, _)| *r == rating) else {
            continue;
        };
        if leveldiff > 1 {
            time /= leveldiff as f32;
        }
        if !matches!(best, Some((t, _)) if time >= t) {
            let uses = (cap.uses as f64 * 3f64.powi(leveldiff as i32)).min(65535.0) as u32;
            best = Some((time, use_wear(uses, wear)));
        }
    }
    best
}

/// minetest_game's torch texture: 16 frames of flame, one above another.
const TORCH_TILE: &str = "esp_torch_animated.png";
const TORCH_FRAMES_LENGTH: f32 = 3.3;
const TORCH_IMAGE: &str = "esp_torch_on_floor.png";

/// The model each torch node uses.
fn torch_mesh(id: u16) -> &'static str {
    match id {
        CONTENT_TORCH_WALL => "esp_torch_wall.obj",
        CONTENT_TORCH_CEILING => "esp_torch_ceiling.obj",
        _ => "esp_torch_floor.obj",
    }
}

/// The torch node for a placement direction (param2: 0 ceiling, 1 floor,
/// otherwise a wall), as minetest_game's torch on_place picks it. Other
/// nodes are returned unchanged.
pub fn torch_variant(id: u16, param2: u8) -> u16 {
    if !is_torch(id) {
        return id;
    }
    match param2 & 7 {
        0 => CONTENT_TORCH_CEILING,
        1 => CONTENT_TORCH,
        _ => CONTENT_TORCH_WALL,
    }
}

fn def(id: u16) -> Option<&'static Node> {
    NODES.get(node_index(id)).filter(|n| n.id == id).or_else(|| NODES.iter().find(|n| n.id == id))
}

/// Where a node is in NODES: its id, less three past Luanti's reserved ids.
fn node_index(id: u16) -> usize {
    if id >= 128 { id as usize - 3 } else { id as usize }
}

/// Light a node gives off (0 for most).
pub fn light_source(id: u16) -> u8 {
    match def(id) {
        Some(n) if n.kind == Kind::Torch => TORCH_LIGHT,
        Some(n) if n.id == CONTENT_FURNACE_ACTIVE => FURNACE_LIGHT,
        _ => 0,
    }
}

/// Whether light passes through a node (fading by one level per node).
pub fn passes_light(id: u16) -> bool {
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| matches!(n.kind, Kind::Torch | Kind::Leaves | Kind::Sapling | Kind::Apple | Kind::Mark | Kind::Chest | Kind::ChestOpen | Kind::Glass | Kind::Ladder | Kind::Shaped | Kind::Door | Kind::DoorHidden | Kind::Trapdoor | Kind::Liquid | Kind::Plant | Kind::Seed | Kind::Bed | Kind::BedTop))
}

/// Whether a node is a liquid (water or lava).
pub fn is_liquid(id: u16) -> bool {
    def(id).is_some_and(|n| n.kind == Kind::Liquid)
}

pub fn is_lava(id: u16) -> bool {
    matches!(id, CONTENT_LAVA | CONTENT_LAVA_FLOWING)
}

/// The light a node is drawn with, whatever light reaches it: torches and
/// a burning furnace (which also light their surroundings), and lava
/// (which doesn't, as far as the server is concerned).
pub fn glow(id: u16) -> u8 {
    if is_lava(id) { LAVA_GLOW } else { light_source(id) }
}

/// Breath a node takes from a player whose head is in it, every 2 s once
/// their breath is gone in HP (minetest_game's water and lava: 1).
pub fn drowning(id: u16) -> u16 {
    is_liquid(id) as u16
}

/// Damage a node does each second to a player in it.
pub fn damage_per_second(id: u16) -> u16 {
    if is_lava(id) { LAVA_DAMAGE } else { 0 }
}

/// Whether water washes a node away (minetest_game's floodable): torches.
pub fn floodable(id: u16) -> bool {
    is_torch(id)
}

/// Whether placing a node can replace this one (builtin's buildable_to):
/// air, an apple mark, or water.
pub fn buildable_to(id: u16) -> bool {
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| matches!(n.kind, Kind::Mark | Kind::Liquid | Kind::Plant))
}

/// Whether direct sunlight carries on down through a node at full
/// strength. Leaves let light through but cast shade.
pub fn passes_sunlight(id: u16) -> bool {
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| matches!(n.kind, Kind::Torch | Kind::Sapling | Kind::Apple | Kind::Mark | Kind::Glass | Kind::Ladder | Kind::Door | Kind::DoorHidden | Kind::Plant | Kind::Seed))
}

/// Whether players stand on a node (and so whether it can hold a torch).
pub fn walkable(id: u16) -> bool {
    def(id).is_some_and(|n| matches!(n.kind, Kind::Solid | Kind::Leaves | Kind::Furnace | Kind::Chest | Kind::ChestOpen | Kind::Glass | Kind::Shaped | Kind::Door | Kind::Trapdoor | Kind::Bed | Kind::BedTop))
}

/// Whether right-clicking a node opens it (and so doesn't place against
/// it, unless the player is sneaking).
pub fn is_furnace(id: u16) -> bool {
    def(id).is_some_and(|n| n.kind == Kind::Furnace)
}

/// The facedir param2 for a node placed at `above` by a player standing at
/// `player` (nodes): the front turned towards the player
/// (builtin's item_place_node with core.dir_to_facedir).
pub fn facedir_param2(above: [i16; 3], player: [f32; 3]) -> u8 {
    let dx = above[0] as f32 - player[0];
    let dz = above[2] as f32 - player[2];
    if dx.abs() > dz.abs() {
        if dx < 0.0 { 3 } else { 1 }
    } else if dz < 0.0 {
        2
    } else {
        0
    }
}

pub fn is_torch(id: u16) -> bool {
    def(id).is_some_and(|n| n.kind == Kind::Torch)
}

/// Nodes that hang on what they were placed against (param2 says which
/// way): torches and ladders.
pub fn is_wallmounted(id: u16) -> bool {
    def(id).is_some_and(|n| matches!(n.kind, Kind::Torch | Kind::Ladder))
}

pub fn is_sapling(id: u16) -> bool {
    def(id).is_some_and(|n| n.kind == Kind::Sapling)
}

pub fn is_leaves(id: u16) -> bool {
    def(id).is_some_and(|n| n.kind == Kind::Leaves)
}

/// The sapling that a kind of leaves gives, and the trunk that keeps them
/// from decaying (minetest_game's register_leafdecay: leaves stay while a
/// trunk of their own tree is within LEAF_DECAY_RADIUS).
pub fn tree_of_leaves(id: u16) -> Option<(u16, u16)> {
    match id {
        CONTENT_BIRCH_LEAVES => Some((CONTENT_BIRCH_SAPLING, CONTENT_BIRCH_TREE)),
        CONTENT_FIR_NEEDLES => Some((CONTENT_FIR_SAPLING, CONTENT_FIR_TREE)),
        CONTENT_APPLE_LEAVES => Some((CONTENT_APPLE_SAPLING, CONTENT_APPLE_TREE)),
        _ => None,
    }
}

/// The trunk that keeps a node from decaying: leaves' own tree, and apples
/// on apple trees (minetest_game registers both for leafdecay).
pub fn decay_trunk(id: u16) -> Option<u16> {
    match id {
        CONTENT_APPLE => Some(CONTENT_APPLE_TREE),
        _ => tree_of_leaves(id).map(|(_, trunk)| trunk),
    }
}

/// Whether a node is a trunk whose digging may make leaves decay.
pub fn is_trunk(id: u16) -> bool {
    matches!(id, CONTENT_BIRCH_TREE | CONTENT_FIR_TREE | CONTENT_APPLE_TREE)
}

pub fn is_apple(id: u16) -> bool {
    id == CONTENT_APPLE
}

pub fn is_mark(id: u16) -> bool {
    id == CONTENT_APPLE_MARK
}

/// minetest_game's leafdecay radius for pine and aspen.
pub const LEAF_DECAY_RADIUS: i32 = 3;

/// For a wall-mounted node, the offset to the node it hangs on, from its
/// param2: 0 ceiling, 1 floor, 2 +X, 3 -X, 4 +Z, 5 -Z (Luanti's order).
pub fn wallmounted_support(param2: u8) -> (i32, i32, i32) {
    match param2 & 7 {
        0 => (0, 1, 0),
        1 => (0, -1, 0),
        2 => (1, 0, 0),
        3 => (-1, 0, 0),
        4 => (0, 0, 1),
        _ => (0, 0, -1),
    }
}

/// The param2 the client predicts when placing a wall-mounted node
/// against `under` from `above` (Game::nodePlacement): which way the
/// support lies.
pub fn wallmounted_param2(under: [i16; 3], above: [i16; 3]) -> u8 {
    let d = [under[0] - above[0], under[1] - above[1], under[2] - above[2]];
    if d[1].abs() > d[0].abs().max(d[2].abs()) {
        if d[1] < 0 { 1 } else { 0 }
    } else if d[0].abs() > d[2].abs() {
        if d[0] < 0 { 3 } else { 2 }
    } else if d[2] < 0 {
        5
    } else {
        4
    }
}

/// Content id for a node name, including the built-in "air".
pub fn node_id_of(name: &str) -> Option<u16> {
    if name == "air" {
        return Some(crate::world::CONTENT_AIR);
    }
    NODES.iter().find(|n| n.name == name).map(|n| n.id)
}

/// Id of any item (node or craftitem) by name.
pub fn id_of(name: &str) -> Option<u16> {
    node_id_of(name)
        .or_else(|| CRAFTITEMS.iter().find(|c| c.name == name).map(|c| c.id))
        .or_else(|| TOOLS.iter().find(|t| t.name == name).map(|t| t.id))
}

pub fn name_of(id: u16) -> &'static str {
    match (def(id), craftitem(id), tool(id)) {
        (Some(n), _, _) => n.name,
        (_, Some(c), _) => c.name,
        (_, _, Some(t)) => t.name,
        _ => "air",
    }
}

pub fn description_of(id: u16) -> &'static str {
    match (def(id), craftitem(id), tool(id)) {
        (Some(n), _, _) => n.description,
        (_, Some(c), _) => c.description,
        (_, _, Some(t)) => t.description,
        _ => "",
    }
}

/// Whether an id is an item a player can hold: any of our nodes,
/// craftitems or tools.
pub fn is_item(id: u16) -> bool {
    def(id).is_some() || craftitem(id).is_some() || tool(id).is_some()
}

/// How many of an item fit in one inventory slot: 99 (Luanti's default),
/// or 1 for tools.
pub fn stack_max(id: u16) -> u16 {
    // A full bucket doesn't stack (minetest_game's stack_max = 1).
    // So doesn't a bed.
    if is_tool(id) || matches!(id, ITEM_BUCKET_WATER | ITEM_BUCKET_LAVA) || beds::is_bed(id) { 1 } else { 99 }
}

/// Whether an item points at liquids, as buckets do: then the client
/// points at water rather than through it.
fn liquids_pointable(id: u16) -> bool {
    matches!(id, ITEM_BUCKET_EMPTY | ITEM_BUCKET_WATER | ITEM_BUCKET_LAVA)
}

/// Whether an item can be placed as a node.
pub fn is_node(id: u16) -> bool {
    def(id).is_some()
}

/// An item's rating in a group (0: not in it), as for "group:wood" in
/// recipes.
pub fn group_of(id: u16, group: &str) -> i16 {
    let groups = match (def(id), craftitem(id), tool(id)) {
        (Some(n), _, _) => n.groups,
        (_, Some(c), _) => c.groups,
        (_, _, Some(t)) => t.groups,
        _ => &[],
    };
    groups.iter().find(|(g, _)| *g == group).map_or(0, |(_, r)| *r)
}

/// The item digging a node gives, if any, and how many.
pub fn drop_of(id: u16) -> Option<(u16, u16)> {
    def(id).filter(|n| n.drop_count > 0).map(|n| (n.drop, n.drop_count))
}

/// Default textures compiled into the firmware. Files in the SD card's media
/// folder with the same name replace them.
const EMBEDDED_MEDIA: &[(&str, &[u8])] = &[
    ("esp_stone.png", include_bytes!("../media/esp_stone.png")),
    ("esp_dirt.png", include_bytes!("../media/esp_dirt.png")),
    ("esp_grass.png", include_bytes!("../media/esp_grass.png")),
    ("esp_grass_side.png", include_bytes!("../media/esp_grass_side.png")),
    ("esp_hand.png", include_bytes!("../media/esp_hand.png")),
    ("esp_sand.png", include_bytes!("../media/esp_sand.png")),
    // Torch models and textures from minetest_game's default mod (CC BY-SA
    // 3.0, see media/LICENSE-torch.txt).
    ("esp_torch_on_floor.png", include_bytes!("../media/esp_torch_on_floor.png")),
    ("esp_torch_animated.png", include_bytes!("../media/esp_torch_animated.png")),
    ("esp_torch_floor.obj", include_bytes!("../media/esp_torch_floor.obj")),
    ("esp_torch_wall.obj", include_bytes!("../media/esp_torch_wall.obj")),
    ("esp_torch_ceiling.obj", include_bytes!("../media/esp_torch_ceiling.obj")),
    ("esp_sandstone.png", include_bytes!("../media/esp_sandstone.png")),
    ("esp_snow.png", include_bytes!("../media/esp_snow.png")),
    ("esp_snow_side.png", include_bytes!("../media/esp_snow_side.png")),
    ("esp_stone_with_coal.png", include_bytes!("../media/esp_stone_with_coal.png")),
    ("esp_stone_with_iron.png", include_bytes!("../media/esp_stone_with_iron.png")),
    // Player model and skin from Minetest Game's player_api (CC BY-SA 3.0,
    // see media/LICENSE-character.txt).
    ("character.b3d", include_bytes!("../media/character.b3d")),
    ("character.png", include_bytes!("../media/character.png")),
    ("esp_birch_tree.png", include_bytes!("../media/esp_birch_tree.png")),
    ("esp_birch_tree_top.png", include_bytes!("../media/esp_birch_tree_top.png")),
    ("esp_birch_leaves.png", include_bytes!("../media/esp_birch_leaves.png")),
    ("esp_fir_tree.png", include_bytes!("../media/esp_fir_tree.png")),
    ("esp_fir_tree_top.png", include_bytes!("../media/esp_fir_tree_top.png")),
    ("esp_fir_needles.png", include_bytes!("../media/esp_fir_needles.png")),
    // Sounds from minetest_game's default mod (see media/LICENSE-sounds.txt),
    // under their original names: the client asks for some by name
    // ("default_dig_<group>" while digging, "player_damage").
    ("default_dig_choppy.1.ogg", include_bytes!("../media/default_dig_choppy.1.ogg")),
    ("default_dig_choppy.2.ogg", include_bytes!("../media/default_dig_choppy.2.ogg")),
    ("default_dig_choppy.3.ogg", include_bytes!("../media/default_dig_choppy.3.ogg")),
    ("default_dig_cracky.1.ogg", include_bytes!("../media/default_dig_cracky.1.ogg")),
    ("default_dig_cracky.2.ogg", include_bytes!("../media/default_dig_cracky.2.ogg")),
    ("default_dig_cracky.3.ogg", include_bytes!("../media/default_dig_cracky.3.ogg")),
    ("default_dig_crumbly.ogg", include_bytes!("../media/default_dig_crumbly.ogg")),
    ("default_dig_dig_immediate.ogg", include_bytes!("../media/default_dig_dig_immediate.ogg")),
    ("default_dig_oddly_breakable_by_hand.ogg", include_bytes!("../media/default_dig_oddly_breakable_by_hand.ogg")),
    ("default_dig_snappy.ogg", include_bytes!("../media/default_dig_snappy.ogg")),
    ("default_dirt_footstep.1.ogg", include_bytes!("../media/default_dirt_footstep.1.ogg")),
    ("default_dirt_footstep.2.ogg", include_bytes!("../media/default_dirt_footstep.2.ogg")),
    ("default_dug_node.1.ogg", include_bytes!("../media/default_dug_node.1.ogg")),
    ("default_dug_node.2.ogg", include_bytes!("../media/default_dug_node.2.ogg")),
    ("default_grass_footstep.1.ogg", include_bytes!("../media/default_grass_footstep.1.ogg")),
    ("default_grass_footstep.2.ogg", include_bytes!("../media/default_grass_footstep.2.ogg")),
    ("default_grass_footstep.3.ogg", include_bytes!("../media/default_grass_footstep.3.ogg")),
    ("default_hard_footstep.1.ogg", include_bytes!("../media/default_hard_footstep.1.ogg")),
    ("default_hard_footstep.2.ogg", include_bytes!("../media/default_hard_footstep.2.ogg")),
    ("default_hard_footstep.3.ogg", include_bytes!("../media/default_hard_footstep.3.ogg")),
    ("default_place_node.1.ogg", include_bytes!("../media/default_place_node.1.ogg")),
    ("default_place_node.2.ogg", include_bytes!("../media/default_place_node.2.ogg")),
    ("default_place_node.3.ogg", include_bytes!("../media/default_place_node.3.ogg")),
    ("default_place_node_hard.1.ogg", include_bytes!("../media/default_place_node_hard.1.ogg")),
    ("default_place_node_hard.2.ogg", include_bytes!("../media/default_place_node_hard.2.ogg")),
    ("default_sand_footstep.1.ogg", include_bytes!("../media/default_sand_footstep.1.ogg")),
    ("default_sand_footstep.2.ogg", include_bytes!("../media/default_sand_footstep.2.ogg")),
    ("default_sand_footstep.3.ogg", include_bytes!("../media/default_sand_footstep.3.ogg")),
    ("default_snow_footstep.1.ogg", include_bytes!("../media/default_snow_footstep.1.ogg")),
    ("default_snow_footstep.2.ogg", include_bytes!("../media/default_snow_footstep.2.ogg")),
    ("default_snow_footstep.3.ogg", include_bytes!("../media/default_snow_footstep.3.ogg")),
    ("default_snow_footstep.4.ogg", include_bytes!("../media/default_snow_footstep.4.ogg")),
    ("default_snow_footstep.5.ogg", include_bytes!("../media/default_snow_footstep.5.ogg")),
    ("default_wood_footstep.1.ogg", include_bytes!("../media/default_wood_footstep.1.ogg")),
    ("default_wood_footstep.2.ogg", include_bytes!("../media/default_wood_footstep.2.ogg")),
    ("player_damage.ogg", include_bytes!("../media/player_damage.ogg")),
    // Extra skins to choose from in the inventory (see skins.rs).
    ("esp_skin_wizard.png", include_bytes!("../media/esp_skin_wizard.png")),
    ("esp_skin_elf.png", include_bytes!("../media/esp_skin_elf.png")),
    ("esp_skin_borgo.png", include_bytes!("../media/esp_skin_borgo.png")),
    ("esp_skin_strider.png", include_bytes!("../media/esp_skin_strider.png")),
    ("esp_birch_wood.png", include_bytes!("../media/esp_birch_wood.png")),
    ("esp_fir_wood.png", include_bytes!("../media/esp_fir_wood.png")),
    ("esp_cobble.png", include_bytes!("../media/esp_cobble.png")),
    // Item pictures and the crafting arrow from minetest_game's default mod
    // (CC BY-SA 3.0, see media/LICENSE-items.txt), under their own names.
    ("default_stick.png", include_bytes!("../media/default_stick.png")),
    ("default_coal_lump.png", include_bytes!("../media/default_coal_lump.png")),
    ("default_iron_lump.png", include_bytes!("../media/default_iron_lump.png")),
    ("gui_furnace_arrow_bg.png", include_bytes!("../media/gui_furnace_arrow_bg.png")),
    ("default_tool_woodpick.png", include_bytes!("../media/default_tool_woodpick.png")),
    ("default_tool_stonepick.png", include_bytes!("../media/default_tool_stonepick.png")),
    ("default_tool_woodshovel.png", include_bytes!("../media/default_tool_woodshovel.png")),
    ("default_tool_stoneshovel.png", include_bytes!("../media/default_tool_stoneshovel.png")),
    ("default_tool_woodaxe.png", include_bytes!("../media/default_tool_woodaxe.png")),
    ("default_tool_stoneaxe.png", include_bytes!("../media/default_tool_stoneaxe.png")),
    ("default_tool_woodsword.png", include_bytes!("../media/default_tool_woodsword.png")),
    ("default_tool_stonesword.png", include_bytes!("../media/default_tool_stonesword.png")),
    ("default_furnace_top.png", include_bytes!("../media/default_furnace_top.png")),
    ("default_furnace_bottom.png", include_bytes!("../media/default_furnace_bottom.png")),
    ("default_furnace_side.png", include_bytes!("../media/default_furnace_side.png")),
    ("default_furnace_front.png", include_bytes!("../media/default_furnace_front.png")),
    ("default_furnace_fire_bg.png", include_bytes!("../media/default_furnace_fire_bg.png")),
    ("default_furnace_front_active.png", include_bytes!("../media/default_furnace_front_active.png")),
    ("default_furnace_fire_fg.png", include_bytes!("../media/default_furnace_fire_fg.png")),
    ("gui_furnace_arrow_fg.png", include_bytes!("../media/gui_furnace_arrow_fg.png")),
    ("default_steel_ingot.png", include_bytes!("../media/default_steel_ingot.png")),
    ("default_tool_steelpick.png", include_bytes!("../media/default_tool_steelpick.png")),
    ("default_tool_steelshovel.png", include_bytes!("../media/default_tool_steelshovel.png")),
    ("default_tool_steelaxe.png", include_bytes!("../media/default_tool_steelaxe.png")),
    ("default_tool_steelsword.png", include_bytes!("../media/default_tool_steelsword.png")),
    ("default_furnace_active.ogg", include_bytes!("../media/default_furnace_active.ogg")),
    ("default_cool_lava.1.ogg", include_bytes!("../media/default_cool_lava.1.ogg")),
    ("default_cool_lava.2.ogg", include_bytes!("../media/default_cool_lava.2.ogg")),
    ("default_cool_lava.3.ogg", include_bytes!("../media/default_cool_lava.3.ogg")),
    ("esp_stone_brick.png", include_bytes!("../media/esp_stone_brick.png")),
    ("esp_stone_block.png", include_bytes!("../media/esp_stone_block.png")),
    ("esp_sandstone_brick.png", include_bytes!("../media/esp_sandstone_brick.png")),
    ("esp_sandstone_block.png", include_bytes!("../media/esp_sandstone_block.png")),
    ("default_coal_block.png", include_bytes!("../media/default_coal_block.png")),
    ("default_steel_block.png", include_bytes!("../media/default_steel_block.png")),
    ("default_glass.png", include_bytes!("../media/default_glass.png")),
    ("default_glass_detail.png", include_bytes!("../media/default_glass_detail.png")),
    ("default_ladder_wood.png", include_bytes!("../media/default_ladder_wood.png")),
    ("default_ladder_steel.png", include_bytes!("../media/default_ladder_steel.png")),
    ("default_glass_footstep.ogg", include_bytes!("../media/default_glass_footstep.ogg")),
    ("default_break_glass.1.ogg", include_bytes!("../media/default_break_glass.1.ogg")),
    ("default_break_glass.2.ogg", include_bytes!("../media/default_break_glass.2.ogg")),
    ("default_break_glass.3.ogg", include_bytes!("../media/default_break_glass.3.ogg")),
    ("default_metal_footstep.1.ogg", include_bytes!("../media/default_metal_footstep.1.ogg")),
    ("default_metal_footstep.2.ogg", include_bytes!("../media/default_metal_footstep.2.ogg")),
    ("default_metal_footstep.3.ogg", include_bytes!("../media/default_metal_footstep.3.ogg")),
    ("default_dig_metal.ogg", include_bytes!("../media/default_dig_metal.ogg")),
    ("default_dug_metal.1.ogg", include_bytes!("../media/default_dug_metal.1.ogg")),
    ("default_dug_metal.2.ogg", include_bytes!("../media/default_dug_metal.2.ogg")),
    ("default_place_node_metal.1.ogg", include_bytes!("../media/default_place_node_metal.1.ogg")),
    ("default_place_node_metal.2.ogg", include_bytes!("../media/default_place_node_metal.2.ogg")),
    ("chest_open.obj", include_bytes!("../media/chest_open.obj")),
    ("default_chest_top.png", include_bytes!("../media/default_chest_top.png")),
    ("default_chest_side.png", include_bytes!("../media/default_chest_side.png")),
    ("default_chest_front.png", include_bytes!("../media/default_chest_front.png")),
    ("default_chest_inside.png", include_bytes!("../media/default_chest_inside.png")),
    ("default_chest_lock.png", include_bytes!("../media/default_chest_lock.png")),
    ("default_chest_open.ogg", include_bytes!("../media/default_chest_open.ogg")),
    ("default_chest_close.ogg", include_bytes!("../media/default_chest_close.ogg")),
    ("esp_apple_tree.png", include_bytes!("../media/esp_apple_tree.png")),
    ("esp_apple_tree_top.png", include_bytes!("../media/esp_apple_tree_top.png")),
    ("esp_apple_leaves.png", include_bytes!("../media/esp_apple_leaves.png")),
    ("esp_apple_wood.png", include_bytes!("../media/esp_apple_wood.png")),
    ("default_apple.png", include_bytes!("../media/default_apple.png")),
    ("default_sapling.png", include_bytes!("../media/default_sapling.png")),
    ("default_aspen_sapling.png", include_bytes!("../media/default_aspen_sapling.png")),
    ("default_pine_sapling.png", include_bytes!("../media/default_pine_sapling.png")),
    ("default_tool_breaks.1.ogg", include_bytes!("../media/default_tool_breaks.1.ogg")),
    ("default_tool_breaks.2.ogg", include_bytes!("../media/default_tool_breaks.2.ogg")),
    ("default_tool_breaks.3.ogg", include_bytes!("../media/default_tool_breaks.3.ogg")),
    // minetest_game's doors (see media/LICENSE-doors.txt).
    ("door_a.b3d", include_bytes!("../media/door_a.b3d")),
    ("door_b.b3d", include_bytes!("../media/door_b.b3d")),
    ("doors_door_wood.png", include_bytes!("../media/doors_door_wood.png")),
    ("doors_door_steel.png", include_bytes!("../media/doors_door_steel.png")),
    ("doors_door_glass.png", include_bytes!("../media/doors_door_glass.png")),
    ("doors_item_wood.png", include_bytes!("../media/doors_item_wood.png")),
    ("doors_item_steel.png", include_bytes!("../media/doors_item_steel.png")),
    ("doors_item_glass.png", include_bytes!("../media/doors_item_glass.png")),
    ("doors_door_open.ogg", include_bytes!("../media/doors_door_open.ogg")),
    ("doors_door_close.ogg", include_bytes!("../media/doors_door_close.ogg")),
    ("doors_steel_door_open.ogg", include_bytes!("../media/doors_steel_door_open.ogg")),
    ("doors_steel_door_close.ogg", include_bytes!("../media/doors_steel_door_close.ogg")),
    ("doors_glass_door_open.ogg", include_bytes!("../media/doors_glass_door_open.ogg")),
    ("doors_glass_door_close.ogg", include_bytes!("../media/doors_glass_door_close.ogg")),
    ("doors_trapdoor.png", include_bytes!("../media/doors_trapdoor.png")),
    ("doors_trapdoor_side.png", include_bytes!("../media/doors_trapdoor_side.png")),
    ("doors_trapdoor_steel.png", include_bytes!("../media/doors_trapdoor_steel.png")),
    ("doors_trapdoor_steel_side.png", include_bytes!("../media/doors_trapdoor_steel_side.png")),
    // minetest_game's water and buckets (see media/LICENSE-items.txt).
    ("default_water_source_animated.png", include_bytes!("../media/default_water_source_animated.png")),
    ("default_water_flowing_animated.png", include_bytes!("../media/default_water_flowing_animated.png")),
    ("default_water.png", include_bytes!("../media/default_water.png")),
    ("default_water_footstep.1.ogg", include_bytes!("../media/default_water_footstep.1.ogg")),
    ("default_water_footstep.2.ogg", include_bytes!("../media/default_water_footstep.2.ogg")),
    ("default_water_footstep.3.ogg", include_bytes!("../media/default_water_footstep.3.ogg")),
    ("bucket.png", include_bytes!("../media/bucket.png")),
    ("bucket_water.png", include_bytes!("../media/bucket_water.png")),
    // minetest_game's farming, tall grass and wool (see media/LICENSE-farming.txt).
    ("farming_soil.png", include_bytes!("../media/farming_soil.png")),
    ("farming_soil_wet.png", include_bytes!("../media/farming_soil_wet.png")),
    ("farming_soil_wet_side.png", include_bytes!("../media/farming_soil_wet_side.png")),
    ("farming_wheat_seed.png", include_bytes!("../media/farming_wheat_seed.png")),
    ("farming_cotton_seed.png", include_bytes!("../media/farming_cotton_seed.png")),
    ("farming_wheat.png", include_bytes!("../media/farming_wheat.png")),
    ("farming_flour.png", include_bytes!("../media/farming_flour.png")),
    ("farming_bread.png", include_bytes!("../media/farming_bread.png")),
    ("farming_cotton.png", include_bytes!("../media/farming_cotton.png")),
    ("farming_string.png", include_bytes!("../media/farming_string.png")),
    ("farming_straw.png", include_bytes!("../media/farming_straw.png")),
    ("farming_cotton_wild.png", include_bytes!("../media/farming_cotton_wild.png")),
    ("farming_tool_woodhoe.png", include_bytes!("../media/farming_tool_woodhoe.png")),
    ("farming_tool_stonehoe.png", include_bytes!("../media/farming_tool_stonehoe.png")),
    ("farming_tool_steelhoe.png", include_bytes!("../media/farming_tool_steelhoe.png")),
    ("farming_wheat_1.png", include_bytes!("../media/farming_wheat_1.png")),
    ("farming_wheat_2.png", include_bytes!("../media/farming_wheat_2.png")),
    ("farming_wheat_3.png", include_bytes!("../media/farming_wheat_3.png")),
    ("farming_wheat_4.png", include_bytes!("../media/farming_wheat_4.png")),
    ("farming_wheat_5.png", include_bytes!("../media/farming_wheat_5.png")),
    ("farming_wheat_6.png", include_bytes!("../media/farming_wheat_6.png")),
    ("farming_wheat_7.png", include_bytes!("../media/farming_wheat_7.png")),
    ("farming_wheat_8.png", include_bytes!("../media/farming_wheat_8.png")),
    ("farming_cotton_1.png", include_bytes!("../media/farming_cotton_1.png")),
    ("farming_cotton_2.png", include_bytes!("../media/farming_cotton_2.png")),
    ("farming_cotton_3.png", include_bytes!("../media/farming_cotton_3.png")),
    ("farming_cotton_4.png", include_bytes!("../media/farming_cotton_4.png")),
    ("farming_cotton_5.png", include_bytes!("../media/farming_cotton_5.png")),
    ("farming_cotton_6.png", include_bytes!("../media/farming_cotton_6.png")),
    ("farming_cotton_7.png", include_bytes!("../media/farming_cotton_7.png")),
    ("farming_cotton_8.png", include_bytes!("../media/farming_cotton_8.png")),
    ("default_grass_1.png", include_bytes!("../media/default_grass_1.png")),
    ("default_grass_2.png", include_bytes!("../media/default_grass_2.png")),
    ("default_grass_3.png", include_bytes!("../media/default_grass_3.png")),
    ("default_grass_4.png", include_bytes!("../media/default_grass_4.png")),
    ("default_grass_5.png", include_bytes!("../media/default_grass_5.png")),
    ("wool_white.png", include_bytes!("../media/wool_white.png")),
    ("default_lava.png", include_bytes!("../media/default_lava.png")),
    ("default_lava_source_animated.png", include_bytes!("../media/default_lava_source_animated.png")),
    ("default_lava_flowing_animated.png", include_bytes!("../media/default_lava_flowing_animated.png")),
    ("default_obsidian.png", include_bytes!("../media/default_obsidian.png")),
    ("bucket_lava.png", include_bytes!("../media/bucket_lava.png")),
    ("beds_bed.png", include_bytes!("../media/beds_bed.png")),
    ("beds_bed_fancy.png", include_bytes!("../media/beds_bed_fancy.png")),
    ("beds_bed_foot.png", include_bytes!("../media/beds_bed_foot.png")),
    ("beds_bed_head.png", include_bytes!("../media/beds_bed_head.png")),
    ("beds_bed_side1.png", include_bytes!("../media/beds_bed_side1.png")),
    ("beds_bed_side2.png", include_bytes!("../media/beds_bed_side2.png")),
    ("beds_bed_side_bottom.png", include_bytes!("../media/beds_bed_side_bottom.png")),
    ("beds_bed_side_bottom_r.png", include_bytes!("../media/beds_bed_side_bottom_r.png")),
    ("beds_bed_side_top.png", include_bytes!("../media/beds_bed_side_top.png")),
    ("beds_bed_side_top_r.png", include_bytes!("../media/beds_bed_side_top_r.png")),
    ("beds_bed_top1.png", include_bytes!("../media/beds_bed_top1.png")),
    ("beds_bed_top2.png", include_bytes!("../media/beds_bed_top2.png")),
    ("beds_bed_top_bottom.png", include_bytes!("../media/beds_bed_top_bottom.png")),
    ("beds_bed_top_top.png", include_bytes!("../media/beds_bed_top_top.png")),
    ("beds_bed_under.png", include_bytes!("../media/beds_bed_under.png")),
];

// ---------------------------------------------------------------------------
// Item definitions (TOCLIENT_ITEMDEF)
// ---------------------------------------------------------------------------

const ITEM_NONE: u8 = 0;
const ITEM_NODE: u8 = 1;
const ITEM_CRAFT: u8 = 2;
const ITEM_TOOL: u8 = 3;

struct Item<'a> {
    item_type: u8,
    name: &'a str,
    description: &'a str,
    inventory_image: &'a str,
    wield_image: &'a str,
    wield_scale: [f32; 3],
    place_prediction: &'a str,
    /// Serialised ToolCapabilities, or empty. The client aborts if the hand
    /// ("" item) has none.
    tool_capabilities: Vec<u8>,
    /// Played when the item is placed (a block's "place" sound).
    place_sound: Sound,
    stack_max: u16,
    groups: &'a [(&'a str, i16)],
    /// Left-click uses it (an apple is eaten) instead of digging.
    usable: bool,
    liquids_pointable: bool,
}

/// Serialised ToolCapabilities (version 5, protocol >= 38).
fn tool_capabilities(caps: &ToolCaps) -> Vec<u8> {
    let mut w = Writer::plain();
    w.u8(5)
        .f32(caps.full_punch_interval)
        .i16(caps.max_drop_level)
        .u32(caps.groupcaps.len() as u32);
    for cap in caps.groupcaps {
        w.string16(cap.group).i16(cap.uses as i16).i16(cap.maxlevel).u32(cap.times.len() as u32);
        for &(rating, time) in cap.times {
            w.i16(rating).f32(time);
        }
    }
    w.u32(1).string16("fleshy").i16(caps.fleshy); // damage groups
    w.u16(0); // punch_attack_uses
    w.into_inner()
}

/// ItemImageDef for protocol >= 51: image name, then a TileAnimation (none).
fn item_image(w: &mut Writer, name: &str) {
    w.string16(name).u8(0);
}

/// SimpleSoundSpec: name, gain, pitch, fade.
fn no_sound(w: &mut Writer) {
    sound(w, ("", 1.0));
}

fn sound(w: &mut Writer, (name, gain): (&str, f32)) {
    w.string16(name).f32(gain).f32(1.0).f32(0.0);
}

/// A sound: file name (without number and ".ogg": the client picks one of
/// "name.1.ogg", "name.2.ogg" ... at random) and volume.
type Sound = (&'static str, f32);

/// The sounds of a block, as minetest_game's default.node_sound_*_defaults
/// give them.
struct Sounds {
    footstep: Sound,
    /// While digging. "__group" makes the client play "default_dig_<group>"
    /// for the group the tool digs it by.
    dig: Sound,
    dug: Sound,
    place: Sound,
}

const GROUP_DIG: Sound = ("__group", 1.0);

fn sounds_of(id: u16) -> Sounds {
    // Stairs and slabs sound like their block.
    if let Some((_, m)) = stairs::shape_of(id) {
        return sounds_of(m.base);
    }
    let stone = Sounds {
        footstep: ("default_hard_footstep", 0.2),
        dig: GROUP_DIG,
        dug: ("default_hard_footstep", 1.0),
        place: ("default_place_node_hard", 1.0),
    };
    let dirt = |footstep: Sound| Sounds {
        footstep,
        dig: ("default_dig_crumbly", 0.4),
        dug: ("default_dirt_footstep", 1.0),
        place: ("default_place_node", 1.0),
    };
    let wood = Sounds {
        footstep: ("default_wood_footstep", 0.15),
        dig: ("default_dig_choppy", 0.4),
        dug: ("default_wood_footstep", 1.0),
        place: ("default_place_node_hard", 1.0),
    };
    match id {
        CONTENT_DIRT => dirt(("default_dirt_footstep", 0.25)),
        CONTENT_GRASS => dirt(("default_grass_footstep", 0.25)),
        CONTENT_SNOW => dirt(("default_snow_footstep", 0.2)),
        CONTENT_SAND => Sounds {
            footstep: ("default_sand_footstep", 0.05),
            dig: GROUP_DIG,
            dug: ("default_sand_footstep", 0.15),
            place: ("default_place_node", 1.0),
        },
        farming::SOIL | farming::SOIL_WET => dirt(("default_dirt_footstep", 0.25)),
        // farming's seeds: no digging sound, a soft rustle when dug.
        farming::SEED_WHEAT | farming::SEED_COTTON => Sounds {
            footstep: ("default_dirt_footstep", 0.25),
            dig: ("", 0.0),
            dug: ("default_grass_footstep", 0.2),
            place: ("default_place_node", 0.25),
        },
        farming::WOOL_WHITE => Sounds {
            footstep: ("", 1.0),
            dig: ("default_dig_dig_immediate", 0.5),
            dug: ("default_dug_node", 0.25),
            place: ("default_place_node_hard", 1.0),
        },
        id if farming::is_attached_plant(id) || id == farming::STRAW => sounds_of(CONTENT_BIRCH_LEAVES),
        CONTENT_BIRCH_LEAVES | CONTENT_FIR_NEEDLES | CONTENT_BIRCH_SAPLING | CONTENT_FIR_SAPLING | CONTENT_APPLE_LEAVES
        | CONTENT_APPLE | CONTENT_APPLE_SAPLING => Sounds {
            footstep: ("default_grass_footstep", 0.45),
            dig: GROUP_DIG,
            dug: ("default_grass_footstep", 0.7),
            place: ("default_place_node", 1.0),
        },
        CONTENT_BIRCH_TREE | CONTENT_FIR_TREE | CONTENT_BIRCH_WOOD | CONTENT_FIR_WOOD | CONTENT_APPLE_TREE | CONTENT_APPLE_WOOD => wood,
        id if is_chest(id) => wood,
        CONTENT_LADDER_WOOD => wood,
        // minetest_game's node_sound_glass_defaults and _metal_defaults.
        CONTENT_GLASS => Sounds {
            footstep: ("default_glass_footstep", 0.3),
            dig: ("default_glass_footstep", 0.5),
            dug: ("default_break_glass", 1.0),
            place: ("default_place_node_hard", 1.0),
        },
        id if doors::door_of(id).is_some_and(|(d, _)| d.item == ITEM_DOOR_WOOD) => wood,
        id if doors::trapdoor_of(id).is_some_and(|(t, _)| !t.locked) => wood,
        id if doors::trapdoor_of(id).is_some() => sounds_of(CONTENT_STEEL_BLOCK),
        id if doors::door_of(id).is_some_and(|(d, _)| d.item == ITEM_DOOR_GLASS) => sounds_of(CONTENT_GLASS),
        id if doors::door_of(id).is_some_and(|(d, _)| d.item == ITEM_DOOR_STEEL) => sounds_of(CONTENT_STEEL_BLOCK),
        // minetest_game's node_sound_water_defaults.
        // Lava: no sounds of its own in minetest_game.
        CONTENT_LAVA | CONTENT_LAVA_FLOWING => Sounds {
            footstep: ("", 1.0),
            dig: ("", 1.0),
            dug: ("", 1.0),
            place: ("default_place_node", 1.0),
        },
        CONTENT_WATER | CONTENT_WATER_FLOWING => Sounds {
            footstep: ("default_water_footstep", 0.2),
            dig: ("default_dig_dig_immediate", 0.5),
            dug: ("default_dug_node", 0.25),
            place: ("default_place_node_hard", 1.0),
        },
        CONTENT_STEEL_BLOCK | CONTENT_LADDER_STEEL => Sounds {
            footstep: ("default_metal_footstep", 0.2),
            dig: ("default_dig_metal", 0.5),
            dug: ("default_dug_metal", 0.5),
            place: ("default_place_node_metal", 0.5),
        },
        id if is_torch(id) => wood,
        id if beds::is_bed(id) => wood,
        // stone, sandstone, ores
        _ => stone,
    }
}

/// A trapdoor item's picture (minetest_game's inventory_image).
fn trapdoor_image(id: u16) -> &'static str {
    match doors::trapdoor_of(id) {
        Some((t, _)) if t.locked => TRAPDOOR_STEEL_CLOSED[0],
        _ => TRAPDOOR_WOOD_CLOSED[0],
    }
}

/// A bed item's picture (beds.lua's inventory_image).
fn bed_image(id: u16) -> &'static str {
    if id == beds::FANCY_BOTTOM { "beds_bed_fancy.png" } else { "beds_bed.png" }
}

/// The sound of placing a node, and its gain.
pub fn place_sound(id: u16) -> (&'static str, f32) {
    sounds_of(id).place
}

fn serialize_item(item: &Item) -> Vec<u8> {
    let mut w = Writer::plain();
    w.u8(6) // version
        .u8(item.item_type)
        .string16(item.name)
        .string16(item.description);
    item_image(&mut w, item.inventory_image); // empty: node items get a 3D cube
    item_image(&mut w, item.wield_image);
    for v in item.wield_scale {
        w.f32(v);
    }
    w.i16(item.stack_max as i16)
        .u8(item.usable as u8)
        .u8(item.liquids_pointable as u8)
        .bytes16(&item.tool_capabilities)
        .u16(item.groups.len() as u16);
    for (group, rating) in item.groups {
        w.string16(group).i16(*rating);
    }
    w.string16(item.place_prediction);
    sound(&mut w, item.place_sound); // sound_place
    no_sound(&mut w); // sound_place_failed
    w.f32(4.0) // range
        .string16("") // palette image
        .u32(0xFFFF_FFFF); // colour (ARGB)
    item_image(&mut w, ""); // inventory overlay
    item_image(&mut w, ""); // wield overlay
    // >= 5.4: short description
    w.string16("");
    // >= 5.7: use sounds
    no_sound(&mut w);
    no_sound(&mut w);
    // >= 5.8: optional place_param2
    w.u8(0);
    // Later optional sections are omitted; the client uses defaults.
    w.into_inner()
}

/// The serialised ItemDefManager, before compression.
pub fn itemdef_bytes() -> Vec<u8> {
    let mut items = vec![Item {
        item_type: ITEM_NONE,
        name: "", // the hand
        description: "",
        inventory_image: "",
        wield_image: "esp_hand.png",
        wield_scale: [1.0, 1.0, 2.5],
        place_prediction: "",
        tool_capabilities: tool_capabilities(&HAND),
        place_sound: ("", 1.0),
        stack_max: 99,
        groups: &[],
        usable: false,
        liquids_pointable: false,
    }];
    for n in NODES {
        items.push(Item {
            item_type: ITEM_NODE,
            name: n.name,
            description: n.description,
            // A torch or sapling is a flat picture, not a cube.
            inventory_image: match n.kind {
                Kind::Torch => TORCH_IMAGE,
                // Tall grass shows a taller tuft than the one it places.
                _ if n.id == farming::GRASS[0] => "default_grass_3.png",
                Kind::Sapling | Kind::Apple | Kind::Ladder | Kind::Plant | Kind::Seed => n.tiles[0],
                Kind::Trapdoor => trapdoor_image(n.id),
                Kind::Bed => bed_image(n.id),
                _ => "",
            },
            wield_image: match n.kind {
                Kind::Torch => TORCH_IMAGE,
                _ if n.id == farming::GRASS[0] => "default_grass_3.png",
                Kind::Sapling | Kind::Apple | Kind::Ladder | Kind::Plant | Kind::Seed => n.tiles[0],
                Kind::Trapdoor => trapdoor_image(n.id),
                Kind::Bed => bed_image(n.id),
                _ => "",
            },
            wield_scale: [1.0, 1.0, 1.0],
            // A bed is placed as two nodes, or not at all (its on_place):
            // the client doesn't guess.
            place_prediction: if n.kind == Kind::Bed { "" } else { n.name },
            tool_capabilities: Vec::new(),
            place_sound: sounds_of(n.id).place,
            stack_max: stack_max(n.id),
            groups: n.groups,
            usable: n.kind == Kind::Apple,
            liquids_pointable: false,
        });
    }
    for c in CRAFTITEMS {
        items.push(Item {
            item_type: ITEM_CRAFT,
            name: c.name,
            description: c.description,
            inventory_image: c.image,
            wield_image: c.image,
            wield_scale: [1.0, 1.0, 1.0],
            place_prediction: "",
            tool_capabilities: Vec::new(),
            place_sound: ("", 1.0),
            stack_max: stack_max(c.id),
            groups: c.groups,
            // The empty bucket is used (left-click) to scoop water; bread
            // is eaten.
            usable: matches!(c.id, ITEM_BUCKET_EMPTY | ITEM_BREAD),
            liquids_pointable: liquids_pointable(c.id),
        });
    }
    for t in TOOLS {
        items.push(Item {
            item_type: ITEM_TOOL,
            name: t.name,
            description: t.description,
            inventory_image: t.image,
            wield_image: t.wield_image,
            wield_scale: [1.0, 1.0, 1.0],
            place_prediction: "",
            tool_capabilities: tool_capabilities(&t.caps),
            place_sound: ("", 1.0),
            stack_max: stack_max(t.id),
            groups: t.groups,
            // Hoes are used (left-click) on dirt.
            usable: farming::hoe_uses(t.id).is_some(),
            liquids_pointable: false,
        });
    }

    let mut w = Writer::plain();
    w.u8(0).u16(items.len() as u16);
    for item in &items {
        w.bytes16(&serialize_item(item));
    }
    w.u16(0); // aliases
    w.into_inner()
}

// ---------------------------------------------------------------------------
// Node definitions (TOCLIENT_NODEDEF)
// ---------------------------------------------------------------------------

const CONTENTFEATURES_VERSION: u8 = 13;
const TILE_FLAG_BACKFACE_CULLING: u16 = 1 << 0;
const NDT_NORMAL: u8 = 0;
/// Leaves: see-through, or solid if the player turns off "fancy leaves".
const NDT_ALLFACES_OPTIONAL: u8 = 6;
const POINTABLE: u8 = 1;
const ALPHAMODE_OPAQUE: u8 = 2;
const ALPHAMODE_CLIP: u8 = 1;
const NDT_MESH: u8 = 16;
const NDT_PLANTLIKE: u8 = 9;
const NDT_AIRLIKE: u8 = 1;
const NDT_SIGNLIKE: u8 = 8;
const NDT_GLASSLIKE_FRAMED_OPTIONAL: u8 = 15;
const NODEBOX_FIXED: u8 = 1;
const NDT_NODEBOX: u8 = 12;
const NDT_LIQUID: u8 = 2;
const NDT_FLOWINGLIQUID: u8 = 3;
const CPT2_FLOWINGLIQUID: u8 = 2;
const CPT2_MESHOPTIONS: u8 = 7;
const ALPHAMODE_BLEND: u8 = 0;
const LIQUID_FLOWING: u8 = 1;
const LIQUID_SOURCE: u8 = 2;
/// minetest_game's water colour under the surface: ARGB.
const WATER_POST_EFFECT: u32 = 0x671E_3C5A;
/// And inside lava: {a = 191, r = 255, g = 64, b = 0}.
const LAVA_POST_EFFECT: u32 = 0xBFFF_4000;
const TILE_FLAG_HAS_ALIGN_STYLE: u16 = 1 << 5;
const ALIGN_STYLE_WORLD: u8 = 1;
const TILE_FLAG_TILEABLE_HORIZONTAL: u16 = 1 << 1;
const TILE_FLAG_TILEABLE_VERTICAL: u16 = 1 << 2;
const TAT_VERTICAL_FRAMES: u8 = 1;
const CPT_LIGHT: u8 = 1;
const CPT2_FACEDIR: u8 = 3;
const CPT2_WALLMOUNTED: u8 = 4;
const NODEBOX_WALLMOUNTED: u8 = 2;
const NODEBOX_REGULAR: u8 = 0;
const LEVELED_MAX: u8 = 0x7F;

/// TileDef version 6: name, animation (none), flags.
fn tile(w: &mut Writer, name: &str, flags: u16) {
    w.u8(6).string16(name).u8(0).u16(flags);
}

/// How long a cycle of an animated texture takes (square frames stacked
/// vertically), or None for a still one.
fn animation_length(tile: &str) -> Option<f32> {
    match tile {
        TORCH_TILE => Some(TORCH_FRAMES_LENGTH),
        FURNACE_ACTIVE_TILE => Some(1.5),
        WATER_SOURCE_TILE => Some(2.0),
        WATER_FLOWING_TILE => Some(0.5),
        LAVA_SOURCE_TILE => Some(3.0),
        LAVA_FLOWING_TILE => Some(3.3),
        _ => None,
    }
}

/// A TileDef with an animation of square frames stacked vertically, the
/// whole cycle taking `length` seconds.
fn animated_tile(w: &mut Writer, name: &str, flags: u16, length: f32) {
    w.u8(6).string16(name);
    w.u8(TAT_VERTICAL_FRAMES).u16(16).u16(16).f32(length);
    w.u16(flags);
}

/// A selection box for wall-mounted nodes: (top, bottom, side) boxes in
/// BS units (10 per node), the side one against the -X wall. These are
/// minetest_game's torch boxes (each of its three torch nodes sets the one
/// it uses).
fn torch_selection_box(w: &mut Writer) {
    w.u8(6).u8(NODEBOX_WALLMOUNTED);
    let boxes: [[f32; 6]; 3] = [
        [-1.0 / 8.0, -1.0 / 16.0, -5.0 / 16.0, 1.0 / 8.0, 0.5, 1.0 / 8.0], // ceiling
        [-1.0 / 8.0, -0.5, -1.0 / 8.0, 1.0 / 8.0, 2.0 / 16.0, 1.0 / 8.0], // floor
        [-0.5, -0.5, -1.0 / 8.0, -1.0 / 8.0, 1.0 / 8.0, 1.0 / 8.0],     // wall
    ];
    for b in boxes {
        for v in b {
            w.f32(v * 10.0);
        }
    }
}

/// minetest_game's aspen sapling box: a thin upright box, in BS units.
fn sapling_selection_box(w: &mut Writer) {
    w.u8(6).u8(NODEBOX_FIXED).u16(1);
    for v in [-3.0 / 16.0, -0.5, -3.0 / 16.0, 3.0 / 16.0, 0.5, 3.0 / 16.0] {
        w.f32(v * 10.0);
    }
}

/// minetest_game's open chest box (the body, without the lid), in BS units.
fn open_chest_selection_box(w: &mut Writer) {
    w.u8(6).u8(NODEBOX_FIXED).u16(1);
    for v in [-0.5, -0.5, -0.5, 0.5, 3.0 / 16.0, 0.5] {
        w.f32(v * 10.0);
    }
}

/// A ladder's box: Luanti's default wallmounted boxes, a sixteenth of a
/// node thick against the wall (top, bottom, side), in BS units.
fn ladder_selection_box(w: &mut Writer) {
    w.u8(6).u8(NODEBOX_WALLMOUNTED);
    let t = 1.0 / 16.0;
    let boxes: [[f32; 6]; 3] = [
        [-0.5, 0.5 - t, -0.5, 0.5, 0.5, 0.5],
        [-0.5, -0.5, -0.5, 0.5, -0.5 + t, 0.5],
        [-0.5, -0.5, -0.5, -0.5 + t, 0.5, 0.5],
    ];
    for b in boxes {
        for v in b {
            w.f32(v * 10.0);
        }
    }
}

/// minetest_game's open chest model.
const CHEST_OPEN_MESH: &str = "chest_open.obj";

/// The selection boxes of farming's nodes (nodes.lua, api.lua): low
/// tufts of grass, taller wild cotton, and crops and seeds flat on the
/// ground.
fn farming_box(id: u16) -> [f32; 6] {
    if farming::is_grass(id) {
        [-6.0 / 16.0, -0.5, -6.0 / 16.0, 6.0 / 16.0, -5.0 / 16.0, 6.0 / 16.0]
    } else if id == farming::COTTON_WILD {
        [-6.0 / 16.0, -0.5, -6.0 / 16.0, 6.0 / 16.0, 5.0 / 16.0, 6.0 / 16.0]
    } else {
        [-0.5, -0.5, -0.5, 0.5, -5.0 / 16.0, 0.5]
    }
}

/// A door's selection and collision box: two nodes tall, a thin slab at the
/// front, turned by facedir.
const DOOR_BOX: [f32; 6] = [-0.5, -0.5, -0.5, 0.5, 1.5, -6.0 / 16.0];

/// minetest_game's apple box, in BS units.
fn apple_selection_box(w: &mut Writer) {
    w.u8(6).u8(NODEBOX_FIXED).u16(1);
    for v in [-3.0 / 16.0, -7.0 / 16.0, -3.0 / 16.0, 3.0 / 16.0, 4.0 / 16.0, 3.0 / 16.0] {
        w.f32(v * 10.0);
    }
}

fn regular_nodebox(w: &mut Writer) {
    w.u8(6).u8(NODEBOX_REGULAR);
}

/// A fixed node box made of `boxes` (in nodes), in BS units.
fn fixed_nodebox(w: &mut Writer, boxes: &[[f32; 6]]) {
    w.u8(6).u8(NODEBOX_FIXED).u16(boxes.len() as u16);
    for b in boxes {
        for v in b {
            w.f32(v * 10.0);
        }
    }
}

fn serialize_node(n: &Node) -> Vec<u8> {
    let mut w = Writer::plain();
    w.u8(CONTENTFEATURES_VERSION).string16(n.name);
    w.u16(n.groups.len() as u16);
    for (group, level) in n.groups {
        w.string16(group).i16(*level);
    }
    let torch = n.kind == Kind::Torch;
    let leaves = n.kind == Kind::Leaves;
    // Saplings and apples: plants, drawn as two crossed pictures.
    let sapling = matches!(n.kind, Kind::Sapling | Kind::Apple);
    let mark = n.kind == Kind::Mark;
    let chest = matches!(n.kind, Kind::Chest | Kind::ChestOpen);
    let glass = n.kind == Kind::Glass;
    let ladder = n.kind == Kind::Ladder;
    let shaped = stairs::shape_of(n.id);
    let door = doors::door_of(n.id).map(|(d, _)| d);
    let hidden = n.kind == Kind::DoorHidden;
    let trapdoor = doors::trapdoor_of(n.id);
    let liquid = n.kind == Kind::Liquid;
    let flowing = matches!(n.id, CONTENT_WATER_FLOWING | CONTENT_LAVA_FLOWING);
    let lava = is_lava(n.id);
    let plant = n.kind == Kind::Plant;
    let seed = n.kind == Kind::Seed;
    // Wheat grows as a "#" of pictures (meshoptions; param2 says which shape).
    let wheat = farming::WHEAT_STAGES.contains(&n.id);
    let bed = beds::is_bed(n.id);
    if torch {
        w.u8(CPT_LIGHT).u8(CPT2_WALLMOUNTED);
    } else if leaves {
        w.u8(CPT_LIGHT).u8(0);
    } else if n.kind == Kind::Furnace {
        w.u8(0).u8(CPT2_FACEDIR);
    } else if ladder {
        w.u8(CPT_LIGHT).u8(CPT2_WALLMOUNTED);
    } else if glass {
        w.u8(CPT_LIGHT).u8(0);
    } else if matches!(n.kind, Kind::Chest | Kind::ChestOpen) {
        // paramtype "light", as minetest_game's chests.
        w.u8(CPT_LIGHT).u8(CPT2_FACEDIR);
    } else if sapling || mark {
        w.u8(CPT_LIGHT).u8(0);
    } else if liquid {
        w.u8(CPT_LIGHT).u8(if flowing { CPT2_FLOWINGLIQUID } else { 0 });
    } else if plant {
        w.u8(CPT_LIGHT).u8(if wheat { CPT2_MESHOPTIONS } else { 0 });
    } else if seed {
        w.u8(CPT_LIGHT).u8(CPT2_WALLMOUNTED);
    } else if shaped.is_some() || door.is_some() || hidden || trapdoor.is_some() || bed {
        w.u8(CPT_LIGHT).u8(CPT2_FACEDIR);
    } else {
        w.u8(0) // param_type: none
            .u8(0); // param_type_2: none
    }

    // visual
    let (drawtype, mesh) = match n.kind {
        Kind::Torch => (NDT_MESH, torch_mesh(n.id)),
        Kind::Leaves => (NDT_ALLFACES_OPTIONAL, ""),
        Kind::Solid | Kind::Furnace | Kind::Chest => (NDT_NORMAL, ""),
        Kind::ChestOpen => (NDT_MESH, CHEST_OPEN_MESH),
        Kind::Glass => (NDT_GLASSLIKE_FRAMED_OPTIONAL, ""),
        Kind::Ladder => (NDT_SIGNLIKE, ""),
        Kind::Sapling | Kind::Apple => (NDT_PLANTLIKE, ""),
        Kind::Mark => (NDT_AIRLIKE, ""),
        Kind::Shaped => (NDT_NODEBOX, ""),
        Kind::Door => (NDT_MESH, doors::mesh_of(n.id)),
        Kind::DoorHidden => (NDT_AIRLIKE, ""),
        Kind::Trapdoor => (NDT_NODEBOX, ""),
        Kind::Plant => (NDT_PLANTLIKE, ""),
        Kind::Seed => (NDT_SIGNLIKE, ""),
        Kind::Bed | Kind::BedTop => (NDT_NODEBOX, ""),
        Kind::Liquid if flowing => (NDT_FLOWINGLIQUID, ""),
        Kind::Liquid => (NDT_LIQUID, ""),
    };
    w.u8(drawtype).string16(mesh).f32(1.0); // drawtype, mesh, visual_scale
    // Mesh tiles: no backface culling (the flame is single planes), as
    // the Lua API defaults them.
    // Plant pictures are seen from both sides (Luanti turns culling off
    // for plantlike by default, as for meshes).
    let flags = if torch {
        TILE_FLAG_TILEABLE_HORIZONTAL | TILE_FLAG_TILEABLE_VERTICAL
    } else if let Some(d) = door {
        // Wooden and steel doors cull their back faces, as minetest_game
        // asks; the glass door, left to the mesh default, doesn't.
        let culling = if d.item == ITEM_DOOR_GLASS { 0 } else { TILE_FLAG_BACKFACE_CULLING };
        TILE_FLAG_TILEABLE_HORIZONTAL | TILE_FLAG_TILEABLE_VERTICAL | culling
    } else if sapling || ladder || plant || seed {
        0
    } else {
        TILE_FLAG_BACKFACE_CULLING
    };
    w.u8(6);
    for (i, t) in n.tiles.iter().copied().enumerate() {
        // minetest_game's water: the first tile drawn from both sides, the
        // rest from the front.
        let flags = if liquid { if i == 0 { 0 } else { TILE_FLAG_BACKFACE_CULLING } } else { flags };
        match animation_length(t) {
            Some(length) => animated_tile(&mut w, t, flags, length),
            // Stairs of stone and the like continue their block's texture
            // across the world (minetest_game's worldaligntex).
            None if shaped.is_some_and(|(_, m)| m.world_aligned) => {
                w.u8(6).string16(t).u8(0).u16(flags | TILE_FLAG_HAS_ALIGN_STYLE).u8(ALIGN_STYLE_WORLD);
            }
            None => tile(&mut w, t, flags),
        }
    }
    for _ in 0..6 {
        tile(&mut w, "", flags); // overlay tiles
    }
    w.u8(6);
    for i in 0..6 {
        // Flowing water's two surfaces (top, then sides), which the client
        // draws flowing liquids with.
        match i {
            0 if flowing && lava => animated_tile(&mut w, LAVA_FLOWING_TILE, 0, 3.3),
            1 if flowing && lava => animated_tile(&mut w, LAVA_FLOWING_TILE, TILE_FLAG_BACKFACE_CULLING, 3.3),
            0 if flowing => animated_tile(&mut w, WATER_FLOWING_TILE, 0, 0.5),
            1 if flowing => animated_tile(&mut w, WATER_FLOWING_TILE, TILE_FLAG_BACKFACE_CULLING, 0.5),
            _ => tile(&mut w, "", flags), // special tiles
        }
    }
    w.u8(255) // legacy alpha: opaque
        .u8(255)
        .u8(255)
        .u8(255) // colour
        .string16("") // palette
        .u8(if liquid && !lava { 3 } else if plant { 1 } else { 0 }) // waving: water ripples, plants sway
        .u8(0) // connect_sides
        .u16(0) // connects_to
        .u32(if lava { LAVA_POST_EFFECT } else if liquid { WATER_POST_EFFECT } else { 0 }) // post_effect_color (ARGB)
        .u8(0); // leveled

    // lighting
    let doorway = door.is_some() || hidden;
    w.u8((torch || leaves || sapling || mark || chest || glass || ladder || shaped.is_some() || doorway || trapdoor.is_some() || liquid || plant || seed || bed) as u8) // light_propagates
        .u8((torch || sapling || mark || glass || ladder || doorway || plant || seed) as u8) // sunlight_propagates
        .u8(glow(n.id));

    let tree = n.groups.iter().any(|(g, _)| *g == "tree");
    w.u8((n.kind == Kind::Solid && !tree) as u8); // is_ground_content

    // interaction
    // The hidden door node is walkable, as minetest_game's (falling nodes
    // stop on it), though its collision box is tiny.
    w.u8((walkable(n.id) || hidden) as u8) // walkable
        .u8(if mark || hidden || liquid { 0 } else { POINTABLE })
        .u8(!(mark || hidden || liquid) as u8) // diggable
        .u8(ladder as u8) // climbable
        .u8((mark || liquid || plant) as u8) // buildable_to
        .u8(matches!(n.kind, Kind::Furnace | Kind::Chest | Kind::ChestOpen | Kind::Door | Kind::Trapdoor | Kind::Bed) as u8) // rightclickable
        .u32(damage_per_second(n.id) as u32); // damage_per_second

    // liquid
    if liquid {
        let (flowing_name, source_name, viscosity) =
            if lava { ("esp:lava_flowing", "esp:lava_source", 7) } else { ("esp:water_flowing", "esp:water_source", 1) };
        w.u8(if flowing { LIQUID_FLOWING } else { LIQUID_SOURCE })
            .string16(flowing_name)
            .string16(source_name)
            .u8(viscosity) // viscosity
            .u8(!lava as u8) // renewable
            .u8(8) // range
            .u8(1) // drowning
            .u8(0); // floodable
    } else {
        w.u8(0) // liquid_type: none
            .string16("")
            .string16("")
            .u8(0) // viscosity
            .u8(1) // renewable
            .u8(8) // range
            .u8(0) // drowning
            .u8(0); // floodable
    }

    match (shaped, trapdoor) {
        (Some((shape, _)), _) => fixed_nodebox(&mut w, shape.boxes()), // node_box
        (_, Some((_, open))) => fixed_nodebox(&mut w, &[doors::trapdoor_box(open)]),
        _ if bed => fixed_nodebox(&mut w, beds::boxes(n.id)),
        _ => regular_nodebox(&mut w), // node_box (unused by meshes)
    }
    if let Some((_, open)) = trapdoor {
        // Its selection box is its node box; so, left empty, is its
        // collision box.
        fixed_nodebox(&mut w, &[doors::trapdoor_box(open)]);
    } else if let Some((shape, _)) = shaped {
        // builtin's register_node: a nodebox's selection box is its node
        // box. Its collision box is left empty, so the client uses the
        // node box for that too.
        fixed_nodebox(&mut w, shape.boxes());
    } else if torch {
        torch_selection_box(&mut w);
    } else if n.kind == Kind::Apple {
        apple_selection_box(&mut w);
    } else if n.kind == Kind::ChestOpen {
        open_chest_selection_box(&mut w);
    } else if ladder {
        ladder_selection_box(&mut w);
    } else if sapling {
        sapling_selection_box(&mut w);
    } else if door.is_some() {
        fixed_nodebox(&mut w, &[DOOR_BOX]);
    } else if plant || seed {
        fixed_nodebox(&mut w, &[farming_box(n.id)]);
    } else if bed {
        fixed_nodebox(&mut w, &[beds::selection_box(n.id)]);
    } else {
        regular_nodebox(&mut w); // selection_box
    }
    if door.is_some() {
        fixed_nodebox(&mut w, &[DOOR_BOX]);
    } else if hidden {
        // 1px block inside the door's hinge, near the top.
        fixed_nodebox(&mut w, &[[-15.0 / 32.0, 13.0 / 32.0, -15.0 / 32.0, -13.0 / 32.0, 0.5, -13.0 / 32.0]]);
    } else if bed {
        // Left empty in beds.lua, so the node box.
        fixed_nodebox(&mut w, beds::boxes(n.id));
    } else {
        regular_nodebox(&mut w); // collision_box (torches aren't walkable)
    }

    let sounds = sounds_of(n.id);
    sound(&mut w, sounds.footstep);
    sound(&mut w, sounds.dig);
    sound(&mut w, sounds.dug);

    w.u8(0) // legacy_facedir_simple
        .u8(0); // legacy_wallmounted

    // A steel door's owner alone can dig it, so the client doesn't guess
    // that it went (minetest_game's node_dig_prediction = "").
    w.string16(if doors::is_locked(n.id) { "" } else { "air" }) // node_dig_prediction
        .u8(LEVELED_MAX)
        .u8(if lava {
            ALPHAMODE_OPAQUE
        } else if liquid {
            ALPHAMODE_BLEND
        } else if torch || leaves || sapling || mark || glass || ladder || doorway || trapdoor.is_some() || plant || seed || bed {
            ALPHAMODE_CLIP
        } else {
            ALPHAMODE_OPAQUE
        })
        // Swimming: water slows you as its viscosity says.
        .u8(if lava { 7 } else { liquid as u8 }) // move_resistance: the viscosity
        .u8(liquid as u8) // liquid_move_physics
        .u8(0); // post_effect_color_shaded
    w.into_inner()
}

/// The serialised NodeDefManager, before compression.
pub fn nodedef_bytes() -> Vec<u8> {
    let mut list = Writer::plain();
    for n in NODES {
        list.u16(n.id).bytes16(&serialize_node(n));
    }
    let mut w = Writer::plain();
    w.u8(1).u16(NODES.len() as u16).bytes32(&list.into_inner());
    w.into_inner()
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

pub struct MediaFile {
    pub name: String,
    pub sha1: [u8; 20],
    pub data: Vec<u8>,
}

pub struct Media {
    files: Vec<MediaFile>,
}

impl Media {
    /// Built-in textures, plus (and overridden by) any files in `dir`.
    pub fn load(dir: &str) -> Self {
        let mut files: Vec<MediaFile> = EMBEDDED_MEDIA
            .iter()
            .map(|(name, data)| MediaFile {
                name: name.to_string(),
                sha1: sha1(data),
                data: data.to_vec(),
            })
            .collect();

        match fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let data = match fs::read(entry.path()) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!("Reading media {name}: {e}");
                            continue;
                        }
                    };
                    let file = MediaFile {
                        sha1: sha1(&data),
                        name: name.clone(),
                        data,
                    };
                    match files.iter_mut().find(|f| f.name.eq_ignore_ascii_case(&name)) {
                        Some(existing) => {
                            info!("Media {name} from SD card replaces built-in version");
                            *existing = file;
                        }
                        None => files.push(file),
                    }
                }
            }
            Err(_) => info!("No media folder at {dir}, using built-in textures only"),
        }

        let total: usize = files.iter().map(|f| f.data.len()).sum();
        info!("{} media files, {total} bytes", files.len());
        Self { files }
    }

    pub fn files(&self) -> &[MediaFile] {
        &self.files
    }

    pub fn get(&self, name: &str) -> Option<&MediaFile> {
        self.files.iter().find(|f| f.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nodes are listed in id order without gaps, so `def` finds a node by
    /// indexing (ids 125 to 127 are Luanti's own, so the list must stop
    /// short of them, or skip them).
    #[test]
    fn node_ids_in_order() {
        for (i, n) in NODES.iter().enumerate() {
            assert_eq!(node_index(n.id), i, "{} is out of place", n.name);
            assert!(!(125..=127).contains(&n.id), "{} has a reserved id", n.name);
        }
        assert!(is_liquid(CONTENT_WATER) && floodable(CONTENT_TORCH) && passes_light(CONTENT_WATER) && !passes_sunlight(CONTENT_WATER));
        assert!(buildable_to(CONTENT_WATER) && buildable_to(crate::world::CONTENT_AIR) && !buildable_to(CONTENT_STONE));
        assert!(!walkable(CONTENT_WATER));
        assert_eq!(stack_max(ITEM_BUCKET_WATER), 1);
        assert_eq!(stack_max(ITEM_BUCKET_EMPTY), 99);
    }
}
