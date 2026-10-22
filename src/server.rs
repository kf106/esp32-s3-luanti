//! The server loop: peers, the reliability layer, and command dispatch.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use log::{debug, info, warn};

use crate::auth::{self, Account, AuthStore, NameError};
use crate::content::{self, Media};
use crate::messages::{self, *};
use crate::protocol::*;
use crate::inventory::{self, Action, Inventory, InventoryStore, List, Stack};
use crate::items::{Change, Items};
use crate::player::{PlayerData, PlayerStore};
use crate::skins::{self, Skins};
use crate::srp;
use crate::doors::{self, Door};
use crate::stairs;
use crate::status::{self, SharedStatus};
use crate::chest::{self, Chests};
use crate::clock::{self, Clock};
use crate::furnace::{self, FurnaceList, Furnaces, Lists, Place};
use crate::nature::{self, Job, Rng, Timers};
use crate::world::{self, BlockPos, NodePos, World};

/// The resend timeout before any round trip has been measured (Luanti's
/// starting resend_timeout).
const RESEND_INTERVAL: Duration = Duration::from_millis(500);
const PEER_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// minetest_game's furnaces run a one-second node timer.
const FURNACE_STEP_INTERVAL: Duration = Duration::from_secs(1);
/// Leaf decay and sapling timers handled per tick, at most (each may look
/// at a few hundred nodes).
const TIMERS_PER_TICK: usize = 8;
/// How far chest sounds carry, in nodes (minetest_game's max_hear_distance).
const CHEST_HEARING: f32 = 10.0;
/// How far furnace sounds carry, in nodes (minetest_game's max_hear_distance).
const FURNACE_HEARING: f32 = 16.0;
/// How far sounds carry by default (Luanti's max_hear_distance).
const HEARING: f32 = 32.0;
/// How often the heaps are checked for damage.
const HEAP_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// How many recent events the heap check reports.
const RECENT: usize = 32;
/// How far a door is heard opening and closing (its max_hear_distance).
const DOOR_HEARING: f32 = 10.0;
/// Wi-Fi power saving goes on after this long with nobody doing anything
/// (no one connected, or every player standing still).
const IDLE_POWER_SAVE: Duration = Duration::from_secs(30);
/// How many reliable packets may be in flight (unacknowledged) per channel.
/// Luanti's own minimum window is 32.
const RELIABLE_WINDOW: usize = 32;
/// Reliable packets from a client that arrive ahead of a missing one are
/// kept (and acknowledged) if they are at most this far ahead, as Luanti
/// does. Dropping them instead makes the client resend them with growing
/// delays, and it gives up on a packet not acknowledged within 30 s.
const EARLY_WINDOW: u16 = 512;
/// How often to repeat "please reconnect" to a client from before a restart.
const STALE_NOTICE_INTERVAL: Duration = Duration::from_secs(3);
/// If the loop has been busy this long without waiting for packets, it
/// pauses for one scheduler tick so lower-priority tasks (including the
/// idle task the watchdog checks) get to run.
const MAX_BUSY: Duration = Duration::from_millis(200);
/// Time per pass of the main loop for generating map blocks, so incoming
/// packets are read often enough not to overflow the socket's queue.
const BLOCK_TIME_BUDGET: Duration = Duration::from_millis(15);
/// When a send fails because the Wi-Fi transmit buffers are full, hold
/// back non-urgent sending (new reliable packets, map blocks, resends) for
/// this long so the radio can catch up.
const CONGESTION_BACKOFF: Duration = Duration::from_millis(20);
/// A packet that couldn't be sent is tried again after this long (rather
/// than the full RESEND_INTERVAL).
const RETRY_UNSENT: Duration = Duration::from_millis(30);
/// Each resend of a packet waits this much longer than the last (Luanti).
const RESEND_SCALE_BASE: f32 = 1.5;
/// Report failed sends at most this often.
const SEND_FAILURE_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// Sent to clients in TOCLIENT_AUTH_ACCEPT. Luanti's default
/// `dedicated_server_step` is 0.09 seconds.
const SEND_INTERVAL: f32 = 0.09;
const MAP_SEED: u64 = 0;
/// Start the day at 10:00, running at Luanti's default speed.

/// Horizontal view radius in map blocks (the client may ask for less).
const MAX_VIEW_RADIUS: i16 = 5;
/// Keep at most this many packets queued on the block channel, so blocks
/// are generated as the client takes them rather than all at once.
const BLOCK_QUEUE_LIMIT: usize = 8;
/// Blocks taking longer than this to build are reported in the log.
const SLOW_BLOCK: Duration = Duration::from_millis(500);
/// How many map blocks the builder thread may have in hand at once. Few,
/// so that newly wanted (nearer) blocks don't wait behind a long queue.
const BUILDS_IN_FLIGHT: usize = 4;
/// Built blocks kept for other players (or a return visit); emptied when
/// full. A block is about 1 to 3 KB.
const BUILT_CACHE_MAX: usize = 600;
/// Block data goes on channel 2, like the Luanti server.
const BLOCK_CHANNEL: usize = 2;
const PRIVILEGES: &[&str] = &["interact", "shout", "fly", "fast", "noclip"];
/// How far (in nodes, from the player's feet) a player may dig or build.
/// The hand's range is 4 nodes from the eyes; this leaves room for lag.
const MAX_INTERACT_DISTANCE: f32 = 8.0;
/// Hotbar slots shown by the client (Luanti's default).
const HOTBAR_SIZE: usize = 8;
/// How often a moving player's position is saved to the SD card.
const SAVE_INTERVAL: Duration = Duration::from_secs(30);
/// How many players can be in the game at once (Luanti's max_users,
/// default 15 there). More would overload the ESP32: see the README.
const MAX_PLAYERS: usize = 6;
/// Luanti's chat limits (chat_message_max_size, and
/// chat_message_limit_per_10sec).
const CHAT_MAX_CHARS: usize = 500;
const CHAT_MAX_PER_10S: usize = 8;
const CHAT_HELP: &str = "\
Commands: /help, /me <action>, /msg <player> <message>, /status, /time";
/// The welcome message, shown in the chat area when a player joins, one
/// line per message. A /sd/motd.txt file replaces it.
const WELCOME: &str = "\
Welcome! This Luanti server runs on an ESP32-S3 microcontroller: a Waveshare ESP32-S3-LCD-1.47 board, with 512 KB of RAM plus 8 MB of PSRAM.
It is written from scratch in Rust, with no Lua: the world, digging and building, torches, trees, items and skins are all built in. Up to 6 players.
Source: https://github.com/kf106/esp32-s3-luanti
Chat works as usual; type /help for the commands.";
/// How often dropped items are moved on.
const ITEM_STEP_INTERVAL: Duration = Duration::from_millis(50);
/// builtin's item_drop: thrown from 1.2 nodes above the feet, at 2.9
/// nodes per second along the look direction plus 2 upwards.
const DROP_HEIGHT: f32 = 1.2;
const DROP_SPEED: f32 = 2.9;
const DROP_LIFT: f32 = 2.0;

struct Unacked {
    seqnum: u16,
    data: Vec<u8>,
    /// When it first went out (None: not yet, the radio was full), for the
    /// round-trip time when its ACK comes.
    first_sent: Option<Instant>,
    /// When to send it (again) if no ACK has come.
    due: Instant,
    /// Times it has been resent.
    resends: u32,
}

struct Channel {
    /// Next seqnum we will use for an outgoing reliable packet.
    next_out: u16,
    /// Next seqnum we expect from the client.
    next_in: u16,
    /// Next sequence number for an outgoing split packet.
    next_split: u16,
    /// Reliable packets we have sent that the client hasn't acknowledged.
    unacked: Vec<Unacked>,
    /// Packet bodies waiting for room in the reliable window.
    queue: VecDeque<Vec<u8>>,
    /// Reliable packets from the client that arrived before `next_in`'s,
    /// by sequence number: the inner packet's bytes.
    early: HashMap<u16, Vec<u8>>,
    /// Commands from the client that came split into chunks, by their
    /// split sequence number, while chunks are still to come.
    splits: HashMap<u16, IncomingSplit>,
}

/// One chunk of a split command, as it arrived. (A struct, as Xtensa
/// function calls carry only six argument words.)
struct SplitChunk<'a> {
    seqnum: u16,
    count: u16,
    num: u16,
    data: &'a [u8],
}

/// A command arriving in chunks (Luanti's IncomingSplitPacket).
struct IncomingSplit {
    chunks: Vec<Option<Vec<u8>>>,
    received: usize,
    started: Instant,
}

/// At most this many chunks in one split command: a client's biggest is a
/// request for every media file, a few kilobytes.
const MAX_SPLIT_CHUNKS: u16 = 64;
/// At most this many split commands being put together on a channel.
const MAX_SPLITS: usize = 4;
/// Unfinished split commands are dropped after this long (Luanti drops
/// unreliable ones after 30 s; reliable ones always complete).
const SPLIT_TIMEOUT: Duration = Duration::from_secs(30);

impl Channel {
    fn new() -> Self {
        Self {
            next_out: SEQNUM_INITIAL,
            next_in: SEQNUM_INITIAL,
            next_split: SEQNUM_INITIAL,
            unacked: Vec::new(),
            queue: VecDeque::new(),
            early: HashMap::new(),
            splits: HashMap::new(),
        }
    }
}

struct Peer {
    id: u16,
    channels: [Channel; CHANNEL_COUNT],
    last_seen: Instant,
    name: Option<String>,
    hello_sent: bool,
    /// Auth mechanisms we offered in TOCLIENT_HELLO.
    allowed_auth_mechs: u32,
    authenticated: bool,
    content_sent: bool,
    ready: bool,
    /// Blocks the client has (sent, and not reported deleted).
    sent_blocks: crate::fasthash::FastSet<BlockPos>,
    /// Blocks still to send, nearest first.
    pending_blocks: VecDeque<BlockPos>,
    /// Blocks being built for this player, in the order wanted.
    awaiting: VecDeque<BlockPos>,
    /// Block the player was in when `pending_blocks` was planned.
    view_center: Option<BlockPos>,
    view_radius: i16,
    /// The player's position in BS units (10 per node), velocity and yaw,
    /// as last reported. The player's object id is the peer id.
    pos_bs: [f32; 3],
    speed_bs: [f32; 3],
    yaw: f32,
    /// The animation other clients are showing for this player.
    anim: Animation,
    /// SRP login in progress: set by SRP_BYTES_A, checked by SRP_BYTES_M.
    srp: Option<srp::Challenge>,
    /// SRP_BYTES_A was received; Luanti allows only one try per connection.
    srp_started: bool,
    /// Loaded from the SD card when the player joins.
    inventory: Inventory,
    pitch: f32,
    hp: u16,
    /// What was last written to the player file, and when.
    saved: Option<PlayerData>,
    saved_at: Instant,
    /// The key of the skin they wear (see skins.rs).
    skin: String,
    /// When their recent chat messages were sent, for the flood limit.
    chat_times: VecDeque<Instant>,
    /// The furnace whose window they have open.
    furnace: Option<NodePos>,
    /// When the furnace window was last sent (opened or updated).
    furnace_form_sent: Option<Instant>,
    /// Keys held, as last reported.
    keys: u32,
    /// The chest whose window they have open.
    chest: Option<NodePos>,
    /// Average round-trip time (seconds) from ACKs, and the resend timeout
    /// it gives (Luanti's UDPPeer::reportRTT).
    avg_rtt: Option<f32>,
    resend_timeout: Duration,
}

impl Peer {
    fn new(id: u16) -> Self {
        Self {
            id,
            channels: std::array::from_fn(|_| Channel::new()),
            last_seen: Instant::now(),
            name: None,
            hello_sent: false,
            allowed_auth_mechs: 0,
            authenticated: false,
            content_sent: false,
            ready: false,
            sent_blocks: Default::default(),
            pending_blocks: VecDeque::new(),
            awaiting: VecDeque::new(),
            view_center: None,
            view_radius: MAX_VIEW_RADIUS,
            pos_bs: [0.0; 3],
            speed_bs: [0.0; 3],
            yaw: 0.0,
            anim: Animation::new(ANIM_STAND, ANIM_FPS),
            srp: None,
            srp_started: false,
            inventory: Inventory::new(),
            pitch: 0.0,
            hp: PLAYER_MAX_HP,
            saved: None,
            saved_at: Instant::now(),
            skin: skins::DEFAULT_SKIN.to_string(),
            chat_times: VecDeque::new(),
            furnace: None,
            furnace_form_sent: None,
            keys: 0,
            chest: None,
            avg_rtt: None,
            resend_timeout: RESEND_INTERVAL,
        }
    }

    /// A round-trip time from an ACK: keep a running average, and resend
    /// after twice it, from 0.1 to 2 seconds (Luanti's reportRTT).
    fn report_rtt(&mut self, rtt: f32) {
        let avg = match self.avg_rtt {
            Some(avg) => avg * 0.9 + rtt * 0.1,
            None => rtt,
        };
        self.avg_rtt = Some(avg);
        self.resend_timeout = Duration::from_secs_f32((avg * 2.0).clamp(0.1, 2.0));
    }

    fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("?")
    }

    fn player_data(&self) -> PlayerData {
        PlayerData {
            pos: self.pos_bs.map(|v| v / 10.0),
            yaw: self.yaw,
            pitch: self.pitch,
            hp: self.hp,
            skin: self.skin.clone(),
        }
    }

    /// Write the player file if this is a player in the game and anything
    /// changed since the last save.
    fn save(&mut self, store: &PlayerStore) {
        if !self.ready {
            return;
        }
        let data = self.player_data();
        if self.saved.as_ref() != Some(&data) {
            store.save(self.display_name(), &data);
            self.saved = Some(data);
        }
        self.saved_at = Instant::now();
    }
}

/// Content packets built once at startup and sent to every client.
struct Content {
    itemdef: Vec<u8>,
    nodedef: Vec<u8>,
    announce_media: Vec<u8>,
    media: Media,
}

pub struct Server {
    socket: UdpSocket,
    auth: AuthStore,
    world: World,
    inventories: InventoryStore,
    players: PlayerStore,
    furnaces: Furnaces,
    chests: Chests,
    /// Chests shown with their lids up (someone has them open).
    open_chests: HashSet<NodePos>,
    status: SharedStatus,
    content: Content,
    peers: HashMap<SocketAddr, Peer>,
    next_peer_id: u16,
    /// Which player send_blocks starts with, rotating.
    block_turn: usize,
    /// Addresses of clients left over from before a restart, and when we
    /// last told them to reconnect.
    stale_told: HashMap<SocketAddr, Instant>,
    /// SRP challenges are computed on another thread (they take about a
    /// second on the ESP32); results come back here.
    srp_results: Receiver<SrpResult>,
    /// Logins waiting for the SRP thread; None if it couldn't be started.
    srp_jobs: Option<Sender<SrpJob>>,
    /// The thread on the second core that builds map blocks (None if it
    /// couldn't be started: then blocks are built in the server loop).
    builder: Option<BlockBuilder>,
    /// Blocks the builder is working on.
    building: HashSet<BlockPos>,
    /// Built blocks (TOCLIENT_BLOCKDATA commands), with the world version
    /// they were built from.
    built: crate::fasthash::FastMap<BlockPos, (u64, Arc<Vec<u8>>)>,
    /// Dropped items, and when they were last moved on.
    items: Items,
    items_stepped: Instant,
    /// When the furnaces' timers last ran.
    furnaces_stepped: Instant,
    /// The time of day, kept over restarts.
    clock: Clock,
    /// Who owns each steel door.
    door_owners: doors::Owners,
    /// The last RECENT things the server did, for the heap check.
    recent: VecDeque<(Instant, &'static str, u16)>,
    heap_checked: Instant,
    heap_check_timed: bool,
    heap_damaged: bool,
    /// Id for the next sound that may need fading out.
    next_sound_id: i32,
    /// When a player last did something (moved, looked, dug, chatted...)
    /// or a client was logging in, and whether Wi-Fi power saving is on.
    last_activity: Instant,
    power_saving: bool,
    /// Leaf decay and sapling timers.
    timers: Timers,
    rng: Rng,
    skins: Skins,
    /// Welcome message lines.
    welcome: Vec<String>,
    /// Sending is held back until then (see CONGESTION_BACKOFF).
    congested_until: Instant,
    /// Sends that failed since the last report, and when that was.
    send_failures: u32,
    send_failures_reported: Instant,
    /// Reliable packets resent, and ACKs received, since the last report.
    resends: u32,
    acks: u32,
}

/// Everything the server needs, gathered up by main().
pub struct Setup {
    pub socket: UdpSocket,
    /// The welcome message from the SD card, replacing WELCOME.
    pub motd: Option<String>,
    pub auth: AuthStore,
    pub world: World,
    pub inventories: InventoryStore,
    pub players: PlayerStore,
    pub furnaces: Furnaces,
    pub chests: Chests,
    pub items: Items,
    pub clock: Clock,
    pub door_owners: doors::Owners,
    pub media: Media,
    pub status: SharedStatus,
}

/// A login for the SRP thread to work out.
struct SrpJob {
    addr: SocketAddr,
    peer_id: u16,
    /// The name exactly as the client sent it: it is part of the proof.
    name: String,
    account: auth::Account,
    bytes_a: Vec<u8>,
}

/// The SRP thread, started once, at startup: starting a thread per login
/// needs 16 KB of internal RAM in one piece, which may not be there once
/// the server has been running a while. Logins queue up and are worked
/// out one at a time.
fn start_srp_thread(results: Sender<SrpResult>) -> Option<Sender<SrpJob>> {
    let (jobs, queue) = mpsc::channel::<SrpJob>();
    // Measured at 2 KB used, but once 6.5 KB (see the "Stack left"
    // report): room to spare.
    let spawned = spawn_on_second_core(b"srp\0", 12 * 1024, 4, move || {
            for job in queue {
                let started = Instant::now();
                let challenge = srp::start(&srp::Login {
                    username: &job.name,
                    salt: &job.account.salt,
                    verifier: &job.account.verifier,
                    bytes_a: &job.bytes_a,
                });
                let _ = results.send(SrpResult {
                    addr: job.addr,
                    peer_id: job.peer_id,
                    salt: job.account.salt,
                    challenge,
                    took: started.elapsed(),
                });
            }
        });
    match spawned {
        Ok(_) => Some(jobs),
        Err(e) => {
            log::error!("Starting the SRP thread: {e}; existing accounts won't be able to log in");
            None
        }
    }
}

/// A map block for the builder to make, from a snapshot of the world's
/// changes.
struct BuildJob {
    pos: BlockPos,
    state: Arc<world::State>,
}

/// A finished block: the TOCLIENT_BLOCKDATA command, and the world version
/// it was built from.
struct Built {
    pos: BlockPos,
    version: u64,
    payload: Vec<u8>,
}

struct BlockBuilder {
    jobs: Sender<BuildJob>,
    done: Receiver<Built>,
}

/// Start the thread that builds (generates and compresses) map blocks, on
/// the second CPU core, which otherwise has little to do: the server loop
/// on the first core then only sends them.
fn start_block_builder(status: SharedStatus, port: u16) -> Option<BlockBuilder> {
    let (jobs, queue) = mpsc::channel::<BuildJob>();
    let (finished, done) = mpsc::channel::<Built>();
    // Each finished block is followed by an empty datagram to the server's
    // own port, which wakes the server loop from waiting for packets so it
    // sends the block straight away.
    let nudge = UdpSocket::bind(("127.0.0.1", 0))
        .map_err(|e| warn!("Block builder: no wake-up socket ({e}); blocks may wait up to {POLL_INTERVAL:?}"))
        .ok();
    // Priority 1, just above core 1's idle task; logins and the display
    // come first. After each block it pauses for a scheduler tick so the
    // idle task (which the task watchdog checks on) gets a turn.
    // Measured: about 7.5 KB used (see the "Stack left" report).
    let spawned = spawn_on_second_core(b"blocks\0", 16 * 1024, 1, move || {
        // Its own copy of the terrain caches; the changes come with each job.
        let mut world = World::builder();
        for job in queue {
            let started = Instant::now();
            let version = job.state.version();
            world.use_snapshot(job.state);
            let payload = build_blockdata(&world.block_payload(job.pos));
            let took = started.elapsed();
            if took >= SLOW_BLOCK {
                warn!("Block {:?} took {} ms to build", job.pos, took.as_millis());
            }
            {
                let mut st = status::lock(&status);
                st.blocks_built += 1;
                st.block_us += took.as_micros() as u64;
            }
            if finished.send(Built { pos: job.pos, version, payload }).is_err() {
                break;
            }
            if let Some(n) = &nudge {
                let _ = n.send_to(&[], ("127.0.0.1", port));
            }
            let_other_tasks_run();
        }
    });
    match spawned {
        Ok(()) => Some(BlockBuilder { jobs, done }),
        Err(e) => {
            warn!("Starting the block builder: {e}; building blocks in the server loop instead");
            None
        }
    }
}

/// Start a thread pinned to the second core (core 1). The server loop runs
/// on core 0, with the Wi-Fi driver; core 1 is otherwise nearly idle.
/// `priority` is FreeRTOS's (the server loop has 1; higher runs first).
#[cfg(target_os = "espidf")]
fn spawn_on_second_core<F>(name: &'static [u8], stack: usize, priority: u8, f: F) -> std::io::Result<()>
where
    F: FnOnce() + Send + 'static,
{
    use esp_idf_svc::sys::{esp_pthread_get_default_config, esp_pthread_set_cfg};
    // The next thread this one starts takes these settings.
    unsafe {
        let mut cfg = esp_pthread_get_default_config();
        cfg.thread_name = name.as_ptr() as *const _;
        cfg.pin_to_core = 1;
        cfg.prio = priority as _;
        cfg.stack_size = stack as _;
        esp_pthread_set_cfg(&cfg);
    }
    let spawned = std::thread::Builder::new().stack_size(stack).spawn(f).map(|_| ());
    unsafe {
        let cfg = esp_pthread_get_default_config();
        esp_pthread_set_cfg(&cfg);
    }
    spawned
}

#[cfg(not(target_os = "espidf"))]
fn spawn_on_second_core<F>(name: &'static [u8], stack: usize, _priority: u8, f: F) -> std::io::Result<()>
where
    F: FnOnce() + Send + 'static,
{
    let name = String::from_utf8_lossy(name.strip_suffix(b"\0").unwrap_or(name)).into_owned();
    std::thread::Builder::new().name(name).stack_size(stack).spawn(f).map(|_| ())
}

struct SrpResult {
    addr: SocketAddr,
    /// The peer id when the work started, in case the address was reused.
    peer_id: u16,
    salt: Vec<u8>,
    challenge: Option<srp::Challenge>,
    took: Duration,
}

/// Pause for one FreeRTOS tick. (A short std sleep would busy-wait.)
#[cfg(target_os = "espidf")]
fn let_other_tasks_run() {
    unsafe { esp_idf_svc::sys::vTaskDelay(1) };
}

#[cfg(not(target_os = "espidf"))]
fn let_other_tasks_run() {
    std::thread::yield_now();
}

impl Server {
    /// Takes one struct rather than six arguments: with the hidden return
    /// pointer that made seven, one more than the Xtensa calling
    /// convention passes in registers, and the seventh (the status) arrived
    /// as garbage on the ESP32.
    pub fn new(setup: Setup) -> Self {
        let Setup { socket, motd, auth, world, inventories, players, furnaces, chests, mut items, clock, door_owners, media, status } = setup;
        // Items dropped before a restart: out of any block built over them.
        items.settle(&|x, y, z| content::walkable(world.node((x, y, z))));
        let itemdef = build_itemdef(&content::itemdef_bytes());
        let nodedef = build_nodedef(&content::nodedef_bytes());
        let announce_media = build_announce_media(
            media.files().iter().map(|f| (f.name.as_str(), &f.sha1)),
        );
        info!(
            "Content: {} nodes, ITEMDEF {} bytes, NODEDEF {} bytes, ANNOUNCE_MEDIA {} bytes",
            content::NODES.len(),
            itemdef.len(),
            nodedef.len(),
            announce_media.len()
        );
        let skins = Skins::from_media(&media);
        let (srp_sender, srp_results) = mpsc::channel();
        let srp_jobs = start_srp_thread(srp_sender);
        let port = socket.local_addr().map(|a| a.port()).unwrap_or(0);
        let builder = start_block_builder(status.clone(), port);
        // Saplings planted before a restart: timers again, as when planted
        // (minetest_game's node timers survive a restart; ours don't).
        let mut timers = Timers::new();
        let mut rng = Rng::new();
        let saplings = world.changed_nodes(content::is_sapling);
        for &pos in &saplings {
            timers.start(pos, Job::Grow, nature::grow_delay(&mut rng));
        }
        if !saplings.is_empty() {
            info!("{} saplings growing", saplings.len());
        }
        for pos in world.changed_nodes(content::is_mark) {
            timers.start(pos, Job::Regrow, nature::grow_delay(&mut rng));
        }
        // Chests left open when the server stopped are closed again
        // (minetest_game's "close opened chests on load").
        let mut world = world;
        for pos in world.changed_nodes(|id| content::chest_pair(id).is_some_and(|(_, open)| open == id)) {
            if let Some((closed, _)) = content::chest_pair(world.node(pos)) {
                let param2 = world.param2(pos);
                world.set_node(pos, closed, param2);
            }
        }
        Self {
            socket,
            auth,
            world,
            inventories,
            players,
            furnaces,
            chests,
            open_chests: HashSet::new(),
            status,
            content: Content {
                itemdef,
                nodedef,
                announce_media,
                media,
            },
            peers: HashMap::new(),
            next_peer_id: 2,
            block_turn: 0,
            stale_told: HashMap::new(),
            srp_results,
            srp_jobs,
            builder,
            building: HashSet::new(),
            built: Default::default(),
            items,
            clock,
            door_owners,
            recent: VecDeque::new(),
            heap_checked: Instant::now(),
            heap_check_timed: false,
            heap_damaged: false,
            items_stepped: Instant::now(),
            furnaces_stepped: Instant::now(),
            next_sound_id: 1,
            last_activity: Instant::now(),
            power_saving: false,
            timers,
            rng,
            skins,
            welcome: motd
                .as_deref()
                .unwrap_or(WELCOME)
                .lines()
                .map(str::trim_end)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
            congested_until: Instant::now(),
            send_failures: 0,
            send_failures_reported: Instant::now(),
            resends: 0,
            acks: 0,
        }
    }

    /// Where the shared status data is, as seen through the `status` field.
    pub fn status_data_addr(&self) -> usize {
        Arc::as_ptr(&self.status) as usize
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        self.socket.set_read_timeout(Some(POLL_INTERVAL))?;
        let mut buf = [0u8; 1500];
        let mut last_wait = Instant::now();
        // For the status screen: how long the loop works rather than waits.
        let (mut busy, mut reported) = (Duration::ZERO, Instant::now());
        loop {
            // Wait up to POLL_INTERVAL for a packet, then take everything
            // else already waiting before doing anything slow: the socket's
            // queue is short, and packets that don't fit are lost.
            let pass = Instant::now();
            let received = self.socket.recv_from(&mut buf);
            let waited = pass.elapsed();
            match received {
                Ok((len, from)) => {
                    self.handle_datagram(&buf[..len], from);
                    self.socket.set_nonblocking(true)?;
                    let drained = loop {
                        match self.socket.recv_from(&mut buf) {
                            Ok((len, from)) => self.handle_datagram(&buf[..len], from),
                            Err(e) => break e,
                        }
                    };
                    self.socket.set_nonblocking(false)?;
                    if !matches!(drained.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
                        return Err(drained);
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    last_wait = Instant::now(); // we were idle for a while
                }
                Err(e) => return Err(e),
            }
            self.tick();
            if last_wait.elapsed() >= MAX_BUSY {
                let_other_tasks_run();
                last_wait = Instant::now();
            }
            busy += pass.elapsed().saturating_sub(waited);
            if reported.elapsed() >= Duration::from_millis(250) {
                status::lock(&self.status).busy_us += busy.as_micros() as u64;
                busy = Duration::ZERO;
                reported = Instant::now();
            }
        }
    }

    fn handle_datagram(&mut self, data: &[u8], from: SocketAddr) {
        // The block builder's "a block is ready" nudge (see BlockBuilder):
        // it only needed to wake the loop.
        if data.is_empty() && from.ip().is_loopback() {
            return;
        }
        status::lock(&self.status).packets_in += 1;
        let dg = match parse_datagram(data) {
            Ok(dg) => dg,
            Err(e) => {
                warn!("Bad datagram from {from}: {e}");
                hexdump(data);
                return;
            }
        };

        match self.peers.get(&from) {
            // A client uses peer id 0 until we give it one. A non-zero id
            // from an address we don't know is a session from before the
            // server restarted: Luanti ignores those, and so do we, after
            // telling the client to reconnect.
            None if dg.peer_id != PEER_ID_INEXISTENT => {
                self.reject_stale(from, dg.peer_id);
                return;
            }
            None => {
                if matches!(dg.packet, Packet::Control(Control::Disco)) {
                    return;
                }
                self.add_peer(from);
            }
            // The same address with someone else's id: also stale.
            Some(peer) if dg.peer_id != PEER_ID_INEXISTENT && dg.peer_id != peer.id => {
                debug!("Packet from {from} with peer id {} (expected {}); ignored", dg.peer_id, peer.id);
                return;
            }
            Some(_) => {}
        }
        if let Some(peer) = self.peers.get_mut(&from) {
            peer.last_seen = Instant::now();
        }

        self.handle_packet(from, dg.channel as usize, dg.packet);
    }

    /// Handle kept packets that are now next in order.
    fn process_early(&mut self, from: SocketAddr, channel: usize) {
        loop {
            let Some(peer) = self.peers.get_mut(&from) else {
                return;
            };
            let ch = &mut peer.channels[channel];
            let Some(raw) = ch.early.remove(&ch.next_in) else {
                return;
            };
            ch.next_in = ch.next_in.wrapping_add(1);
            match parse_inner(&raw) {
                Ok(packet) => self.handle_packet(from, channel, packet),
                Err(e) => warn!("Bad kept packet from {from}: {e}"),
            }
        }
    }

    /// Tell a client still connected to a previous run of the server that
    /// it must reconnect: an unreliable ACCESS_DENIED (we don't know its
    /// sequence numbers) with the reconnect button, then DISCO. At most
    /// every few seconds per address, as it keeps sending until it stops.
    fn reject_stale(&mut self, from: SocketAddr, peer_id: u16) {
        let now = Instant::now();
        if let Some(last) = self.stale_told.get(&from) {
            if now.duration_since(*last) < STALE_NOTICE_INTERVAL {
                return;
            }
        } else {
            info!("{from} is still using peer id {peer_id} from before the server restarted; telling it to reconnect");
        }
        if self.stale_told.len() >= 64 {
            self.stale_told.retain(|_, t| now.duration_since(*t) < Duration::from_secs(60));
        }
        self.stale_told.insert(from, now);
        let denied = build_access_denied(
            AccessDenied::CustomString,
            "The server restarted. Please reconnect.",
            true,
        );
        self.send_raw(from, &unreliable_datagram(0, &denied));
        self.send_raw(from, &disco_datagram());
    }

    fn handle_packet(&mut self, from: SocketAddr, channel: usize, packet: Packet<'_>) {
        match packet {
            Packet::Reliable { seqnum, inner, raw } => {
                let Some(peer) = self.peers.get_mut(&from) else {
                    return;
                };
                let ch = &mut peer.channels[channel];
                let expected = ch.next_in;

                if seqnum == expected {
                    ch.next_in = expected.wrapping_add(1);
                    self.send_raw(from, &ack_datagram(channel as u8, seqnum));
                    self.handle_packet(from, channel, *inner);
                    self.process_early(from, channel);
                } else if seqnum_higher(expected, seqnum) {
                    // Already processed; our ACK must have been lost, so resend it.
                    debug!("Duplicate reliable {seqnum} on ch{channel}");
                    self.send_raw(from, &ack_datagram(channel as u8, seqnum));
                } else if seqnum.wrapping_sub(expected) < EARLY_WINDOW {
                    // Arrived ahead of one that was lost: keep it for later
                    // and acknowledge it now.
                    debug!("Early reliable {seqnum} on ch{channel} (expecting {expected}), kept");
                    ch.early.entry(seqnum).or_insert_with(|| raw.to_vec());
                    self.send_raw(from, &ack_datagram(channel as u8, seqnum));
                } else {
                    warn!("Reliable {seqnum} on ch{channel} is too far ahead of {expected}; dropped");
                }
            }

            Packet::Control(Control::Ack(seqnum)) => {
                if let Some(peer) = self.peers.get_mut(&from) {
                    let unacked = &mut peer.channels[channel].unacked;
                    if let Some(i) = unacked.iter().position(|u| u.seqnum == seqnum) {
                        // Round-trip time from its first sending ("a bit off
                        // for re-sent packets but that's okay", as Luanti says).
                        if let Some(sent) = unacked.remove(i).first_sent {
                            peer.report_rtt(sent.elapsed().as_secs_f32());
                            self.acks += 1;
                        }
                    }
                }
                self.flush(from, channel);
            }

            Packet::Control(Control::Ping) => {
                // Nothing to do: if it came reliably, it has already been ACKed.
            }

            Packet::Control(Control::Disco) => {
                if let Some(mut peer) = self.peers.remove(&from) {
                    peer.save(&self.players);
                    info!("Peer {} ({}) disconnected", peer.id, peer.display_name());
                    if peer.ready {
                        self.remove_player_object(peer.id);
                        self.event(format!("{} left", peer.display_name()));
                        status::lock(&self.status).leaves += 1;
                    }
                    self.close_unwatched_lids();
                    self.refresh_players();
                }
            }

            Packet::Control(other) => {
                warn!("Unexpected control packet from {from}: {other}");
            }

            Packet::Original(payload) => self.handle_command(from, channel, payload),

            Packet::Split {
                seqnum,
                chunk_count,
                chunk_num,
                data,
            } => {
                let chunk = SplitChunk { seqnum, count: chunk_count, num: chunk_num, data };
                if let Some(payload) = self.add_split_chunk(from, channel, chunk) {
                    self.handle_command(from, channel, &payload);
                }
            }
        }
    }

    /// One chunk of a split command (Luanti's IncomingSplitBuffer::insert):
    /// the whole command once its last chunk is in.
    fn add_split_chunk(&mut self, from: SocketAddr, channel: usize, chunk: SplitChunk<'_>) -> Option<Vec<u8>> {
        let SplitChunk { seqnum, count, num, data } = chunk;
        let ch = &mut self.peers.get_mut(&from)?.channels[channel];
        if count == 0 || count > MAX_SPLIT_CHUNKS || num >= count {
            warn!("Split packet {seqnum} from {from}: chunk {num} of {count} refused");
            return None;
        }
        ch.splits.retain(|_, s| s.started.elapsed() < SPLIT_TIMEOUT);
        if !ch.splits.contains_key(&seqnum) && ch.splits.len() >= MAX_SPLITS {
            warn!("Split packet {seqnum} from {from}: too many unfinished; dropped");
            return None;
        }
        let split = ch.splits.entry(seqnum).or_insert_with(|| IncomingSplit {
            chunks: vec![None; count as usize],
            received: 0,
            started: Instant::now(),
        });
        if split.chunks.len() != count as usize {
            warn!("Split packet {seqnum} from {from}: chunk count changed from {} to {count}", split.chunks.len());
            return None;
        }
        let slot = &mut split.chunks[num as usize];
        if slot.is_none() {
            *slot = Some(data.to_vec());
            split.received += 1;
        }
        if split.received < split.chunks.len() {
            return None;
        }
        let split = ch.splits.remove(&seqnum)?;
        let payload: Vec<u8> = split.chunks.into_iter().flatten().flatten().collect();
        debug!("Split packet {seqnum} from {from}: {count} chunks, {} bytes", payload.len());
        Some(payload)
    }

    fn handle_command(&mut self, from: SocketAddr, channel: usize, payload: &[u8]) {
        let Some(cmd) = messages::command_id(payload) else {
            return; // empty ORIGINAL packet: the client's "hello", nothing to do
        };
        let peer_id = self.peers.get(&from).map_or(0, |p| p.id);
        self.note_command(cmd, peer_id);
        // Logging in and loading, and anything a player does, keep Wi-Fi
        // fully awake. (Positions count only if something changed: see
        // on_playerpos.)
        let ready = self.peers.get(&from).is_some_and(|p| p.ready);
        let acting = matches!(
            cmd,
            TOSERVER_INTERACT
                | TOSERVER_INVENTORY_ACTION
                | TOSERVER_INVENTORY_FIELDS
                | TOSERVER_CHAT_MESSAGE
                | TOSERVER_DAMAGE
                | TOSERVER_RESPAWN_LEGACY
        );
        if !ready || acting {
            self.wake();
        }
        match cmd {
            TOSERVER_INIT => self.on_init(from, payload),
            TOSERVER_FIRST_SRP => self.on_first_srp(from, payload),
            TOSERVER_SRP_BYTES_A => self.on_srp_bytes_a(from, payload),
            TOSERVER_SRP_BYTES_M => self.on_srp_bytes_m(from, payload),
            TOSERVER_INTERACT => self.on_interact(from, payload),
            TOSERVER_INVENTORY_ACTION => self.on_inventory_action(from, payload),
            TOSERVER_DAMAGE => self.on_damage(from, payload),
            TOSERVER_RESPAWN_LEGACY => self.on_respawn(from),
            TOSERVER_INIT2 => self.on_init2(from, payload),
            TOSERVER_REQUEST_MEDIA => self.on_request_media(from, payload),
            TOSERVER_CLIENT_READY => self.on_client_ready(from, payload),
            TOSERVER_PLAYERPOS => self.on_playerpos(from, payload),
            TOSERVER_DELETEDBLOCKS => self.on_deleted_blocks(from, payload),
            // The client confirms blocks it has meshed; we don't need that yet.
            TOSERVER_GOTBLOCKS => {}
            TOSERVER_INVENTORY_FIELDS => self.on_inventory_fields(from, payload),
            TOSERVER_CHAT_MESSAGE => self.on_chat(from, payload),
            // Window size and similar client details: nothing to do.
            0x0053 => {}
            // TOSERVER_REMOVED_SOUNDS: sounds the client finished playing.
            0x003a => {}
            other => {
                info!(
                    "Command 0x{other:04x} on ch{channel}, {} bytes:",
                    payload.len()
                );
                hexdump(&payload[..payload.len().min(64)]);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Login sequence
    // -----------------------------------------------------------------------

    fn on_init(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(init) = parse_init(payload) else {
            warn!("Malformed TOSERVER_INIT from {from}");
            hexdump(payload);
            return;
        };
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if peer.hello_sent {
            return; // the client resends INIT until it sees our HELLO
        }
        let id = peer.id;
        info!("TOSERVER_INIT from peer {id}: {init:?}");

        if init.max_proto < SERVER_PROTO_MIN || init.min_proto > SERVER_PROTO_MAX {
            warn!(
                "Client protocol range {}..={} is incompatible with ours {}..={}",
                init.min_proto, init.max_proto, SERVER_PROTO_MIN, SERVER_PROTO_MAX
            );
            self.deny(from, AccessDenied::WrongVersion, "");
            return;
        }

        match auth::check_name(&init.name) {
            Ok(()) => {}
            Err(NameError::BadChars) => {
                self.deny(from, AccessDenied::WrongCharsInName, "");
                return;
            }
            Err(NameError::Empty | NameError::TooLong) => {
                self.deny(from, AccessDenied::WrongName, "");
                return;
            }
        }

        // Checked now so a client isn't taken through logging in only to be
        // turned away, and again when it is ready to join (Luanti does both).
        if self.user_limit_reached(&init.name) {
            info!("{} tried to join, but the game is full ({MAX_PLAYERS} players)", init.name);
            self.deny(from, AccessDenied::TooManyUsers, "");
            return;
        }

        let auth_mechs = match self.auth.load(&init.name) {
            Ok(Some(_)) => AUTH_MECHANISM_SRP,
            Ok(None) => AUTH_MECHANISM_FIRST_SRP,
            Err(e) => {
                warn!("Reading account {}: {e}", init.name);
                self.deny(from, AccessDenied::ServerFail, "");
                return;
            }
        };

        let proto = init.max_proto.min(SERVER_PROTO_MAX);
        let ser_ver = init.max_ser_ver.min(SER_FMT_VER_HIGHEST);
        let hello = build_hello(ser_ver, proto, auth_mechs);

        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        peer.name = Some(init.name);
        peer.hello_sent = true;
        peer.allowed_auth_mechs = auth_mechs;
        info!(
            "Sending TOCLIENT_HELLO: protocol {proto}, serialisation {ser_ver}, auth {}",
            if auth_mechs == AUTH_MECHANISM_SRP { "SRP (existing account)" } else { "FIRST_SRP (new account)" }
        );
        self.send_command(from, 0, &hello);
    }

    fn on_first_srp(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(msg) = parse_first_srp(payload) else {
            warn!("Malformed TOSERVER_FIRST_SRP from {from}");
            self.deny(from, AccessDenied::UnexpectedData, "");
            return;
        };
        let Some(peer) = self.peers.get(&from) else {
            return;
        };
        if peer.authenticated {
            info!("{} tried to change password (not supported yet)", peer.display_name());
            return;
        }
        if !peer.hello_sent || peer.allowed_auth_mechs & AUTH_MECHANISM_FIRST_SRP == 0 {
            warn!("Unexpected TOSERVER_FIRST_SRP from {from}");
            self.deny(from, AccessDenied::UnexpectedData, "");
            return;
        }
        let name = peer.display_name().to_string();
        let mechs = peer.allowed_auth_mechs;

        if msg.is_empty {
            warn!("{name} is registering with an empty password");
        }
        let account = Account {
            salt: msg.salt,
            verifier: msg.verifier,
        };
        match self.auth.create(&name, &account) {
            Ok(true) => {
                info!(
                    "Registered account {name} (salt {} bytes, verifier {} bytes)",
                    account.salt.len(),
                    account.verifier.len()
                );
                self.event(format!("New account: {name}"));
            }
            Ok(false) => {
                warn!("{name} is already registered");
                self.deny(from, AccessDenied::AlreadyConnected, "");
                return;
            }
            Err(e) => {
                warn!("Saving account {name}: {e}");
                self.deny(from, AccessDenied::ServerFail, "");
                return;
            }
        }

        self.accept_auth(from, mechs);
    }

    /// Login succeeded: drop any older connection with the same name (most
    /// likely a client that crashed and hasn't timed out yet), then let the
    /// client in.
    fn accept_auth(&mut self, from: SocketAddr, mechs: u32) {
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        peer.authenticated = true;
        peer.srp = None;
        let name = peer.display_name().to_ascii_lowercase();
        let stale: Vec<SocketAddr> = self
            .peers
            .iter()
            .filter(|(a, p)| **a != from && p.authenticated && p.display_name().to_ascii_lowercase() == name)
            .map(|(a, _)| *a)
            .collect();
        for addr in stale {
            // Tell the old client why (if it is still there), then forget it.
            self.deny(addr, AccessDenied::AlreadyConnected, "");
            if let Some(mut old) = self.peers.remove(&addr) {
                old.save(&self.players);
                info!("{} logged in again; dropped the old connection from {addr}", old.display_name());
                if old.ready {
                    self.remove_player_object(old.id);
                }
            }
            self.close_unwatched_lids();
            self.refresh_players();
        }
        info!("Sending TOCLIENT_AUTH_ACCEPT to {name}");
        let accept = build_auth_accept(MAP_SEED, SEND_INTERVAL, mechs);
        self.send_command(from, 0, &accept);
    }

    fn on_srp_bytes_a(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some((bytes_a, based_on)) = parse_srp_bytes_a(payload) else {
            warn!("Malformed TOSERVER_SRP_BYTES_A from {from}");
            self.deny(from, AccessDenied::UnexpectedData, "");
            return;
        };
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if peer.authenticated {
            info!("{} asked to change password (not supported yet)", peer.display_name());
            return;
        }
        // based_on 0 would mean a legacy password hash; we only offer SRP.
        if !peer.hello_sent
            || peer.srp_started
            || peer.allowed_auth_mechs & AUTH_MECHANISM_SRP == 0
            || based_on != 1
        {
            warn!("Unexpected TOSERVER_SRP_BYTES_A from {from} (based_on {based_on})");
            self.deny(from, AccessDenied::UnexpectedData, "");
            return;
        }
        peer.srp_started = true;
        let name = peer.display_name().to_string();

        let account = match self.auth.load(&name) {
            Ok(Some(a)) => a,
            Ok(None) => {
                warn!("Account {name} disappeared during login");
                self.deny(from, AccessDenied::ServerFail, "");
                return;
            }
            Err(e) => {
                warn!("Reading account {name}: {e}");
                self.deny(from, AccessDenied::ServerFail, "");
                return;
            }
        };

        // The maths takes about a second on the ESP32, so it runs on its
        // own thread and the server carries on; tick() picks up the result.
        let peer_id = self.peers.get(&from).map_or(0, |p| p.id);
        let job = SrpJob { addr: from, peer_id, name, account, bytes_a };
        let queued = self.srp_jobs.as_ref().is_some_and(|jobs| jobs.send(job).is_ok());
        if !queued {
            warn!("The SRP thread isn't running; can't log {from} in");
            self.deny(from, AccessDenied::ServerFail, "");
        }
    }

    /// Send the salt and B for logins whose SRP maths has finished.
    fn finish_srp(&mut self) {
        while let Ok(r) = self.srp_results.try_recv() {
            let Some(peer) = self.peers.get_mut(&r.addr).filter(|p| p.id == r.peer_id) else {
                continue; // gone while we were working
            };
            let name = peer.display_name().to_string();
            let Some(challenge) = r.challenge else {
                warn!("{name}: SRP safety check failed on A");
                self.deny(r.addr, AccessDenied::UnexpectedData, "");
                continue;
            };
            info!("{name}: SRP challenge computed in {} ms", r.took.as_millis());
            let reply = build_srp_bytes_s_b(&r.salt, &challenge.bytes_b);
            peer.srp = Some(challenge);
            self.send_command(r.addr, 0, &reply);
        }
    }

    fn on_srp_bytes_m(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(m) = parse_srp_bytes_m(payload) else {
            warn!("Malformed TOSERVER_SRP_BYTES_M from {from}");
            self.deny(from, AccessDenied::UnexpectedData, "");
            return;
        };
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        let Some(challenge) = peer.srp.take() else {
            warn!("Unexpected TOSERVER_SRP_BYTES_M from {from}");
            self.deny(from, AccessDenied::UnexpectedData, "");
            return;
        };
        let name = peer.display_name().to_string();
        let mechs = peer.allowed_auth_mechs;
        if challenge.check(&m) {
            info!("{name} logged in");
            self.accept_auth(from, mechs);
        } else {
            info!("{name} supplied the wrong password");
            self.event(format!("Wrong password: {name}"));
            self.deny(from, AccessDenied::WrongPassword, "");
        }
    }

    /// Something worth showing on the status screen.
    fn event(&self, text: String) {
        status::lock(&self.status).event(text);
    }

    fn edit_event(&self, name: &str, verb: &str, id: u16) {
        let node = content::name_of(id);
        let node = node.split_once(':').map_or(node, |(_, n)| n).replace('_', " ");
        let mut st = status::lock(&self.status);
        st.edits += 1;
        st.event(format!("{name} {verb} {node}"));
    }

    /// Update the screen's player list: players in the game in joining
    /// order, and how many connections are still logging in or loading.
    fn refresh_players(&self) {
        let mut ready: Vec<(u16, String)> = self
            .peers
            .values()
            .filter(|p| p.ready)
            .map(|p| (p.id, p.display_name().to_string()))
            .collect();
        ready.sort();
        let mut st = status::lock(&self.status);
        st.joining = self.peers.len() - ready.len();
        st.players = ready.into_iter().map(|(_, n)| n).collect();
    }

    fn deny(&mut self, addr: SocketAddr, reason: AccessDenied, custom: &str) {
        info!("Denying access to {addr}: {reason:?} {custom}");
        let pkt = build_access_denied(reason, custom, false);
        self.send_command(addr, 0, &pkt);
    }

    // -----------------------------------------------------------------------
    // Content and joining
    // -----------------------------------------------------------------------

    fn on_init2(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.authenticated {
            warn!("TOSERVER_INIT2 from unauthenticated peer {}", peer.id);
            return;
        }
        if peer.content_sent {
            return;
        }
        peer.content_sent = true;
        let lang = parse_init2_lang(payload).unwrap_or_default();
        info!(
            "TOSERVER_INIT2 from {} (language \"{lang}\"): sending content",
            peer.display_name()
        );

        // Same order as the Luanti server, all on channel 0.
        let itemdef = self.content.itemdef.clone();
        let nodedef = self.content.nodedef.clone();
        let announce = self.content.announce_media.clone();
        self.send_command(from, 0, &itemdef);
        self.send_command(from, 0, &nodedef);
        self.send_command(from, 0, &announce);
        self.send_command(from, 0, &build_movement());
        // The time now, so every player's day matches.
        let now = self.time_of_day();
        self.send_command(from, 0, &build_time_of_day(now, clock::TIME_SPEED));
        self.send_command(from, 0, &build_csm_restriction_flags());
    }

    fn on_request_media(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(names) = parse_request_media(payload) else {
            warn!("Malformed TOSERVER_REQUEST_MEDIA from {from}");
            return;
        };
        let mut files = Vec::new();
        for name in &names {
            match self.content.media.get(name) {
                Some(f) => files.push((f.name.as_str(), f.data.as_slice())),
                None => warn!("Client asked for unknown media \"{name}\""),
            }
        }
        info!("Client requested {} media files: {}", names.len(), names.join(", "));
        let bunches = build_media_bunches(&files);
        for bunch in bunches {
            self.send_command(from, 2, &bunch);
        }
    }

    fn on_client_ready(&mut self, from: SocketAddr, payload: &[u8]) {
        let ready = parse_client_ready(payload);
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.content_sent || peer.ready {
            return;
        }
        let name = peer.display_name().to_string();
        // Two could have been logging in at once for the last place.
        if self.user_limit_reached(&name) {
            info!("{name} tried to join, but the game is full ({MAX_PLAYERS} players)");
            self.deny(from, AccessDenied::TooManyUsers, "");
            return;
        }
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        peer.ready = true;
        match ready {
            Some(r) => info!(
                "{} is in the game (client {}, formspec v{})",
                peer.display_name(),
                r.version,
                r.formspec_version
            ),
            None => info!("{} is in the game", peer.display_name()),
        }

        // Where they left off, or the spawn point for a newcomer.
        let id = peer.id;
        let name = peer.display_name().to_string();
        let saved = self.players.load(&name);
        let (spawn, yaw, pitch, hp) = match &saved {
            Some(d) => (self.safe_position(d.pos), d.yaw, d.pitch, d.hp),
            None => (self.spawn_point(id), 0.0, 0.0, PLAYER_MAX_HP),
        };
        // A skin that has since gone falls back to the default.
        let skin = saved.as_ref().map_or(skins::DEFAULT_SKIN, |d| d.skin.as_str());
        let skin = self.skins.get(self.skins.index_of(skin)).key.clone();
        info!(
            "{name} {} at ({:.1}, {:.1}, {:.1}) with {hp} HP",
            if saved.is_some() { "returns" } else { "spawns" },
            spawn[0],
            spawn[1],
            spawn[2]
        );

        let spawn_bs = spawn.map(|v| v * 10.0);
        if let Some(peer) = self.peers.get_mut(&from) {
            peer.pos_bs = spawn_bs;
            peer.yaw = yaw;
            peer.pitch = pitch;
            peer.hp = hp;
            peer.skin = skin;
            peer.saved = saved;
            peer.saved_at = Instant::now();
            peer.inventory = self.inventories.load(peer.display_name());
            let items: u32 = peer.inventory.main.iter().flatten().map(|s| s.count as u32).sum();
            info!("{} has {items} items", peer.display_name());
        }
        self.send_command(from, 0, &build_privileges(PRIVILEGES));
        // Frame ranges for the client's own model (third-person view)
        self.send_command(from, 0, &build_local_player_animations());
        self.send_command(from, 0, &build_move_player(spawn_bs, pitch, yaw));
        self.send_inventory(from);
        self.send_inventory_formspec(from);
        self.add_player_object(from);
        self.send_command(from, 0, &build_health_bar(hp));
        self.send_command(from, 0, &build_hp(hp, false));
        if hp == 0 {
            // Left while dead: back to the death screen, as Luanti does.
            self.send_command(from, 0, &build_deathscreen());
        }
        self.plan_blocks(from, spawn, MAX_VIEW_RADIUS);
        self.send_welcome(from);
        if let Some(peer) = self.peers.get(&from) {
            let name = peer.display_name().to_string();
            self.event(format!("{name} joined"));
            status::lock(&self.status).joins += 1;
        }
        self.refresh_players();
    }

    /// Tell a newly joined client about every player (itself included), and
    /// everyone else about the new player.
    fn add_player_object(&mut self, new: SocketAddr) {
        let Some(peer) = self.peers.get(&new) else {
            return;
        };
        let new_id = peer.id;
        let new_init = player_init_data(&PlayerInit {
            name: peer.display_name(),
            id: new_id,
            pos_bs: peer.pos_bs,
            yaw: peer.yaw,
            anim: peer.anim,
            skin: &self.skin_texture(&peer.skin),
        });

        // The client's own object must be in its first object packet: from
        // then on it calls LocalPlayer::isDead(), which needs that object.
        let mut for_new = vec![(new_id, new_init.clone())];
        let mut others = Vec::new();
        for (addr, p) in &self.peers {
            if *addr != new && p.ready {
                for_new.push((p.id, player_init_data(&PlayerInit {
                    name: p.display_name(),
                    id: p.id,
                    pos_bs: p.pos_bs,
                    yaw: p.yaw,
                    anim: p.anim,
                    skin: &self.skin_texture(&p.skin),
                })));
                others.push(*addr);
            }
        }
        for d in self.items.iter() {
            for_new.push((d.id, item_init(d)));
        }
        info!(
            "Adding player object {new_id}; {} other player(s) online",
            others.len()
        );
        self.send_command(new, 0, &build_remove_add(&[], &for_new));
        let announce = build_remove_add(&[], &[(new_id, new_init)]);
        for addr in others {
            self.send_command(addr, 0, &announce);
        }
    }

    /// Tell the remaining clients that a player has gone.
    fn remove_player_object(&mut self, id: u16) {
        let packet = build_remove_add(&[id], &[]);
        let ready: Vec<SocketAddr> = self
            .peers
            .iter()
            .filter(|(_, p)| p.ready)
            .map(|(a, _)| *a)
            .collect();
        for addr in ready {
            self.send_command(addr, 0, &packet);
        }
    }

    fn on_playerpos(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(pp) = parse_playerpos(payload) else {
            return;
        };
        let Some(peer) = self.peers.get(&from) else {
            return;
        };
        if !peer.ready {
            return;
        }
        let id = peer.id;
        let radius = (pp.wanted_range as i16).clamp(1, MAX_VIEW_RADIUS);
        let pos = pp.pos_nodes();
        let center = node_to_block(pos);
        if peer.view_center != Some(center) || peer.view_radius != radius {
            self.plan_blocks(from, pos, radius);
        }

        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        let moved = peer.pos_bs != pp.pos_bs || peer.speed_bs != pp.speed_bs || peer.yaw != pp.yaw;
        let active = moved || peer.pitch != pp.pitch || peer.keys != pp.keys;
        peer.keys = pp.keys;
        peer.pos_bs = pp.pos_bs;
        peer.speed_bs = pp.speed_bs;
        peer.yaw = pp.yaw;
        peer.pitch = pp.pitch;

        // Walk, mine or stand, as player_api does from the pressed keys.
        // Animation changes go reliably on channel 0: if one were lost the
        // player would be stuck walking (or frozen) until the next change.
        let anim = animation_for(&pp);
        if anim != peer.anim {
            peer.anim = anim;
            let packet = build_object_messages(&[(id, set_animation_message(anim))]);
            let others: Vec<SocketAddr> = self
                .peers
                .iter()
                .filter(|(a, p)| **a != from && p.ready)
                .map(|(a, _)| *a)
                .collect();
            for addr in others {
                self.send_command(addr, 0, &packet);
            }
        }
        if active {
            self.wake();
        }
        if !moved {
            return;
        }

        // Relay to the other players, unreliably on channel 1 like the
        // Luanti server: a lost update is soon replaced by the next.
        let msg = build_object_messages(&[(
            id,
            update_position_message(pp.pos_bs, pp.speed_bs, pp.yaw),
        )]);
        // Skipped while the radio is behind: the next update replaces it.
        if self.congested() {
            return;
        }
        let datagram = unreliable_datagram(1, &msg);
        let others: Vec<SocketAddr> = self.peers.iter().filter(|(a, p)| **a != from && p.ready).map(|(a, _)| *a).collect();
        for addr in others {
            self.send_raw(addr, &datagram);
        }
    }

    /// Digging and building. The client has already shown the change; we
    /// check it, apply it to the world, and tell everyone who has that map
    /// block. If we refuse, we send the real node back to undo the
    /// client's prediction.
    fn on_interact(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(msg) = parse_interact(payload) else {
            warn!("Malformed TOSERVER_INTERACT from {from}");
            hexdump(payload);
            return;
        };
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.ready {
            return;
        }
        if let Some(pp) = &msg.pos {
            peer.pos_bs = pp.pos_bs;
        }
        let name = peer.display_name().to_string();
        let body = [peer.pos_bs[0] / 10.0, peer.pos_bs[1] / 10.0 + 1.0, peer.pos_bs[2] / 10.0];
        let dead = peer.hp == 0;
        if msg.action == INTERACT_USE {
            if !dead {
                // An empty bucket scoops up the water it points at.
                let scooped = match msg.pointed {
                    Pointed::Node { under, above } => {
                        self.scoop_water(from, BucketUse { slot: msg.item as usize, under, above, body })
                    }
                    _ => false,
                };
                if !scooped {
                    self.use_item(from, msg.item as usize);
                }
            }
            return;
        }
        if let Pointed::Object(id) = msg.pointed {
            if msg.action == INTERACT_START_DIGGING && !dead {
                self.pick_up(from, id, body);
            }
            return;
        }
        let Pointed::Node { under, above } = msg.pointed else {
            return;
        };
        if dead && matches!(msg.action, INTERACT_DIGGING_COMPLETED | INTERACT_PLACE) {
            let target = if msg.action == INTERACT_PLACE { above } else { under };
            self.resend_node(from, target);
            return;
        }
        let in_reach = |p: [i16; 3]| {
            let d: f32 = (0..3).map(|i| (p[i] as f32 - body[i]).powi(2)).sum();
            d.sqrt() <= MAX_INTERACT_DISTANCE
        };
        let node_pos = |p: [i16; 3]| (p[0] as i32, p[1] as i32, p[2] as i32);

        match msg.action {
            INTERACT_DIGGING_COMPLETED => {
                let current = self.world.node(node_pos(under));
                if current == world::CONTENT_AIR || !in_reach(under) {
                    info!("{name}: refused dig at {under:?}");
                    self.resend_node(from, under);
                    return;
                }
                // What the wielded item can dig, falling back to the hand
                // (as Luanti's server checks the client's dig).
                let slot = msg.item as usize;
                let wielded = self.peers.get(&from).and_then(|p| p.inventory.get(slot)).filter(|_| slot < HOTBAR_SIZE);
                let caps = content::caps_of(wielded.map(|s| s.id));
                let wear = content::dig_params(current, caps, wielded.map_or(0, |s| s.wear))
                    .map(|(_, wear)| wear)
                    .or_else(|| content::dig_params(current, &content::HAND, 0).map(|_| 0));
                let Some(wear) = wear else {
                    info!("{name}: {} can't be dug with that", content::name_of(current));
                    self.resend_node(from, under);
                    return;
                };
                // minetest_game's can_dig: a furnace must be emptied first.
                let furnace_at = node_pos(under);
                if content::is_furnace(current) && self.furnaces.get(furnace_at).is_some_and(|f| !f.is_empty()) {
                    info!("{name}: the furnace at {under:?} isn't empty");
                    self.resend_node(from, under);
                    return;
                }
                // A chest must be emptied first, and isn't dug while open; a
                // locked one only by its owner.
                if content::is_chest(current) {
                    let record = self.chests.get(furnace_at);
                    let open = content::chest_pair(current).is_some_and(|(_, o)| o == current);
                    if open || record.is_some_and(|c| !c.is_empty() || !c.allows(&name)) {
                        info!("{name}: can't dig the chest at {under:?} (open, not empty, or not theirs)");
                        self.resend_node(from, under);
                        return;
                    }
                    self.close_chest_windows(furnace_at);
                    self.chests.remove(furnace_at);
                }
                // A steel door or trapdoor only by its owner.
                if doors::is_door(current) || doors::is_trapdoor(current) {
                    if !self.door_owners.allows(furnace_at, &name) {
                        info!("{name}: can't dig the locked door at {under:?}");
                        self.resend_node(from, under);
                        return;
                    }
                    self.door_owners.remove(furnace_at);
                }
                if doors::is_door(current) {
                    // Its hidden top goes with it (after_dig_node).
                    let top = [under[0], under[1] + 1, under[2]];
                    if self.world.node(node_pos(top)) == doors::HIDDEN {
                        self.world.set_node(node_pos(top), world::CONTENT_AIR, 0);
                        self.broadcast_node(top, &build_removenode(top));
                    }
                }
                if content::is_furnace(current) {
                    self.close_furnace(furnace_at);
                    if let Some(gone) = self.furnaces.remove(furnace_at) {
                        self.fade_sounds(&gone.sounds);
                    }
                }
                info!("{name} dug {} at {under:?}", content::name_of(current));
                // Wear out the tool (builtin's node_dig); a worn-out tool
                // breaks with its sound.
                if let Some(mut tool) = wielded.filter(|_| wear > 0) {
                    let intact = tool.add_wear(wear);
                    if let Some(peer) = self.peers.get_mut(&from) {
                        peer.inventory.set(slot, intact.then_some(tool));
                    }
                    if !intact {
                        info!("{name}'s {} broke", content::name_of(tool.id));
                        let at = under.map(|v| v as f32 * 10.0);
                        self.play_sound_near(-1, content::TOOL_BREAKS, 0.5, at, HEARING);
                    }
                    self.inventory_changed(from);
                }
                self.edit_event(&name, "dug", current);
                // A picked apple that grew on a tree leaves a mark where a
                // new one grows (minetest_game's after_dig_node).
                if content::is_apple(current) && self.world.param2(node_pos(under)) == 0 {
                    self.world.set_node(node_pos(under), content::CONTENT_APPLE_MARK, 0);
                    self.broadcast_node(under, &build_addnode(under, content::CONTENT_APPLE_MARK, 0));
                    let after = nature::grow_delay(&mut self.rng);
                    self.timers.start(node_pos(under), Job::Regrow, after);
                } else {
                    self.world.set_node(node_pos(under), world::CONTENT_AIR, 0);
                    self.broadcast_node(under, &build_removenode(under));
                }
                // Items resting on the dug node fall.
                self.wake_items(node_pos(under));
                // Torches hanging on the dug node, and saplings standing on
                // it, fall off and drop as items, as builtin's attached
                // nodes do.
                self.drop_attached(under);
                // Its leaves may now decay (minetest_game's leafdecay).
                self.start_leaf_decay(node_pos(under), current);
                // The drop goes into the digger's inventory; what doesn't
                // fit is dropped where the node was (builtin's
                // handle_node_drops). Leaves sometimes give a sapling.
                let drop = match content::drop_of(current) {
                    Some((leaf, 1)) if content::is_leaves(current) => Some((nature::leaf_drop(leaf, &mut self.rng), 1)),
                    other => other,
                };
                if let Some((item, count)) = drop {
                    let left = match self.peers.get_mut(&from) {
                        Some(peer) => peer.inventory.add(Stack::new(item, count)),
                        None => count,
                    };
                    if left > 0 {
                        info!("{name}'s inventory is full; dropping {left} {}", content::name_of(item));
                        self.spawn_item(Stack::new(item, left), (under.map(|v| v as f32), [0.0; 3]));
                    }
                    self.inventory_changed(from);
                }
            }
            INTERACT_PLACE => {
                // Right-clicking a furnace opens it, unless sneaking: then
                // the wielded item is placed against it (builtin's
                // item_place). The client doesn't predict either.
                let sneaking = msg.pos.as_ref().is_some_and(|p| p.keys & KEY_SNEAK != 0);
                if content::is_furnace(self.world.node(node_pos(under))) && !sneaking {
                    if in_reach(under) {
                        self.open_furnace(from, node_pos(under));
                    }
                    return;
                }
                if content::is_chest(self.world.node(node_pos(under))) && !sneaking {
                    if in_reach(under) {
                        self.open_chest(from, node_pos(under));
                    }
                    return;
                }
                // Doors and trapdoors open and close the same way.
                let target = self.world.node(node_pos(under));
                if (doors::is_door(target) || doors::is_trapdoor(target)) && !sneaking {
                    if in_reach(under) {
                        self.toggle_door(from, node_pos(under));
                    }
                    return;
                }
                // Where the player looks (the stairs and doors use it).
                let (pitch, yaw) = match (&msg.pos, self.peers.get(&from)) {
                    (Some(pp), _) => (pp.pitch, pp.yaw),
                    (None, Some(p)) => (p.pitch, p.yaw),
                    (None, None) => (0.0, 0.0),
                };
                // The client predicts placing the wielded item, if it is a
                // node. An empty slot predicts nothing.
                let slot = msg.item as usize;
                let wielded = self.peers.get(&from).and_then(|p| p.inventory.get(slot));
                // A water bucket pours out a water source.
                if wielded.is_some_and(|s| slot < HOTBAR_SIZE && s.id == content::ITEM_BUCKET_WATER) {
                    if in_reach(above) {
                        self.pour_water(from, BucketUse { slot, under, above, body });
                    }
                    return;
                }
                // A door item places a door (it isn't a node itself).
                if let Some(door) = wielded.filter(|_| slot < HOTBAR_SIZE).and_then(|s| doors::door_item(s.id)) {
                    let place = DoorPlace { under, above, look: look_dir(pitch, yaw), slot };
                    if in_reach(above) {
                        self.place_door(from, door, place);
                    }
                    return;
                }
                let Some(stack) = wielded.filter(|s| slot < HOTBAR_SIZE && content::is_node(s.id)) else {
                    // Nothing to place (an empty hand, or an item such as a
                    // stick that isn't a node). If the client thought
                    // otherwise, correct the node and the inventory.
                    self.resend_node(from, above);
                    self.send_inventory(from);
                    return;
                };
                let id = stack.id;
                // Air, water, or an apple mark (buildable_to: placing
                // replaces it).
                let target = self.world.node(node_pos(above));
                let free = content::buildable_to(target);
                let support = self.world.node(node_pos(under));
                // A slab on the open face of the same kind of slab makes a
                // full block where that slab was. The client predicted a
                // slab in the node beyond, so put that back too.
                if let Some(block) = stairs::merged(id, under, above, support, self.world.param2(node_pos(under))) {
                    if !in_reach(under) {
                        self.resend_node(from, above);
                        return;
                    }
                    info!("{name} made a double slab of {} at {under:?}", content::name_of(id));
                    self.edit_event(&name, "placed", block);
                    if let Some(peer) = self.peers.get_mut(&from) {
                        peer.inventory.take_one(slot);
                    }
                    self.inventory_changed(from);
                    self.world.set_node(node_pos(under), block, 0);
                    self.broadcast_node(under, &build_addnode(under, block, 0));
                    self.resend_node(from, above);
                    return;
                }
                // A torch hangs on something solid (the client checks the
                // same); anything else just needs something to build on.
                let torch = content::is_torch(id);
                // A sapling stands on the node below it, which must be solid
                // (builtin's check_attached_node, as the client checks).
                let below = (above[0] as i32, above[1] as i32 - 1, above[2] as i32);
                let supported = if torch {
                    content::walkable(support)
                } else if content::is_sapling(id) {
                    content::walkable(self.world.node(below))
                } else {
                    support != world::CONTENT_AIR
                };
                // A furnace faces the player who places it.
                // Torches and ladders face what they were placed against.
                let param2 = if content::is_wallmounted(id) {
                    content::wallmounted_param2(under, above)
                } else if content::is_furnace(id) || content::is_chest(id) || doors::is_trapdoor(id) {
                    content::facedir_param2(above, body)
                } else if stairs::shape_of(id).is_some() {
                    // Stairs and slabs: the way the player looks decides
                    // which way up they go (the stairs mod's on_place).
                    let placer = stairs::Placer { feet: [body[0], body[1] - 1.0, body[2]], pitch, yaw };
                    stairs::place_param2(id, under, above, support, self.world.param2(node_pos(under)), &placer)
                } else if content::is_leaves(id) || content::is_apple(id) {
                    // Placed leaves never decay (minetest_game's
                    // after_place_leaves), and placed apples don't regrow.
                    nature::PLACED_LEAVES
                } else {
                    0
                };
                // The item is always the floor torch; on a wall or ceiling
                // it becomes that torch node (minetest_game's on_place).
                let node = content::torch_variant(id, param2);
                if !free || !supported || !in_reach(above) {
                    info!("{name}: refused placing at {above:?}");
                    self.resend_node(from, above);
                    return;
                }
                info!("{name} placed {} at {above:?}", content::name_of(id));
                self.edit_event(&name, "placed", id);
                if let Some(peer) = self.peers.get_mut(&from) {
                    peer.inventory.take_one(slot);
                }
                self.inventory_changed(from);
                self.world.set_node(node_pos(above), node, param2);
                self.broadcast_node(above, &build_addnode(above, node, param2));
                if content::is_sapling(node) {
                    let after = nature::grow_delay(&mut self.rng);
                    self.timers.start(node_pos(above), Job::Grow, after);
                }
                // A locked chest belongs to whoever placed it; so does a
                // steel trapdoor.
                if content::is_locked_chest(node) {
                    self.chests.get_or_add(node_pos(above)).owner = Some(name.clone());
                    self.chests.save();
                }
                if doors::is_locked(node) {
                    self.door_owners.set(node_pos(above), &name);
                }
            }
            // Starting and stopping digging need nothing from us; the mining
            // animation follows the dig key in TOSERVER_PLAYERPOS.
            _ => {}
        }
    }

    /// Right-clicking a door or trapdoor: open it if it's shut, shut it if
    /// it's open (doors.door_toggle, trapdoor_toggle), with its sound. A
    /// steel one only for its owner.
    fn toggle_door(&mut self, from: SocketAddr, pos: NodePos) {
        let name = self.peers.get(&from).map(|p| p.display_name().to_string()).unwrap_or_default();
        if !self.door_owners.allows(pos, &name) {
            let owner = self.door_owners.owner(pos).unwrap_or_default().to_string();
            self.tell(from, &format!("This door is locked; it belongs to {owner}."));
            return;
        }
        let (node, param2) = (self.world.node(pos), self.world.param2(pos));
        // A door changes node and turn; a trapdoor only node.
        let toggled = match (doors::door_of(node), doors::trapdoor_of(node)) {
            (Some((door, _)), _) => doors::toggle(node, param2).map(|(to, p2, opened)| (to, p2, opened, door.open, door.close)),
            (_, Some((trap, _))) => doors::trapdoor_toggle(node).map(|(to, opened)| (to, param2, opened, trap.open, trap.close)),
            _ => None,
        };
        let Some((to, to_param2, opened, open_sound, close_sound)) = toggled else {
            return;
        };
        let p16 = [pos.0 as i16, pos.1 as i16, pos.2 as i16];
        self.world.set_node(pos, to, to_param2);
        self.broadcast_node(p16, &build_addnode(p16, to, to_param2));
        let (sound, gain) = if opened { open_sound } else { close_sound };
        self.play_sound_near(-1, sound, gain, p16.map(|v| v as f32 * 10.0), DOOR_HEARING);
    }

    /// Placing a door (the door item's on_place): in the node pointed at
    /// (or into an apple mark), with room above for its top, facing the
    /// way the player looks, and hinged to pair with a door beside it.
    fn place_door(&mut self, from: SocketAddr, door: &'static Door, at: DoorPlace) {
        let name = self.peers.get(&from).map(|p| p.display_name().to_string()).unwrap_or_default();
        let node_pos = |p: [i16; 3]| (p[0] as i32, p[1] as i32, p[2] as i32);
        let free = content::buildable_to;
        let pos = if free(self.world.node(node_pos(at.under))) { at.under } else { at.above };
        let top = [pos[0], pos[1] + 1, pos[2]];
        if !free(self.world.node(node_pos(pos))) || !free(self.world.node(node_pos(top))) {
            info!("{name}: no room for a door at {pos:?}");
            return;
        }
        let dir = doors::facedir_of(at.look);
        // minetest_game looks for anything in group "door": trapdoors too.
        let next_to_door = content::group_of(self.world.node(doors::beside(node_pos(pos), dir)), "door") > 0;
        let (node, param2, top_param2) = doors::placed(door, dir, next_to_door);
        info!("{name} placed {} at {pos:?}", content::name_of(door.item));
        self.edit_event(&name, "placed", door.item);
        if let Some(peer) = self.peers.get_mut(&from) {
            peer.inventory.take_one(at.slot);
        }
        self.inventory_changed(from);
        self.world.set_node(node_pos(pos), node, param2);
        self.broadcast_node(pos, &build_addnode(pos, node, param2));
        self.world.set_node(node_pos(top), doors::HIDDEN, top_param2);
        self.broadcast_node(top, &build_addnode(top, doors::HIDDEN, top_param2));
        if door.locked {
            self.door_owners.set(node_pos(pos), &name);
        }
        let (sound, gain) = content::place_sound(node);
        self.play_sound_near(-1, sound, gain, pos.map(|v| v as f32 * 10.0), HEARING);
    }

    /// Remove what was attached to the node at `pos` (just removed):
    /// torches hanging on it, and a sapling standing on it. They drop as
    /// items where they were.
    fn drop_attached(&mut self, pos: [i16; 3]) {
        for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let n = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
            let np = (n[0] as i32, n[1] as i32, n[2] as i32);
            let id = self.world.node(np);
            let attached = if content::is_torch(id) {
                let s = content::wallmounted_support(self.world.param2(np));
                (np.0 + s.0, np.1 + s.1, np.2 + s.2) == (pos[0] as i32, pos[1] as i32, pos[2] as i32)
            } else {
                content::is_sapling(id) && dy == 1
            };
            if !attached {
                continue;
            }
            info!("{} at {n:?} lost its support", content::name_of(id));
            self.world.set_node(np, world::CONTENT_AIR, 0);
            self.broadcast_node(n, &build_removenode(n));
            if let Some((item, count)) = content::drop_of(id) {
                self.spawn_item(Stack::new(item, count), (n.map(|v| v as f32), [0.0; 3]));
            }
        }
    }

    /// A trunk was dug at `pos`: its leaves nearby get decay timers.
    fn start_leaf_decay(&mut self, pos: NodePos, trunk: u16) {
        let world = &self.world;
        let node = |p: NodePos| (world.node(p), world.param2(p));
        for leaf in nature::leaves_to_check(pos, trunk, &node) {
            let after = nature::decay_delay(&mut self.rng);
            self.timers.start(leaf, Job::Decay, after);
        }
    }

    /// Run the leaf decay and sapling timers that are due (a few per tick).
    fn run_timers(&mut self) {
        let now = Instant::now();
        for _ in 0..TIMERS_PER_TICK {
            let Some((pos, job)) = self.timers.pop_due(now) else {
                return;
            };
            match job {
                Job::Decay => self.decay_leaf(pos),
                Job::Grow => self.grow_sapling(pos),
                Job::Regrow => self.regrow_apple(pos),
            }
        }
    }

    /// minetest_game's leafdecay_on_timer: a leaf with no trunk of its tree
    /// near it goes, dropping a sapling now and then (never the leaf).
    fn decay_leaf(&mut self, pos: NodePos) {
        let world = &self.world;
        let node = |p: NodePos| (world.node(p), world.param2(p));
        if !nature::should_decay(pos, &node) {
            return;
        }
        let leaf = self.world.node(pos);
        let p16 = [pos.0 as i16, pos.1 as i16, pos.2 as i16];
        self.world.set_node(pos, world::CONTENT_AIR, 0);
        self.broadcast_node(p16, &build_removenode(p16));
        self.wake_items(pos);
        self.drop_attached(p16);
        if let Some(drop) = nature::decay_drop(leaf, &mut self.rng) {
            let r = |rng: &mut Rng| rng.range(0, 99) as f32 / 100.0 - 0.5;
            let at = [pos.0 as f32 + r(&mut self.rng), pos.1 as f32 + r(&mut self.rng), pos.2 as f32 + r(&mut self.rng)];
            self.spawn_item(Stack::new(drop, 1), (at, [0.0; 3]));
        }
    }

    /// minetest_game's apple_mark timer: an apple grows again if apple
    /// leaves are still next to the mark and it is light; in the dark it
    /// waits; with no leaves left the mark goes.
    fn regrow_apple(&mut self, pos: NodePos) {
        if !content::is_mark(self.world.node(pos)) {
            return; // something was placed there
        }
        let daylight = (6000..18000).contains(&self.time_of_day());
        let world = &self.world;
        let node = |p: NodePos| world.node(p);
        let p16 = [pos.0 as i16, pos.1 as i16, pos.2 as i16];
        match nature::regrow(pos, &node, daylight) {
            nature::Regrow::Gone => {
                self.world.set_node(pos, world::CONTENT_AIR, 0);
                self.broadcast_node(p16, &build_removenode(p16));
            }
            nature::Regrow::Wait => self.timers.start(pos, Job::Regrow, nature::REGROW_RETRY),
            nature::Regrow::Apple => {
                self.world.set_node(pos, content::CONTENT_APPLE, 0);
                self.broadcast_node(p16, &build_addnode(p16, content::CONTENT_APPLE, 0));
            }
        }
    }

    /// Left-click with a usable item: eating an apple restores 2 HP
    /// (minetest_game's item_eat(2): one is used up even at full health).
    /// An empty bucket used on a water source (the bucket mod's on_use):
    /// the water goes into the bucket, leaving air. With a stack of empty
    /// buckets, the full one goes into the inventory (or is dropped if it
    /// doesn't fit). Returns whether the wielded item was an empty bucket.
    fn scoop_water(&mut self, addr: SocketAddr, at: BucketUse) -> bool {
        let BucketUse { slot, under, body, .. } = at;
        let Some(peer) = self.peers.get(&addr) else {
            return false;
        };
        let Some(bucket) = peer.inventory.get(slot).filter(|s| slot < HOTBAR_SIZE && s.id == content::ITEM_BUCKET_EMPTY) else {
            return false;
        };
        let name = peer.display_name().to_string();
        let pos = (under[0] as i32, under[1] as i32, under[2] as i32);
        let near = (0..3).map(|i| (under[i] as f32 - body[i]).powi(2)).sum::<f32>().sqrt() <= MAX_INTERACT_DISTANCE;
        if self.world.node(pos) != content::CONTENT_WATER || !near {
            return true;
        }
        info!("{name} scooped up water at {under:?}");
        self.world.set_node(pos, world::CONTENT_AIR, 0);
        self.broadcast_node(under, &build_removenode(under));
        let full = Stack::new(content::ITEM_BUCKET_WATER, 1);
        let mut dropped = false;
        if let Some(peer) = self.peers.get_mut(&addr) {
            if bucket.count == 1 {
                peer.inventory.set(slot, Some(full));
            } else {
                peer.inventory.take_one(slot);
                dropped = peer.inventory.add(full) > 0;
            }
        }
        if dropped {
            self.spawn_item(full, (body, [0.0; 3]));
        }
        self.wake_items(pos);
        self.inventory_changed(addr);
        true
    }

    /// A water bucket placed (the bucket mod's on_place): a water source
    /// in the node pointed at if it can be built into (such as water),
    /// otherwise in the one in front of it, which must be. The bucket is
    /// left empty.
    fn pour_water(&mut self, addr: SocketAddr, at: BucketUse) {
        let BucketUse { slot, under, above, .. } = at;
        let node_pos = |p: [i16; 3]| (p[0] as i32, p[1] as i32, p[2] as i32);
        let pos = if content::buildable_to(self.world.node(node_pos(under))) { under } else { above };
        if !content::buildable_to(self.world.node(node_pos(pos))) {
            self.resend_node(addr, pos);
            return;
        }
        let name = self.peers.get(&addr).map(|p| p.display_name().to_string()).unwrap_or_default();
        info!("{name} poured water at {pos:?}");
        self.edit_event(&name, "placed", content::CONTENT_WATER);
        self.world.set_node(node_pos(pos), content::CONTENT_WATER, 0);
        self.broadcast_node(pos, &build_addnode(pos, content::CONTENT_WATER, 0));
        if let Some(peer) = self.peers.get_mut(&addr) {
            peer.inventory.set(slot, Some(Stack::new(content::ITEM_BUCKET_EMPTY, 1)));
        }
        self.inventory_changed(addr);
    }

    fn use_item(&mut self, addr: SocketAddr, slot: usize) {
        let Some(peer) = self.peers.get_mut(&addr) else {
            return;
        };
        let wielded = peer.inventory.get(slot).filter(|_| slot < HOTBAR_SIZE);
        if !wielded.is_some_and(|s| content::is_apple(s.id)) {
            return;
        }
        peer.inventory.take_one(slot);
        peer.hp = (peer.hp + content::APPLE_HP).min(PLAYER_MAX_HP);
        let (hp, name) = (peer.hp, peer.display_name().to_string());
        peer.save(&self.players);
        info!("{name} ate an apple: {hp} HP");
        self.send_command(addr, 0, &build_hp(hp, false));
        self.send_command(addr, 0, &build_health_bar_change(hp));
        self.inventory_changed(addr);
    }

    /// minetest_game's grow_sapling: on soil and in daylight it becomes a
    /// tree; otherwise it tries again in 5 minutes.
    fn grow_sapling(&mut self, pos: NodePos) {
        let sapling = self.world.node(pos);
        if !content::is_sapling(sapling) {
            return; // dug meanwhile
        }
        let daytime = (6000..18000).contains(&self.time_of_day());
        let world = &self.world;
        let node = |p: NodePos| world.node(p);
        let sky = world.highest_solid(pos.0, pos.2) < pos.1;
        if !nature::can_grow(pos, &node, sky, daytime) {
            self.timers.start(pos, Job::Grow, nature::GROW_RETRY);
            return;
        }
        let height = nature::tree_height(sapling, &mut self.rng);
        let tree = nature::grow(pos, sapling, height, &node);
        info!("The {} at {pos:?} grew into a tree ({} nodes)", content::name_of(sapling), tree.len());
        for (p, id) in tree {
            let p16 = [p.0 as i16, p.1 as i16, p.2 as i16];
            self.world.set_node(p, id, 0);
            self.broadcast_node(p16, &build_addnode(p16, id, 0));
        }
    }

    /// A line in a player's chat area.
    fn tell(&mut self, addr: SocketAddr, text: &str) {
        self.send_command(addr, 0, &build_chat_message(CHAT_RAW, "", text));
    }

    /// A line in everyone's chat area.
    fn tell_all(&mut self, text: &str) {
        info!("CHAT: {text}");
        self.send_to_players(&build_chat_message(CHAT_RAW, "", text));
    }

    /// Something a player typed in chat: a line for everyone, or a command.
    /// As Luanti does: trimmed, at most CHAT_MAX_CHARS characters, one
    /// line, and at most CHAT_MAX_PER_10S messages in ten seconds.
    fn on_chat(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(text) = parse_chat_message(payload) else {
            return;
        };
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.ready {
            return;
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let now = Instant::now();
        while peer.chat_times.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(10)) {
            peer.chat_times.pop_front();
        }
        let name = peer.display_name().to_string();
        if peer.chat_times.len() >= CHAT_MAX_PER_10S {
            let reply = format!("You cannot send more messages. You are limited to {CHAT_MAX_PER_10S} messages per 10 seconds.");
            self.tell(from, &reply);
            return;
        }
        peer.chat_times.push_back(now);
        if text.chars().count() > CHAT_MAX_CHARS {
            self.tell(from, "Your message exceeds the maximum chat message length. It was refused. Send a shorter message.");
            return;
        }
        if text.contains(['\r', '\n']) {
            self.tell(from, "Newlines are not permitted in chat messages");
            return;
        }
        if let Some(command) = text.strip_prefix('/') {
            self.chat_command(from, &name, command);
            return;
        }
        self.tell_all(&format!("<{name}> {text}"));
        self.event(format!("{name}: {text}"));
        status::lock(&self.status).chat_line(status::ChatLine { name, text, action: false });
    }

    /// A chat command, as Luanti's built-in ones word their replies.
    fn chat_command(&mut self, from: SocketAddr, name: &str, command: &str) {
        let (cmd, param) = command.split_once(' ').unwrap_or((command, ""));
        let param = param.trim();
        match cmd {
            "" => self.tell(from, "-!- Empty command."),
            "help" => self.tell(from, CHAT_HELP),
            "me" => {
                if param.is_empty() {
                    self.tell(from, "-!- Invalid command usage: /me <action>");
                } else {
                    self.tell_all(&format!("* {name} {param}"));
                    status::lock(&self.status).chat_line(status::ChatLine {
                        name: name.to_string(),
                        text: param.to_string(),
                        action: true,
                    });
                }
            }
            "msg" => {
                let Some((to, message)) = param.split_once(' ').map(|(t, m)| (t, m.trim())).filter(|(_, m)| !m.is_empty()) else {
                    self.tell(from, "Invalid usage, see /help.");
                    return;
                };
                let target = self
                    .peers
                    .iter()
                    .find(|(_, p)| p.ready && p.display_name().eq_ignore_ascii_case(to))
                    .map(|(a, p)| (*a, p.display_name().to_string()));
                match target {
                    Some((addr, to_name)) => {
                        info!("DM from {name} to {to_name}: {message}");
                        self.tell(addr, &format!("DM from {name}: {message}"));
                        self.tell(from, "Message sent.");
                    }
                    None => self.tell(from, &format!("The player {to} is not online.")),
                }
            }
            "status" => {
                let line = self.status_line();
                self.tell(from, &line);
            }
            "time" => {
                let line = format!("Current time is {}.", self.clock.hh_mm());
                self.tell(from, &line);
            }
            other => self.tell(from, &format!("-!- Invalid command: {other}. Type /help for a list of commands.")),
        }
    }

    /// Like Luanti's status line: version, uptime, time of day, players.
    fn status_line(&self) -> String {
        let up = status::lock(&self.status).started.elapsed().as_secs();
        let players: Vec<&str> = self.peers.values().filter(|p| p.ready).map(|p| p.display_name()).collect();
        format!(
            "# Server: ESP32-S3 Luanti server, uptime: {}h {:02}m, time of day: {}, players ({}/{MAX_PLAYERS}): {}",
            up / 3600,
            up / 60 % 60,
            self.clock.hh_mm(),
            players.len(),
            players.join(", ")
        )
    }

    /// The game's time of day in Luanti's units (0..24000, 6000 = 06:00),
    /// carried on from before a restart (clock.rs).
    fn time_of_day(&self) -> u16 {
        self.clock.time_of_day()
    }

    /// The welcome message, and who else is here, in the chat area.
    fn send_welcome(&mut self, addr: SocketAddr) {
        let mut lines = self.welcome.clone();
        let others: Vec<&str> = self
            .peers
            .iter()
            .filter(|(a, p)| **a != addr && p.ready)
            .map(|(_, p)| p.display_name())
            .collect();
        lines.push(if others.is_empty() {
            "Nobody else is playing right now.".to_string()
        } else {
            format!("Also playing: {}", others.join(", "))
        });
        for line in lines {
            self.send_command(addr, 0, &build_chat_message(CHAT_SYSTEM, "", &line));
        }
    }

    /// Whether the game is full for `name`. A player reconnecting (whose old
    /// connection is still counted until it is replaced) is let in.
    fn user_limit_reached(&self, name: &str) -> bool {
        let others = self
            .peers
            .values()
            .filter(|p| p.ready && !p.display_name().eq_ignore_ascii_case(name))
            .count();
        others >= MAX_PLAYERS
    }

    /// The texture for a skin key.
    fn skin_texture(&self, key: &str) -> String {
        self.skins.get(self.skins.index_of(key)).texture.clone()
    }

    /// The inventory window, with the skin chooser showing this player's
    /// skin. While the window is open the client redraws it when this
    /// changes.
    fn send_inventory_formspec(&mut self, addr: SocketAddr) {
        let Some(peer) = self.peers.get(&addr) else {
            return;
        };
        let skin = self.skins.get(self.skins.index_of(&peer.skin));
        let packet = build_inventory_formspec(&SkinChoice { label: &skin.label, texture: &skin.texture });
        self.send_command(addr, 0, &packet);
    }

    /// Buttons in the inventory window: the skin chooser's < and >.
    fn on_inventory_fields(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some((form, fields)) = parse_inventory_fields(payload) else {
            return;
        };
        let pressed = |name: &str| fields.iter().any(|(f, _)| f == name);
        if form == CHEST_FORM {
            if pressed("quit") {
                self.close_chest_for(from);
            }
            return;
        }
        if form == FURNACE_FORM {
            // The furnace window was closed.
            if pressed("quit") {
                let Some(peer) = self.peers.get_mut(&from) else {
                    return;
                };
                peer.furnace = None;
                // An update sent just before the window closed would open
                // it again: close that too (an empty formspec closes the
                // window only if it is ours, so this is safe).
                if peer.furnace_form_sent.is_some_and(|t| t.elapsed() < Duration::from_millis(1500)) {
                    self.send_command(from, 0, &build_show_formspec("", FURNACE_FORM));
                }
            }
            return;
        }
        let forward = match (pressed(FIELD_SKIN_PREVIOUS), pressed(FIELD_SKIN_NEXT)) {
            (true, false) => false,
            (false, true) => true,
            _ => return, // closing the window, or someone else's form
        };
        if !form.is_empty() {
            return;
        }
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.ready {
            return;
        }
        let next = self.skins.step(self.skins.index_of(&peer.skin), forward);
        let skin = self.skins.get(next);
        peer.skin = skin.key.clone();
        peer.save(&self.players);
        let (id, name, texture, label) = (peer.id, peer.display_name().to_string(), skin.texture.clone(), skin.label.clone());
        info!("{name} now wears the {label} skin");
        self.send_inventory_formspec(from);
        // Everyone (the player too, for the third-person view) redraws
        // the model with the new texture.
        let packet = build_object_messages(&[(id, player_properties_message(&texture))]);
        self.send_to_players(&packet);
    }

    /// Play a sound at a position (BS units) for everyone within `range`
    /// nodes. `id` -1 for a one-off sound; otherwise one that can be faded.
    fn play_sound_near(&mut self, id: i32, sound: &str, gain: f32, pos_bs: [f32; 3], range: f32) {
        let packet = build_play_sound(id, sound, gain, pos_bs);
        let reach = range * 10.0;
        let near: Vec<SocketAddr> = self
            .peers
            .iter()
            .filter(|(_, p)| p.ready && (0..3).map(|i| (p.pos_bs[i] - pos_bs[i]).powi(2)).sum::<f32>() <= reach * reach)
            .map(|(a, _)| *a)
            .collect();
        for addr in near {
            self.send_command(addr, 0, &packet);
        }
    }

    /// Fade sounds out for everyone (those who never heard them ignore it).
    fn fade_sounds(&mut self, ids: &[i32]) {
        for &id in ids {
            self.send_to_players(&build_fade_sound(id, -1.0, 0.0));
        }
    }

    /// Put an item stack into the world and show it to everyone.
    /// `motion` is position and velocity, in nodes.
    fn spawn_item(&mut self, stack: Stack, motion: ([f32; 3], [f32; 3])) {
        let world = &self.world;
        let solid = |x: i32, y: i32, z: i32| content::walkable(world.node((x, y, z)));
        let (id, evicted) = self.items.add(stack, motion, &solid);
        let Some(d) = self.items.get(id) else {
            return;
        };
        let added = [(id, item_init(d))];
        let removed: Vec<u16> = evicted.into_iter().collect();
        self.send_to_players(&build_remove_add(&removed, &added));
    }

    /// Punching a dropped item: into the inventory, as much as fits.
    fn pick_up(&mut self, from: SocketAddr, id: u16, body: [f32; 3]) {
        let Some(d) = self.items.get(id).cloned() else {
            return; // a player, or an item that has just gone
        };
        let dist: f32 = (0..3).map(|i| (d.pos[i] - body[i]).powi(2)).sum::<f32>().sqrt();
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if dist > MAX_INTERACT_DISTANCE {
            return;
        }
        let left = peer.inventory.add(d.stack());
        let name = peer.display_name().to_string();
        if left == d.count {
            return; // inventory full
        }
        info!("{name} picked up {} {}", d.count - left, content::name_of(d.item));
        if left == 0 {
            self.items.remove(id);
            self.send_to_players(&build_remove_add(&[id], &[]));
        } else {
            self.items.set_count(id, left);
            self.send_item_properties(id);
        }
        self.inventory_changed(from);
    }

    /// The node at `node` changed: items on it may fall.
    fn wake_items(&mut self, node: (i32, i32, i32)) {
        for id in self.items.wake_near([node.0, node.1, node.2]) {
            self.send_item_position(id);
        }
    }

    fn step_items(&mut self, dt: f32) {
        let world = &self.world;
        let solid = |x: i32, y: i32, z: i32| content::walkable(world.node((x, y, z)));
        for change in self.items.step(dt, &solid) {
            match change {
                Change::Moved(id) => self.send_item_position(id),
                Change::Resized(id) => self.send_item_properties(id),
                Change::Removed(id) => self.send_to_players(&build_remove_add(&[id], &[])),
            }
        }
    }

    fn send_item_position(&mut self, id: u16) {
        if let Some(d) = self.items.get(id) {
            let msg = build_object_messages(&[(id, item_position_message(d.pos_bs(), d.vel_bs()))]);
            self.send_to_players(&msg);
        }
    }

    fn send_item_properties(&mut self, id: u16) {
        if let Some(d) = self.items.get(id) {
            let init = item_init_of(d);
            let msg = build_object_messages(&[(id, item_properties_message(&init.as_init(id)))]);
            self.send_to_players(&msg);
        }
    }

    /// Send a command reliably to every player in the game.
    fn send_to_players(&mut self, packet: &[u8]) {
        let ready: Vec<SocketAddr> = self.peers.iter().filter(|(_, p)| p.ready).map(|(a, _)| *a).collect();
        for addr in ready {
            self.send_command(addr, 0, packet);
        }
    }

    /// Where a new (or respawning) player appears: just above the ground
    /// near node (8, ?, 8), a few nodes apart so players don't arrive inside
    /// each other. A position is the player's feet; a node's top face is 0.5
    /// above its centre.
    fn spawn_point(&self, peer_id: u16) -> [f32; 3] {
        let slot = (peer_id as i32 - 2).rem_euclid(4);
        let (sx, sz) = (8 + slot * 3, 8);
        let ground = self.world.highest_solid(sx, sz);
        [sx as f32, ground as f32 + 0.6, sz as f32]
    }

    /// A saved position, moved up onto the ground if it is now inside solid
    /// nodes (the world may have changed since).
    fn safe_position(&self, pos: [f32; 3]) -> [f32; 3] {
        let (x, y, z) = (pos[0].round() as i32, pos[1].round() as i32, pos[2].round() as i32);
        let free = |y: i32| !content::walkable(self.world.node((x, y, z)));
        if free(y) && free(y + 1) {
            pos
        } else {
            [pos[0], self.world.highest_solid(x, z) as f32 + 0.6, pos[2]]
        }
    }

    /// Fall damage, worked out by the client.
    fn on_damage(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(damage) = parse_damage(payload) else {
            return;
        };
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.ready || peer.hp == 0 || damage == 0 {
            return;
        }
        peer.hp = peer.hp.saturating_sub(damage);
        let (hp, name) = (peer.hp, peer.display_name().to_string());
        info!("{name} took {damage} fall damage, {hp} HP left");
        self.send_command(from, 0, &build_hp(hp, true));
        self.send_command(from, 0, &build_health_bar_change(hp));
        if hp == 0 {
            info!("{name} died");
            self.event(format!("{name} fell to their death"));
            self.send_command(from, 0, &build_deathscreen());
            if let Some(peer) = self.peers.get_mut(&from) {
                peer.save(&self.players);
            }
        }
    }

    /// The Respawn button on the death screen.
    fn on_respawn(&mut self, from: SocketAddr) {
        let Some(peer) = self.peers.get(&from) else {
            return;
        };
        if !peer.ready || peer.hp != 0 {
            return; // only the dead respawn
        }
        let spawn = self.spawn_point(peer.id);
        let name = peer.display_name().to_string();
        let spawn_bs = spawn.map(|v| v * 10.0);
        if let Some(peer) = self.peers.get_mut(&from) {
            peer.hp = PLAYER_MAX_HP;
            peer.pos_bs = spawn_bs;
            peer.save(&self.players);
        }
        info!("{name} respawns at ({:.1}, {:.1}, {:.1})", spawn[0], spawn[1], spawn[2]);
        self.send_command(from, 0, &build_hp(PLAYER_MAX_HP, false));
        self.send_command(from, 0, &build_health_bar_change(PLAYER_MAX_HP));
        self.send_command(from, 0, &build_move_player(spawn_bs, 0.0, 0.0));
        self.plan_blocks(from, spawn, MAX_VIEW_RADIUS);
    }

    /// Rearranging items in the inventory window. The client has already
    /// shown the move; whatever we decide, we send the real inventory back.
    fn on_inventory_action(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(peer) = self.peers.get_mut(&from) else {
            return;
        };
        if !peer.ready {
            return;
        }
        let text = String::from_utf8_lossy(&payload[2..]).into_owned();
        let name = peer.display_name().to_string();
        let open_furnace = peer.furnace;
        let open_chest = peer.chest;
        let throw = drop_motion(peer.pos_bs, peer.yaw, peer.pitch);
        // Which list an action names: the player's own (not "craftpreview":
        // taking from it is a Craft action, and nothing can be put into it,
        // as Luanti's serverpackethandler has it), or a list of the furnace
        // they have open.
        let furnaces = &self.furnaces;
        let chests = &self.chests;
        let place = |loc: &str, list: &str| -> Option<Place> {
            if inventory::is_own(loc, &name) {
                return List::from_name(list).map(Place::Own);
            }
            if let Some(pos) = chest::pos_of_location(loc) {
                let ok = open_chest == Some(pos) && chests.get(pos).is_some() && list == "main";
                return ok.then_some(Place::Chest(pos));
            }
            let pos = furnace::pos_of_location(loc).filter(|&p| open_furnace == Some(p) && furnaces.get(p).is_some())?;
            FurnaceList::from_name(list).map(|l| Place::Furnace(pos, l))
        };
        let action = inventory::parse_action(&text);
        let places = match &action {
            Some(Action::Move { from: (fi, fl, _), to: (ti, tl, _), .. }) => (place(fi, fl), place(ti, tl)),
            Some(Action::MoveSomewhere { from: (fi, fl, _), to: (ti, tl), .. }) => (place(fi, fl), place(ti, tl)),
            Some(Action::Drop { from: (fi, fl, _), .. }) => (place(fi, fl), None),
            Some(Action::Craft { inv, .. }) => (inventory::is_own(inv, &name).then_some(Place::Own(List::Craft)), None),
            _ => (None, None),
        };
        let Self { peers, furnaces, chests, .. } = self;
        let Some(peer) = peers.get_mut(&from) else {
            return;
        };
        let mut lists = Lists { player: &mut peer.inventory, furnaces, chests };
        let mut dropped = None;
        let ok = match (action, places) {
            (Some(Action::Move { count, from: (_, _, fs), to: (_, _, ts) }), (Some(f), Some(t))) => {
                lists.move_items((f, fs), (t, ts), count)
            }
            (Some(Action::MoveSomewhere { count, from: (_, _, fs), .. }), (Some(f), Some(t))) => {
                lists.move_somewhere((f, fs), t, count)
            }
            (Some(Action::Craft { count, .. }), (Some(_), _)) => match lists.player.craft(count) {
                Some(made) => {
                    info!("{name} crafted {}", made.item_string());
                    true
                }
                None => false,
            },
            (Some(Action::Drop { count, from: (_, _, fs), .. }), (Some(f), _)) => {
                let stack = lists.take((f, fs), count);
                if let Some(stack) = stack {
                    info!("{name} dropped {}", stack.item_string());
                    dropped = Some((stack, throw));
                }
                stack.is_some()
            }
            (Some(Action::Other(kind)), _) => {
                info!("{name}: inventory action {kind} isn't supported");
                false
            }
            _ => {
                info!("{name}: refused inventory action \"{text}\"");
                false
            }
        };
        let furnace_used = [places.0, places.1].into_iter().flatten().find_map(|p| match p {
            Place::Furnace(pos, _) => Some(pos),
            _ => None,
        });
        let chest_used = [places.0, places.1].into_iter().flatten().find_map(|p| match p {
            Place::Chest(pos) => Some(pos),
            _ => None,
        });
        if let Some(pos) = chest_used {
            if ok {
                self.chests.save();
            }
            self.send_chest(pos);
        }
        if let Some((stack, motion)) = dropped {
            self.spawn_item(stack, motion);
        }
        if ok {
            self.inventory_changed(from);
        } else {
            self.send_inventory(from); // undo the client's prediction
        }
        // The client doesn't predict moves into or out of a detached
        // inventory, so it always needs the furnace's contents back.
        if let Some(pos) = furnace_used {
            if ok {
                // minetest_game starts the furnace's timer on any change.
                if let Some(f) = self.furnaces.get_mut(pos) {
                    f.running = true;
                }
                self.furnaces.save();
            }
            self.send_furnace(pos);
        }
    }

    /// Open a chest for a player (minetest_game's on_rightclick): a locked
    /// chest only for its owner. The open sound plays and, unless a block
    /// sits on it, the lid goes up while anyone has it open.
    fn open_chest(&mut self, addr: SocketAddr, pos: NodePos) {
        let node = self.world.node(pos);
        let Some((closed, open)) = content::chest_pair(node) else {
            return;
        };
        let Some(name) = self.peers.get(&addr).map(|p| p.display_name().to_string()) else {
            return;
        };
        let record = self.chests.get_or_add(pos);
        if !record.allows(&name) {
            let owner = record.owner.clone().unwrap_or_default();
            self.tell(addr, &format!("This chest is locked; it belongs to {owner}."));
            return;
        }
        // One chest at a time: a chest they had open closes.
        if let Some(old) = self.peers.get_mut(&addr).and_then(|p| p.chest.take()) {
            self.close_unwatched_lids_at(old);
        }
        if let Some(peer) = self.peers.get_mut(&addr) {
            peer.chest = Some(pos);
        }
        info!("{name} opened the chest at {pos:?}");
        let at = [pos.0 as f32 * 10.0, pos.1 as f32 * 10.0, pos.2 as f32 * 10.0];
        self.play_sound_near(-1, "default_chest_open", 0.3, at, CHEST_HEARING);
        let above = self.world.node((pos.0, pos.1 + 1, pos.2));
        if node == closed && !content::lid_obstructed(above) {
            let param2 = self.world.param2(pos);
            self.swap_node(pos, open, param2);
            self.open_chests.insert(pos);
        }
        if let Some(packet) = self.chest_packet(pos) {
            self.send_command(addr, 0, &packet);
        }
        let form = chest_formspec(&chest::detached_name(pos));
        self.send_command(addr, 0, &build_show_formspec(&form, CHEST_FORM));
    }

    /// A player closed their chest window.
    fn close_chest_for(&mut self, addr: SocketAddr) {
        if let Some(pos) = self.peers.get_mut(&addr).and_then(|p| p.chest.take()) {
            self.close_unwatched_lids_at(pos);
        }
    }

    /// Lower the lid of the chest at `pos` if nobody has it open any more,
    /// with the closing sound (minetest_game's chest_lid_close).
    fn close_unwatched_lids_at(&mut self, pos: NodePos) {
        if self.peers.values().any(|p| p.chest == Some(pos)) || !self.open_chests.remove(&pos) {
            return;
        }
        let node = self.world.node(pos);
        if let Some((closed, open)) = content::chest_pair(node) {
            if node == open {
                let param2 = self.world.param2(pos);
                self.swap_node(pos, closed, param2);
                let at = [pos.0 as f32 * 10.0, pos.1 as f32 * 10.0, pos.2 as f32 * 10.0];
                self.play_sound_near(-1, "default_chest_close", 0.3, at, CHEST_HEARING);
            }
        }
    }

    /// Players left: lower the lids nobody is watching any more.
    fn close_unwatched_lids(&mut self) {
        let open: Vec<NodePos> = self.open_chests.iter().copied().collect();
        for pos in open {
            self.close_unwatched_lids_at(pos);
        }
    }

    /// Close a chest's window for everyone who has it open (it was dug).
    fn close_chest_windows(&mut self, pos: NodePos) {
        let viewers: Vec<SocketAddr> = self.peers.iter().filter(|(_, p)| p.chest == Some(pos)).map(|(a, _)| *a).collect();
        for addr in viewers {
            if let Some(peer) = self.peers.get_mut(&addr) {
                peer.chest = None;
            }
            self.send_command(addr, 0, &build_show_formspec("", CHEST_FORM));
        }
        self.open_chests.remove(&pos);
    }

    /// TOCLIENT_DETACHED_INVENTORY with a chest's contents.
    fn chest_packet(&self, pos: NodePos) -> Option<Vec<u8>> {
        let c = self.chests.get(pos)?;
        let slots = c.main.iter().map(|s| s.map(|s| s.item_string())).collect();
        Some(build_detached_inventory(&chest::detached_name(pos), &[("main", 0, slots)]))
    }

    /// Send a chest's contents to everyone who has it open.
    fn send_chest(&mut self, pos: NodePos) {
        let Some(packet) = self.chest_packet(pos) else {
            return;
        };
        let viewers: Vec<SocketAddr> = self.peers.iter().filter(|(_, p)| p.chest == Some(pos)).map(|(a, _)| *a).collect();
        for addr in viewers {
            self.send_command(addr, 0, &packet);
        }
    }

    /// Change a node's look and nothing else (minetest_game's swap_node).
    fn swap_node(&mut self, pos: NodePos, id: u16, param2: u8) {
        let p16 = [pos.0 as i16, pos.1 as i16, pos.2 as i16];
        self.world.set_node(pos, id, param2);
        self.broadcast_node(p16, &build_addnode(p16, id, param2));
    }

    /// Open a furnace's window for a player: its contents, then the form.
    fn open_furnace(&mut self, addr: SocketAddr, pos: NodePos) {
        self.furnaces.get_or_add(pos);
        let Some(peer) = self.peers.get_mut(&addr) else {
            return;
        };
        peer.furnace = Some(pos);
        info!("{} opened the furnace at {pos:?}", peer.display_name());
        if let Some(packet) = self.furnace_packet(pos) {
            self.send_command(addr, 0, &packet);
        }
        self.send_furnace_form(addr, pos);
    }

    /// Send (or update) the furnace window, showing the fire and progress.
    fn send_furnace_form(&mut self, addr: SocketAddr, pos: NodePos) {
        let display = self.furnaces.get(pos).and_then(|f| f.display);
        let form = furnace_formspec(&furnace::detached_name(pos), display);
        if let Some(peer) = self.peers.get_mut(&addr) {
            peer.furnace_form_sent = Some(Instant::now());
        }
        self.send_command(addr, 0, &build_show_formspec(&form, FURNACE_FORM));
    }

    /// Run the timers of the furnaces that are burning (or were just
    /// filled): smelt, swap between the lit and unlit furnace nodes, keep
    /// open windows up to date, and play the fire and cooling sounds.
    fn step_furnaces(&mut self, dt: f32) {
        let mut save = false;
        for pos in self.furnaces.running() {
            let node = self.world.node(pos);
            if !content::is_furnace(node) {
                warn!("No furnace at {pos:?} any more; forgetting its contents");
                self.furnaces.remove(pos);
                continue;
            }
            let Some(f) = self.furnaces.get_mut(pos) else {
                continue;
            };
            let shown = f.display;
            let step = f.step(dt);
            let display = f.display;
            save |= step.changed;
            let p16 = [pos.0 as i16, pos.1 as i16, pos.2 as i16];
            let at = [pos.0 as f32 * 10.0, pos.1 as f32 * 10.0, pos.2 as f32 * 10.0];
            // The lit or unlit node (minetest_game's swap_node).
            let want = if step.active { content::CONTENT_FURNACE_ACTIVE } else { content::CONTENT_FURNACE };
            if want != node {
                let param2 = self.world.param2(pos);
                self.world.set_node(pos, want, param2);
                self.broadcast_node(p16, &build_addnode(p16, want, param2));
            }
            if step.smelted > 0 {
                let gain = 0.07 * step.smelted.min(7) as f32;
                self.play_sound_near(-1, "default_cool_lava", gain, at, FURNACE_HEARING);
            }
            if step.fire_sound {
                let id = self.next_sound_id;
                self.next_sound_id = if id == i32::MAX { 1 } else { id + 1 };
                self.play_sound_near(id, "default_furnace_active", 0.25, at, FURNACE_HEARING);
                if let Some(f) = self.furnaces.get_mut(pos) {
                    f.sounds.push(id);
                    // Only the last three can still be playing.
                    if f.sounds.len() > 3 {
                        f.sounds.remove(0);
                    }
                }
            }
            if !step.active {
                let sounds = self.furnaces.get_mut(pos).map(|f| std::mem::take(&mut f.sounds)).unwrap_or_default();
                self.fade_sounds(&sounds);
            }
            if step.changed {
                self.send_furnace(pos);
            }
            if display != shown {
                let viewers: Vec<SocketAddr> =
                    self.peers.iter().filter(|(_, p)| p.furnace == Some(pos)).map(|(a, _)| *a).collect();
                for addr in viewers {
                    self.send_furnace_form(addr, pos);
                }
            }
        }
        if save {
            self.furnaces.save();
        }
    }

    /// TOCLIENT_DETACHED_INVENTORY with a furnace's contents.
    fn furnace_packet(&self, pos: NodePos) -> Option<Vec<u8>> {
        let f = self.furnaces.get(pos)?;
        let lists: Vec<_> = f
            .lists()
            .into_iter()
            .map(|(name, width, slots)| (name, width, slots.iter().map(|s| s.map(|s| s.item_string())).collect()))
            .collect();
        Some(build_detached_inventory(&furnace::detached_name(pos), &lists))
    }

    /// Send a furnace's contents to everyone who has it open.
    fn send_furnace(&mut self, pos: NodePos) {
        let Some(packet) = self.furnace_packet(pos) else {
            return;
        };
        let viewers: Vec<SocketAddr> = self.peers.iter().filter(|(_, p)| p.furnace == Some(pos)).map(|(a, _)| *a).collect();
        for addr in viewers {
            self.send_command(addr, 0, &packet);
        }
    }

    /// Close a furnace's window for everyone who has it open (it was dug).
    fn close_furnace(&mut self, pos: NodePos) {
        let viewers: Vec<SocketAddr> = self.peers.iter().filter(|(_, p)| p.furnace == Some(pos)).map(|(a, _)| *a).collect();
        for addr in viewers {
            if let Some(peer) = self.peers.get_mut(&addr) {
                peer.furnace = None;
            }
            self.send_command(addr, 0, &build_show_formspec("", FURNACE_FORM));
        }
    }

    /// Save a player's inventory and send it to them.
    fn inventory_changed(&mut self, addr: SocketAddr) {
        if let Some(peer) = self.peers.get(&addr) {
            self.inventories.save(peer.display_name(), &peer.inventory);
        }
        self.send_inventory(addr);
    }

    fn send_inventory(&mut self, addr: SocketAddr) {
        let Some(peer) = self.peers.get(&addr) else {
            return;
        };
        let lists: Vec<_> = peer
            .inventory
            .lists()
            .into_iter()
            .map(|(name, width, slots)| (name, width, slots.iter().map(|s| s.map(|s| s.item_string())).collect()))
            .collect();
        let packet = build_inventory(&lists);
        self.send_command(addr, 0, &packet);
    }

    /// Send a node change to every player whose client has that map block.
    /// Others will get the changed block when they come near. The change
    /// goes on the block channel, so it can't overtake (and be undone by)
    /// a copy of the block that is still on its way.
    fn broadcast_node(&mut self, pos: [i16; 3], packet: &[u8]) {
        let block = world::block_of_node((pos[0] as i32, pos[1] as i32, pos[2] as i32));
        let targets: Vec<SocketAddr> = self
            .peers
            .iter()
            .filter(|(_, p)| p.ready && p.sent_blocks.contains(&block))
            .map(|(a, _)| *a)
            .collect();
        for addr in targets {
            self.send_command(addr, BLOCK_CHANNEL, packet);
        }
    }

    /// Tell one client what a node really is, undoing its prediction.
    fn resend_node(&mut self, addr: SocketAddr, pos: [i16; 3]) {
        let id = self.world.node((pos[0] as i32, pos[1] as i32, pos[2] as i32));
        let p2 = self.world.param2((pos[0] as i32, pos[1] as i32, pos[2] as i32));
        let packet = if id == world::CONTENT_AIR { build_removenode(pos) } else { build_addnode(pos, id, p2) };
        self.send_command(addr, BLOCK_CHANNEL, &packet);
    }

    fn on_deleted_blocks(&mut self, from: SocketAddr, payload: &[u8]) {
        let Some(list) = parse_block_list(payload) else {
            return;
        };
        if let Some(peer) = self.peers.get_mut(&from) {
            for pos in &list {
                peer.sent_blocks.remove(pos);
            }
            debug!("Client dropped {} blocks", list.len());
            // Re-plan so blocks come back if the player returns.
            peer.view_center = None;
        }
    }

    /// Work out which blocks around `pos` (in nodes) the client still needs,
    /// nearest first.
    fn plan_blocks(&mut self, addr: SocketAddr, pos: [f32; 3], radius: i16) {
        let center = node_to_block(pos);
        let Some(peer) = self.peers.get(&addr) else {
            return;
        };

        // In each column of blocks: the blocks holding the surface there,
        // from one node below its lowest point to one above its highest,
        // plus the blocks around the player (who may be flying or in a
        // cave).
        let mut wanted = Vec::new();
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx * dx + dz * dz > radius * radius + radius {
                    continue; // roughly circular
                }
                let (bx, bz) = (center.0 + dx, center.2 + dz);
                if !world::block_in_world((bx, 0, bz)) {
                    continue;
                }
                let (low, high) = self.world.height_range(bx, bz);
                let surface = world::block_of(low - 1)..=world::block_of(high + 1);
                let around = center.1 - 1..=center.1 + 1;
                let ys = surface.clone().chain(around.filter(|y| !surface.contains(y)));
                for y in ys {
                    let p = (bx, y, bz);
                    // Nothing beyond the edge of the world: the client
                    // treats missing blocks as solid, so that is the wall.
                    if world::block_in_world(p) && !peer.sent_blocks.contains(&p) {
                        wanted.push(p);
                    }
                }
            }
        }
        let Some(peer) = self.peers.get_mut(&addr) else {
            return;
        };
        wanted.sort_by_key(|p| {
            let (dx, dy, dz) = (
                (p.0 - center.0) as i32,
                (p.1 - center.1) as i32,
                (p.2 - center.2) as i32,
            );
            dx * dx + dy * dy * 2 + dz * dz
        });

        if !wanted.is_empty() {
            debug!(
                "{}: {} blocks to send around block {center:?}",
                peer.display_name(),
                wanted.len()
            );
        }
        peer.pending_blocks = wanted.into();
        peer.view_center = Some(center);
        peer.view_radius = radius;
    }

    /// Send players the blocks they still need: from the cache of built
    /// blocks when possible, otherwise asking the builder thread for them
    /// (nearest first) and sending them as they are finished.
    fn send_blocks(&mut self) {
        self.collect_built();
        let started = Instant::now();
        // Take turns between players, so one player loading a lot doesn't
        // hold up the others.
        let mut addrs: Vec<SocketAddr> = self.peers.keys().copied().collect();
        if !addrs.is_empty() {
            let n = self.block_turn % addrs.len();
            addrs.rotate_left(n);
            self.block_turn = self.block_turn.wrapping_add(1);
        }
        for &addr in &addrs {
            loop {
                if self.congested() {
                    return;
                }
                let Some(peer) = self.peers.get_mut(&addr) else {
                    break;
                };
                if !peer.ready {
                    break;
                }
                // Keep a few blocks queued or being built per player, no more:
                // the plan changes as the player moves.
                if peer.channels[BLOCK_CHANNEL].queue.len() + peer.awaiting.len() >= BLOCK_QUEUE_LIMIT {
                    break;
                }
                let Some(pos) = peer.pending_blocks.pop_front() else {
                    break;
                };
                if peer.sent_blocks.contains(&pos) || peer.awaiting.contains(&pos) {
                    continue;
                }
                if let Some(payload) = self.built_block(pos) {
                    self.deliver(addr, pos, &payload);
                } else if self.builder.is_some() {
                    if let Some(peer) = self.peers.get_mut(&addr) {
                        peer.awaiting.push_back(pos);
                    }
                } else {
                    // No builder thread: build here, a little at a time.
                    if started.elapsed() >= BLOCK_TIME_BUDGET {
                        if let Some(peer) = self.peers.get_mut(&addr) {
                            peer.pending_blocks.push_front(pos);
                        }
                        return;
                    }
                    let building = Instant::now();
                    let payload = Arc::new(build_blockdata(&self.world.block_payload(pos)));
                    {
                        let mut st = status::lock(&self.status);
                        st.blocks_built += 1;
                        st.block_us += building.elapsed().as_micros() as u64;
                    }
                    self.keep_built(pos, self.world.version(), payload.clone());
                    self.deliver(addr, pos, &payload);
                }
            }
        }
        self.submit_builds(&addrs);
    }

    /// A built copy of a block that is still up to date, if there is one.
    fn built_block(&mut self, pos: BlockPos) -> Option<Arc<Vec<u8>>> {
        let (version, payload) = self.built.get(&pos)?;
        if self.world.changed_near_since(pos, *version) {
            self.built.remove(&pos);
            return None;
        }
        Some(payload.clone())
    }

    fn keep_built(&mut self, pos: BlockPos, version: u64, payload: Arc<Vec<u8>>) {
        if self.built.len() >= BUILT_CACHE_MAX {
            self.built.clear();
        }
        self.built.insert(pos, (version, payload));
    }

    /// Take finished blocks from the builder and send them to whoever is
    /// waiting. A block the world has changed around since it was started is
    /// dropped; it gets built again.
    fn collect_built(&mut self) {
        let Some(builder) = &self.builder else {
            return;
        };
        let finished: Vec<Built> = builder.done.try_iter().collect();
        for b in finished {
            self.building.remove(&b.pos);
            if self.world.changed_near_since(b.pos, b.version) {
                continue;
            }
            let payload = Arc::new(b.payload);
            self.keep_built(b.pos, b.version, payload.clone());
            let waiting: Vec<SocketAddr> = self
                .peers
                .iter()
                .filter(|(_, p)| p.awaiting.contains(&b.pos))
                .map(|(a, _)| *a)
                .collect();
            for addr in waiting {
                if let Some(peer) = self.peers.get_mut(&addr) {
                    peer.awaiting.retain(|p| *p != b.pos);
                }
                self.deliver(addr, b.pos, &payload);
            }
        }
    }

    /// Hand the builder the next blocks players are waiting for, taking
    /// turns between players, keeping at most BUILDS_IN_FLIGHT in hand.
    fn submit_builds(&mut self, addrs: &[SocketAddr]) {
        let Some(builder) = &self.builder else {
            return;
        };
        let mut round = 0;
        while self.building.len() < BUILDS_IN_FLIGHT {
            let mut submitted = false;
            for addr in addrs {
                if self.building.len() >= BUILDS_IN_FLIGHT {
                    break;
                }
                let Some(peer) = self.peers.get(addr) else {
                    continue;
                };
                // This player's next block not already being built.
                let Some(&pos) = peer.awaiting.iter().filter(|p| !self.building.contains(p)).nth(round) else {
                    continue;
                };
                if builder.jobs.send(BuildJob { pos, state: self.world.snapshot() }).is_err() {
                    warn!("The block builder has stopped; building blocks in the server loop instead");
                    status::problem();
                    self.builder = None;
                    // Hand the waiting blocks back to be built here.
                    for peer in self.peers.values_mut() {
                        for p in peer.awaiting.drain(..).rev() {
                            peer.pending_blocks.push_front(p);
                        }
                    }
                    self.building.clear();
                    return;
                }
                self.building.insert(pos);
                submitted = true;
            }
            if !submitted {
                break;
            }
            round += 1;
        }
    }

    /// Send a block to a player and note that they have it.
    fn deliver(&mut self, addr: SocketAddr, pos: BlockPos, payload: &[u8]) {
        let Some(peer) = self.peers.get_mut(&addr) else {
            return;
        };
        if !peer.sent_blocks.insert(pos) {
            return;
        }
        let finished = peer.pending_blocks.is_empty() && peer.awaiting.is_empty();
        let (total, name) = (peer.sent_blocks.len(), peer.display_name().to_string());
        status::lock(&self.status).blocks_sent += 1;
        self.send_command(addr, BLOCK_CHANNEL, payload);
        if finished {
            info!("{name}: all nearby blocks sent ({total} in view)");
        }
    }

    // -----------------------------------------------------------------------
    // Peers and the reliability layer
    // -----------------------------------------------------------------------

    fn add_peer(&mut self, addr: SocketAddr) {
        let id = self.next_peer_id;
        let next = self.next_peer_id.wrapping_add(1);
        self.next_peer_id = if next < 2 { 2 } else { next };

        self.peers.insert(addr, Peer::new(id));
        info!("New peer {id} from {addr}");
        self.refresh_players();
        self.send_reliable(addr, 0, set_peer_id_body(id));
    }

    /// Send a command reliably, splitting it if it doesn't fit in one datagram.
    fn send_command(&mut self, addr: SocketAddr, channel: usize, payload: &[u8]) {
        if 1 + payload.len() <= MAX_RELIABLE_BODY {
            self.send_reliable(addr, channel, original_body(payload));
            return;
        }
        let Some(peer) = self.peers.get_mut(&addr) else {
            return;
        };
        let ch = &mut peer.channels[channel];
        let split_seqnum = ch.next_split;
        ch.next_split = split_seqnum.wrapping_add(1);
        let bodies = split_bodies(payload, split_seqnum);
        debug!(
            "Splitting {} byte command into {} packets on ch{channel}",
            payload.len(),
            bodies.len()
        );
        ch.queue.extend(bodies);
        self.flush(addr, channel);
    }

    fn send_reliable(&mut self, addr: SocketAddr, channel: usize, body: Vec<u8>) {
        let Some(peer) = self.peers.get_mut(&addr) else {
            return;
        };
        peer.channels[channel].queue.push_back(body);
        self.flush(addr, channel);
    }

    /// Send queued reliable packets while there is room in the window and
    /// the radio is keeping up. What isn't sent stays queued; flush is
    /// called again from tick().
    fn flush(&mut self, addr: SocketAddr, channel: usize) {
        if self.congested() {
            return;
        }
        let mut failed = false;
        let mut count = 0;
        if let Some(peer) = self.peers.get_mut(&addr) {
            let timeout = peer.resend_timeout;
            let ch = &mut peer.channels[channel];
            while ch.unacked.len() < RELIABLE_WINDOW {
                let Some(body) = ch.queue.pop_front() else {
                    break;
                };
                let seqnum = ch.next_out;
                ch.next_out = seqnum.wrapping_add(1);
                let data = reliable_datagram(channel as u8, seqnum, &body);
                let now = Instant::now();
                let sent = send_datagram(&self.socket, addr, &data);
                // Unsent, it is in the window all the same (it has its
                // seqnum), and is tried again soon.
                let (first_sent, due) = if sent { (Some(now), now + timeout) } else { (None, now + RETRY_UNSENT) };
                ch.unacked.push(Unacked { seqnum, data, first_sent, due, resends: 0 });
                count += 1;
                if !sent {
                    failed = true;
                    break;
                }
            }
        }
        status::lock(&self.status).packets_out += count;
        if failed {
            self.note_send_failure();
        }
    }

    /// Send one datagram now. Returns whether it went.
    fn send_raw(&mut self, addr: SocketAddr, data: &[u8]) -> bool {
        status::lock(&self.status).packets_out += 1;
        let sent = send_datagram(&self.socket, addr, data);
        if !sent {
            self.note_send_failure();
        }
        sent
    }

    /// The radio's transmit buffers are full (or sending failed some other
    /// way): back off, and report it now and then rather than per packet.
    fn note_send_failure(&mut self) {
        self.congested_until = Instant::now() + CONGESTION_BACKOFF;
        status::lock(&self.status).send_failures += 1;
        self.send_failures += 1;
        if self.send_failures_reported.elapsed() >= SEND_FAILURE_REPORT_INTERVAL {
            use std::sync::atomic::Ordering::Relaxed;
            let full = SEND_ERRORS_FULL.swap(0, Relaxed);
            let other = SEND_ERRORS_OTHER.swap(0, Relaxed);
            let last = LAST_OTHER_ERROR.load(Relaxed);
            let others = if other > 0 {
                format!(", {other} other (last: {})", std::io::Error::from_raw_os_error(last))
            } else {
                String::new()
            };
            let rtts: Vec<String> = self
                .peers
                .values()
                .filter_map(|p| p.avg_rtt.map(|r| format!("{} {:.0} ms", p.display_name(), r * 1000.0)))
                .collect();
            warn!(
                "{} packet(s) couldn't be sent in the last {}s: {full} with Wi-Fi transmit buffers full{others}; \
                 {} resent, {} ACKs; round trip {}; slowing down",
                self.send_failures,
                self.send_failures_reported.elapsed().as_secs(),
                self.resends,
                self.acks,
                if rtts.is_empty() { "unknown".to_string() } else { rtts.join(", ") },
            );
            self.resends = 0;
            self.acks = 0;
            self.send_failures = 0;
            self.send_failures_reported = Instant::now();
        }
    }

    /// Someone is doing something: Wi-Fi fully awake, at once.
    /// Remember something the server did, for the heap check's report.
    fn note(&mut self, what: &'static str, n: u16) {
        if self.recent.len() >= RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back((Instant::now(), what, n));
    }

    fn note_command(&mut self, cmd: u16, peer: u16) {
        // The command and the peer it came from, in one number.
        self.note("command (id << 4 | peer)", cmd << 4 | (peer & 0xf));
    }

    /// Every HEAP_CHECK_INTERVAL, check the heaps (and, with heap
    /// poisoning on, every block's guard words). Damage is reported with
    /// what the server did just before, so it can be traced to its cause
    /// rather than crashing later somewhere unrelated.
    fn check_heap(&mut self, now: Instant) {
        if self.heap_damaged || now.duration_since(self.heap_checked) < HEAP_CHECK_INTERVAL {
            return;
        }
        let last_good = self.heap_checked;
        self.heap_checked = now;
        let started = Instant::now();
        let ok = heap_intact();
        let took = started.elapsed();
        if !self.heap_check_timed {
            self.heap_check_timed = true;
            info!("Heap check: {} us", took.as_micros());
        }
        if ok {
            return;
        }
        self.heap_damaged = true;
        log::error!("HEAP DAMAGED: found now, intact {} ms ago. What the server did lately, oldest first:", now.duration_since(last_good).as_millis());
        for (at, what, n) in &self.recent {
            log::error!("  {:>6} ms ago: {what} {n:#06x}", now.duration_since(*at).as_millis());
        }
        crate::status::problem();
    }

    fn wake(&mut self) {
        self.last_activity = Instant::now();
        if self.power_saving {
            self.power_saving = false;
            set_wifi_power_save(false);
            status::lock(&self.status).wifi_dozing = false;
            info!("Wi-Fi power saving off");
        }
    }

    /// Let the Wi-Fi radio doze between the router's beacons once nobody
    /// has done anything for IDLE_POWER_SAVE: no one connected, or every
    /// player idle with their map loaded. It saves power and heat (the
    /// board's 3.3 V regulator gets hot), at the cost of delaying packets
    /// by up to a few hundred milliseconds, which only the first move after
    /// a pause will notice.
    fn update_power_saving(&mut self) {
        if self.power_saving || self.last_activity.elapsed() < IDLE_POWER_SAVE {
            return;
        }
        // Not while a player is still being sent their map: it would load
        // slowly, the client's acknowledgements being delayed.
        let loading = self.peers.values().any(|p| !p.ready || !p.pending_blocks.is_empty() || !p.awaiting.is_empty());
        if loading {
            return;
        }
        self.power_saving = true;
        set_wifi_power_save(true);
        status::lock(&self.status).wifi_dozing = true;
        info!("Wi-Fi power saving on: nobody has done anything for {}s", IDLE_POWER_SAVE.as_secs());
    }

    fn congested(&self) -> bool {
        Instant::now() < self.congested_until
    }

    /// Periodic housekeeping: drop silent peers, resend unacknowledged
    /// packets, send map blocks.
    fn tick(&mut self) {
        let now = Instant::now();
        self.finish_srp();
        self.send_blocks();
        if now.duration_since(self.items_stepped) >= ITEM_STEP_INTERVAL {
            let dt = now.duration_since(self.items_stepped).as_secs_f32();
            self.items_stepped = now;
            self.step_items(dt);
        }
        if self.items.save_if_due() {
            self.note("items saved", 0);
        }
        if self.clock.save_if_due() {
            self.note("time saved", 0);
        }
        self.check_heap(now);
        self.update_power_saving();
        self.run_timers();
        if now.duration_since(self.furnaces_stepped) >= FURNACE_STEP_INTERVAL {
            let dt = now.duration_since(self.furnaces_stepped).as_secs_f32();
            self.furnaces_stepped = now;
            self.step_furnaces(dt);
        }

        let mut gone = Vec::new();
        let mut saved = Vec::new();
        let before = self.peers.len();
        let players = &self.players;
        self.peers.retain(|addr, peer| {
            let alive = now.duration_since(peer.last_seen) < PEER_TIMEOUT;
            if !alive {
                info!("Peer {} ({}) at {addr} timed out", peer.id, peer.display_name());
                peer.save(players);
                saved.push(peer.id);
                if peer.ready {
                    gone.push((peer.id, peer.display_name().to_string()));
                }
            } else if peer.ready && now.duration_since(peer.saved_at) >= SAVE_INTERVAL {
                peer.save(players);
                saved.push(peer.id);
            }
            alive
        });
        for id in saved {
            self.note("player saved", id);
        }
        for (id, name) in gone {
            self.remove_player_object(id);
            self.event(format!("{name} timed out"));
            status::lock(&self.status).leaves += 1;
        }
        if self.peers.len() != before {
            self.close_unwatched_lids();
            self.refresh_players();
        }

        // Resend what hasn't been acknowledged, then anything still
        // queued, unless the radio is behind.
        if self.congested() {
            return;
        }
        let mut failed = false;
        let mut status_out = 0;
        let mut resent = 0;
        'peers: for (addr, peer) in self.peers.iter_mut() {
            let timeout = peer.resend_timeout;
            for (n, ch) in peer.channels.iter_mut().enumerate() {
                for u in ch.unacked.iter_mut() {
                    if now < u.due {
                        continue;
                    }
                    if !send_datagram(&self.socket, *addr, &u.data) {
                        u.due = now + RETRY_UNSENT;
                        failed = true;
                        break 'peers;
                    }
                    status_out += 1;
                    if u.first_sent.is_none() {
                        // Its first sending (the radio was full before).
                        u.first_sent = Some(now);
                        u.due = now + timeout;
                    } else {
                        // Each resend of a packet waits 1.5 times longer
                        // (Luanti's RESEND_SCALE_BASE), so a slow link isn't
                        // swamped with copies of what is still on its way.
                        u.resends += 1;
                        resent += 1;
                        u.due = now + timeout.mul_f32(RESEND_SCALE_BASE.powi(u.resends as i32));
                        debug!("Resending reliable {} on ch{n} (resend {})", u.seqnum, u.resends);
                    }
                }
            }
        }
        self.resends += resent;
        status::lock(&self.status).packets_out += status_out;
        if failed {
            self.note_send_failure();
            return;
        }
        let queued: Vec<(SocketAddr, usize)> = self
            .peers
            .iter()
            .flat_map(|(a, p)| p.channels.iter().enumerate().filter(|(_, c)| !c.queue.is_empty()).map(move |(n, _)| (*a, n)))
            .collect();
        for (addr, channel) in queued {
            self.flush(addr, channel);
        }
    }
}

pub fn hexdump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();
        info!("  {:04x}  {:<48} {}", i * 16, hex.join(" "), ascii);
    }
}

/// The way a player with this pitch and yaw (degrees, as the client sends
/// them) looks (ObjectRef::get_look_dir).
fn look_dir(pitch: f32, yaw: f32) -> [f32; 3] {
    let p = (-pitch).to_radians();
    let y = (yaw + 90.0).to_radians();
    [p.cos() * y.cos(), p.sin(), p.cos() * y.sin()]
}

/// A bucket used or placed: the hotbar slot holding it, what it pointed at,
/// and where the player's body is (nodes).
#[derive(Clone, Copy)]
struct BucketUse {
    slot: usize,
    under: [i16; 3],
    above: [i16; 3],
    body: [f32; 3],
}

/// Where a player puts a door: what they pointed at, where they look, and
/// the hotbar slot holding it.
struct DoorPlace {
    under: [i16; 3],
    above: [i16; 3],
    look: [f32; 3],
    slot: usize,
}

/// Where builtin's item_drop puts a dropped item, and how it throws it:
/// position and velocity in nodes.
fn drop_motion(pos_bs: [f32; 3], yaw: f32, pitch: f32) -> ([f32; 3], [f32; 3]) {
    let dir = look_dir(pitch, yaw);
    let pos = [pos_bs[0] / 10.0, pos_bs[1] / 10.0 + DROP_HEIGHT, pos_bs[2] / 10.0];
    let vel = [dir[0] * DROP_SPEED, dir[1] * DROP_SPEED + DROP_LIFT, dir[2] * DROP_SPEED];
    (pos, vel)
}

/// The strings a dropped item's object needs, which ItemInit borrows.
struct ItemStrings {
    itemstring: String,
    item: u16,
    count: u16,
    pos_bs: [f32; 3],
    vel_bs: [f32; 3],
}

impl ItemStrings {
    fn as_init(&self, id: u16) -> ItemInit<'_> {
        ItemInit {
            id,
            itemstring: &self.itemstring,
            item_name: content::name_of(self.item),
            description: content::description_of(self.item),
            count: self.count,
            light_source: content::light_source(self.item),
            pos_bs: self.pos_bs,
            vel_bs: self.vel_bs,
        }
    }
}

fn item_init_of(d: &crate::items::Dropped) -> ItemStrings {
    ItemStrings {
        itemstring: format!("{} {}", content::name_of(d.item), d.count),
        item: d.item,
        count: d.count,
        pos_bs: d.pos_bs(),
        vel_bs: d.vel_bs(),
    }
}

/// GenericCAO initialisation data for a dropped item, followed by its
/// current motion (so a falling item keeps falling for a newcomer).
fn item_init(d: &crate::items::Dropped) -> Vec<u8> {
    item_init_data(&item_init_of(d).as_init(d.id))
}

/// Send a datagram, saying whether it went. ENOMEM ("Not enough space")
/// means the Wi-Fi transmit buffers are full.
fn send_datagram(socket: &UdpSocket, addr: SocketAddr, data: &[u8]) -> bool {
    if test_send_failure() {
        return false;
    }
    match socket.send_to(data, addr) {
        Ok(_) => true,
        Err(e) => {
            use std::sync::atomic::Ordering::Relaxed;
            debug!("send_to {addr} failed: {e}");
            // ENOMEM: lwIP and the Wi-Fi driver out of transmit buffers.
            if e.raw_os_error() == Some(ENOMEM) {
                SEND_ERRORS_FULL.fetch_add(1, Relaxed);
            } else {
                SEND_ERRORS_OTHER.fetch_add(1, Relaxed);
                LAST_OTHER_ERROR.store(e.raw_os_error().unwrap_or(0), Relaxed);
            }
            false
        }
    }
}

/// Send errors by kind since the last report (see note_send_failure).
static SEND_ERRORS_FULL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static SEND_ERRORS_OTHER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LAST_OTHER_ERROR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
/// errno "Not enough space" (the same number in lwIP and on Linux).
const ENOMEM: i32 = 12;

/// Wi-Fi modem power saving (WIFI_PS_MIN_MODEM): the radio sleeps between
/// the router's beacons, which holds incoming packets until it wakes.
/// Off (WIFI_PS_NONE) keeps it listening all the time.
#[cfg(target_os = "espidf")]
fn set_wifi_power_save(on: bool) {
    use esp_idf_svc::sys::{esp_wifi_set_ps, wifi_ps_type_t_WIFI_PS_MIN_MODEM, wifi_ps_type_t_WIFI_PS_NONE};
    let mode = if on { wifi_ps_type_t_WIFI_PS_MIN_MODEM } else { wifi_ps_type_t_WIFI_PS_NONE };
    if let Err(e) = esp_idf_svc::sys::esp!(unsafe { esp_wifi_set_ps(mode) }) {
        warn!("Setting Wi-Fi power saving: {e}");
    }
}

#[cfg(not(target_os = "espidf"))]
fn set_wifi_power_save(_on: bool) {}

/// Whether ESP-IDF's heaps are intact (heap_caps_check_integrity_all,
/// which prints what it finds wrong).
#[cfg(target_os = "espidf")]
fn heap_intact() -> bool {
    unsafe { esp_idf_svc::sys::heap_caps_check_integrity_all(true) }
}

#[cfg(not(target_os = "espidf"))]
fn heap_intact() -> bool {
    true
}

/// For testing on a PC: with LUANTI_FAIL_SENDS=n set, every nth send
/// fails, as sends do on the ESP32 when its Wi-Fi buffers are full.
#[cfg(not(target_os = "espidf"))]
fn test_send_failure() -> bool {
    use std::sync::atomic::{AtomicU32, Ordering};
    static EVERY: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let every = *EVERY.get_or_init(|| std::env::var("LUANTI_FAIL_SENDS").ok().and_then(|v| v.parse().ok()).unwrap_or(0));
    every > 0 && COUNT.fetch_add(1, Ordering::Relaxed) % every == every - 1
}

#[cfg(target_os = "espidf")]
fn test_send_failure() -> bool {
    false
}

fn node_to_block(pos: [f32; 3]) -> BlockPos {
    let b = |v: f32| world::block_of(v.round() as i32);
    (b(pos[0]), b(pos[1]), b(pos[2]))
}
