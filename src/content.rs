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
        drop: CONTENT_STONE,
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
        groups: &[("crumbly", 2), ("cracky", 3)],
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
    // Ores drop themselves until there are items to drop instead. Cracky 3
    // (rather than minetest_game's 2) so the bare hand can dig them.
    Node {
        id: CONTENT_COAL,
        name: "esp:stone_with_coal",
        description: "Coal Ore",
        tiles: ["esp_stone_with_coal.png"; 6],
        groups: &[("cracky", 3)],
        // Stands in for coal lump + stick until there is crafting.
        drop: CONTENT_TORCH,
        drop_count: 4,
        kind: Kind::Solid,
    },
    Node {
        id: CONTENT_IRON,
        name: "esp:stone_with_iron",
        description: "Iron Ore",
        tiles: ["esp_stone_with_iron.png"; 6],
        groups: &[("cracky", 3)],
        drop: CONTENT_IRON,
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
];

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
        _ => 0,
    }
}

/// Whether light passes through a node (fading by one level per node).
pub fn passes_light(id: u16) -> bool {
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| matches!(n.kind, Kind::Torch | Kind::Leaves))
}

/// Whether direct sunlight carries on down through a node at full
/// strength. Leaves let light through but cast shade.
pub fn passes_sunlight(id: u16) -> bool {
    id == crate::world::CONTENT_AIR || def(id).is_some_and(|n| n.kind == Kind::Torch)
}

/// Whether players stand on a node (and so whether it can hold a torch).
pub fn walkable(id: u16) -> bool {
    def(id).is_some_and(|n| matches!(n.kind, Kind::Solid | Kind::Leaves))
}

pub fn is_torch(id: u16) -> bool {
    def(id).is_some_and(|n| n.kind == Kind::Torch)
}

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
pub fn id_of(name: &str) -> Option<u16> {
    if name == "air" {
        return Some(crate::world::CONTENT_AIR);
    }
    NODES.iter().find(|n| n.name == name).map(|n| n.id)
}

pub fn name_of(id: u16) -> &'static str {
    NODES.iter().find(|n| n.id == id).map_or("air", |n| n.name)
}

pub fn description_of(id: u16) -> &'static str {
    def(id).map_or("", |n| n.description)
}

/// Whether an id is an item a player can hold (any of our nodes).
pub fn is_item(id: u16) -> bool {
    NODES.iter().any(|n| n.id == id)
}

/// The item digging a node gives, if any, and how many.
pub fn drop_of(id: u16) -> Option<(u16, u16)> {
    def(id).map(|n| (n.drop, n.drop_count))
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
];

// ---------------------------------------------------------------------------
// Item definitions (TOCLIENT_ITEMDEF)
// ---------------------------------------------------------------------------

const ITEM_NONE: u8 = 0;
const ITEM_NODE: u8 = 1;

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
}

/// The hand's digging abilities, modelled on minetest_game's hand: dirt
/// quickly, stone slowly, 1 damage when punching.
fn hand_tool_capabilities() -> Vec<u8> {
    let mut w = Writer::plain();
    w.u8(5) // version (protocol >= 38)
        .f32(0.9) // full_punch_interval
        .i16(0) // max_drop_level
        .u32(4); // group capabilities:
    w.string16("crumbly").i16(0).i16(1).u32(2); // uses, maxlevel, times
    w.i16(2).f32(3.0).i16(3).f32(0.7);
    w.string16("cracky").i16(0).i16(1).u32(1);
    w.i16(3).f32(4.0);
    // minetest_game's hand: leaves quickly, tree trunks slowly.
    w.string16("snappy").i16(0).i16(1).u32(1);
    w.i16(3).f32(0.4);
    w.string16("oddly_breakable_by_hand").i16(0).i16(1).u32(3);
    w.i16(1).f32(3.5).i16(2).f32(2.0).i16(3).f32(0.7);
    w.u32(1).string16("fleshy").i16(1); // damage groups
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
        CONTENT_BIRCH_LEAVES | CONTENT_FIR_NEEDLES => Sounds {
            footstep: ("default_grass_footstep", 0.45),
            dig: GROUP_DIG,
            dug: ("default_grass_footstep", 0.7),
            place: ("default_place_node", 1.0),
        },
        CONTENT_BIRCH_TREE | CONTENT_FIR_TREE => wood,
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
    w.i16(99) // stack_max
        .u8(0) // usable
        .u8(0) // liquids_pointable
        .bytes16(&item.tool_capabilities)
        .u16(0) // groups
        .string16(item.place_prediction);
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
        tool_capabilities: hand_tool_capabilities(),
        place_sound: ("", 1.0),
    }];
    for n in NODES {
        items.push(Item {
            item_type: ITEM_NODE,
            name: n.name,
            description: n.description,
            // A torch is a flat picture, not a cube.
            inventory_image: if n.kind == Kind::Torch { TORCH_IMAGE } else { "" },
            wield_image: if n.kind == Kind::Torch { TORCH_IMAGE } else { "" },
            wield_scale: [1.0, 1.0, 1.0],
            place_prediction: n.name,
            tool_capabilities: Vec::new(),
            place_sound: sounds_of(n.id).place,
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
const TILE_FLAG_TILEABLE_HORIZONTAL: u16 = 1 << 1;
const TILE_FLAG_TILEABLE_VERTICAL: u16 = 1 << 2;
const TAT_VERTICAL_FRAMES: u8 = 1;
const CPT_LIGHT: u8 = 1;
const CPT2_WALLMOUNTED: u8 = 4;
const NODEBOX_WALLMOUNTED: u8 = 2;
const NODEBOX_REGULAR: u8 = 0;
const LEVELED_MAX: u8 = 0x7F;

/// TileDef version 6: name, animation (none), flags.
fn tile(w: &mut Writer, name: &str, flags: u16) {
    w.u8(6).string16(name).u8(0).u16(flags);
}

/// A TileDef with minetest_game's torch flame animation: square frames
/// stacked vertically, the whole cycle taking TORCH_FRAMES_LENGTH seconds.
fn animated_tile(w: &mut Writer, name: &str, flags: u16) {
    w.u8(6).string16(name);
    w.u8(TAT_VERTICAL_FRAMES).u16(16).u16(16).f32(TORCH_FRAMES_LENGTH);
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
    if torch {
        w.u8(CPT_LIGHT).u8(CPT2_WALLMOUNTED);
    } else if leaves {
        w.u8(CPT_LIGHT).u8(0);
    } else {
        w.u8(0) // param_type: none
            .u8(0); // param_type_2: none
    }

    // visual
    let (drawtype, mesh) = match n.kind {
        Kind::Torch => (NDT_MESH, torch_mesh(n.id)),
        Kind::Leaves => (NDT_ALLFACES_OPTIONAL, ""),
        Kind::Solid => (NDT_NORMAL, ""),
    };
    w.u8(drawtype).string16(mesh).f32(1.0); // drawtype, mesh, visual_scale
    // Mesh tiles: no backface culling (the flame is single planes), as
    // the Lua API defaults them.
    let flags = if torch {
        TILE_FLAG_TILEABLE_HORIZONTAL | TILE_FLAG_TILEABLE_VERTICAL
    } else {
        TILE_FLAG_BACKFACE_CULLING
    };
    w.u8(6);
    for t in n.tiles {
        if torch {
            animated_tile(&mut w, t, flags);
        } else {
            tile(&mut w, t, flags);
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
    w.u8((torch || leaves) as u8) // light_propagates
        .u8(torch as u8) // sunlight_propagates
        .u8(light_source(n.id));

    let tree = n.groups.iter().any(|(g, _)| *g == "tree");
    w.u8((n.kind == Kind::Solid && !tree) as u8); // is_ground_content

    // interaction
    w.u8(walkable(n.id) as u8) // walkable
        .u8(POINTABLE)
        .u8(1) // diggable
        .u8(0) // climbable
        .u8(0) // buildable_to
        .u8(0) // rightclickable
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
        .u8(if torch || leaves { ALPHAMODE_CLIP } else { ALPHAMODE_OPAQUE })
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
