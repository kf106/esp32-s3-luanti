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
/// Light given off by a torch (minetest_game: LIGHT_MAX - 1 = 13; 12 is
/// slightly gentler and keeps the lit area, and the work, smaller).
pub const TORCH_LIGHT: u8 = 12;

/// Content IDs 125, 126 and 127 are reserved for unknown, air and ignore.
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
];

// Saplings, from leaves (one time in SAPLING_RARITY), as minetest_game's
// aspen and pine saplings, with their pictures (CC BY-SA 3.0, see
// media/LICENSE-items.txt).
const SAPLING_GROUPS: &[(&str, i16)] =
    &[("snappy", 2), ("dig_immediate", 3), ("flammable", 3), ("attached_node", 1), ("sapling", 1)];

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
fn use_wear(uses: u32, wear: u16) -> u32 {
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
    NODES.iter().find(|n| n.id == id)
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
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| matches!(n.kind, Kind::Torch | Kind::Leaves | Kind::Sapling | Kind::Apple | Kind::Mark))
}

/// Whether direct sunlight carries on down through a node at full
/// strength. Leaves let light through but cast shade.
pub fn passes_sunlight(id: u16) -> bool {
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| matches!(n.kind, Kind::Torch | Kind::Sapling | Kind::Apple | Kind::Mark))
}

/// Whether players stand on a node (and so whether it can hold a torch).
pub fn walkable(id: u16) -> bool {
    def(id).is_some_and(|n| matches!(n.kind, Kind::Solid | Kind::Leaves | Kind::Furnace))
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
    if is_tool(id) { 1 } else { 99 }
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
        CONTENT_BIRCH_LEAVES | CONTENT_FIR_NEEDLES | CONTENT_BIRCH_SAPLING | CONTENT_FIR_SAPLING | CONTENT_APPLE_LEAVES
        | CONTENT_APPLE | CONTENT_APPLE_SAPLING => Sounds {
            footstep: ("default_grass_footstep", 0.45),
            dig: GROUP_DIG,
            dug: ("default_grass_footstep", 0.7),
            place: ("default_place_node", 1.0),
        },
        CONTENT_BIRCH_TREE | CONTENT_FIR_TREE | CONTENT_BIRCH_WOOD | CONTENT_FIR_WOOD | CONTENT_APPLE_TREE | CONTENT_APPLE_WOOD => wood,
        id if is_torch(id) => wood,
        // stone, sandstone, ores
        _ => stone,
    }
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
        .u8(0) // liquids_pointable
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
    }];
    for n in NODES {
        items.push(Item {
            item_type: ITEM_NODE,
            name: n.name,
            description: n.description,
            // A torch or sapling is a flat picture, not a cube.
            inventory_image: match n.kind {
                Kind::Torch => TORCH_IMAGE,
                Kind::Sapling | Kind::Apple => n.tiles[0],
                _ => "",
            },
            wield_image: match n.kind {
                Kind::Torch => TORCH_IMAGE,
                Kind::Sapling | Kind::Apple => n.tiles[0],
                _ => "",
            },
            wield_scale: [1.0, 1.0, 1.0],
            place_prediction: n.name,
            tool_capabilities: Vec::new(),
            place_sound: sounds_of(n.id).place,
            stack_max: stack_max(n.id),
            groups: n.groups,
            usable: n.kind == Kind::Apple,
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
            usable: false,
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
            usable: false,
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
const NODEBOX_FIXED: u8 = 1;
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
    if torch {
        w.u8(CPT_LIGHT).u8(CPT2_WALLMOUNTED);
    } else if leaves {
        w.u8(CPT_LIGHT).u8(0);
    } else if n.kind == Kind::Furnace {
        w.u8(0).u8(CPT2_FACEDIR);
    } else if sapling || mark {
        w.u8(CPT_LIGHT).u8(0);
    } else {
        w.u8(0) // param_type: none
            .u8(0); // param_type_2: none
    }

    // visual
    let (drawtype, mesh) = match n.kind {
        Kind::Torch => (NDT_MESH, torch_mesh(n.id)),
        Kind::Leaves => (NDT_ALLFACES_OPTIONAL, ""),
        Kind::Solid | Kind::Furnace => (NDT_NORMAL, ""),
        Kind::Sapling | Kind::Apple => (NDT_PLANTLIKE, ""),
        Kind::Mark => (NDT_AIRLIKE, ""),
    };
    w.u8(drawtype).string16(mesh).f32(1.0); // drawtype, mesh, visual_scale
    // Mesh tiles: no backface culling (the flame is single planes), as
    // the Lua API defaults them.
    // Plant pictures are seen from both sides (Luanti turns culling off
    // for plantlike by default, as for meshes).
    let flags = if torch {
        TILE_FLAG_TILEABLE_HORIZONTAL | TILE_FLAG_TILEABLE_VERTICAL
    } else if sapling {
        0
    } else {
        TILE_FLAG_BACKFACE_CULLING
    };
    w.u8(6);
    for t in n.tiles {
        match animation_length(t) {
            Some(length) => animated_tile(&mut w, t, flags, length),
            None => tile(&mut w, t, flags),
        }
    }
    for _ in 0..6 {
        tile(&mut w, "", flags); // overlay tiles
    }
    w.u8(6);
    for _ in 0..6 {
        tile(&mut w, "", flags); // special tiles
    }
    w.u8(255) // legacy alpha: opaque
        .u8(255)
        .u8(255)
        .u8(255) // colour
        .string16("") // palette
        .u8(0) // waving
        .u8(0) // connect_sides
        .u16(0) // connects_to
        .u32(0) // post_effect_color (ARGB)
        .u8(0); // leveled

    // lighting
    w.u8((torch || leaves || sapling || mark) as u8) // light_propagates
        .u8((torch || sapling || mark) as u8) // sunlight_propagates
        .u8(light_source(n.id));

    let tree = n.groups.iter().any(|(g, _)| *g == "tree");
    w.u8((n.kind == Kind::Solid && !tree) as u8); // is_ground_content

    // interaction
    w.u8(walkable(n.id) as u8) // walkable
        .u8(if mark { 0 } else { POINTABLE })
        .u8(!mark as u8) // diggable
        .u8(0) // climbable
        .u8(mark as u8) // buildable_to
        .u8((n.kind == Kind::Furnace) as u8) // rightclickable
        .u32(0); // damage_per_second

    // liquid
    w.u8(0) // liquid_type: none
        .string16("")
        .string16("")
        .u8(0) // viscosity
        .u8(1) // renewable
        .u8(8) // range
        .u8(0) // drowning
        .u8(0); // floodable

    regular_nodebox(&mut w); // node_box (unused by meshes)
    if torch {
        torch_selection_box(&mut w);
    } else if n.kind == Kind::Apple {
        apple_selection_box(&mut w);
    } else if sapling {
        sapling_selection_box(&mut w);
    } else {
        regular_nodebox(&mut w); // selection_box
    }
    regular_nodebox(&mut w); // collision_box (torches aren't walkable)

    let sounds = sounds_of(n.id);
    sound(&mut w, sounds.footstep);
    sound(&mut w, sounds.dig);
    sound(&mut w, sounds.dug);

    w.u8(0) // legacy_facedir_simple
        .u8(0); // legacy_wallmounted

    w.string16("air") // node_dig_prediction
        .u8(LEVELED_MAX)
        .u8(if torch || leaves || sapling || mark { ALPHAMODE_CLIP } else { ALPHAMODE_OPAQUE })
        .u8(0) // move_resistance
        .u8(0) // liquid_move_physics
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
