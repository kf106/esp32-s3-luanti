//! What the server reports to the status screen. The server updates it as
//! things happen; the display thread copies it once a second.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// How many recent events the screen keeps.
const EVENTS: usize = 3;

#[derive(Clone)]
pub struct Status {
    pub started: Instant,
    /// "address:port" clients connect to.
    pub address: String,
    /// Players in the game, in joining order.
    pub players: Vec<String>,
    /// Connections still logging in or loading.
    pub joining: usize,
    pub packets_in: u64,
    pub packets_out: u64,
    pub blocks_sent: u64,
    /// Nodes dug or placed since the server started.
    pub edits: u64,
    /// Recent happenings, newest last.
    pub events: VecDeque<String>,
    /// Time the server loop spent working rather than waiting for
    /// packets, in microseconds since start.
    pub busy_us: u64,
    /// Map blocks built (generated and compressed), and the time that took
    /// in microseconds. Blocks are built once and may be sent to several
    /// players.
    pub blocks_built: u64,
    pub block_us: u64,
    /// Packets that couldn't be sent (Wi-Fi transmit buffers full).
    pub send_failures: u64,
}

pub type SharedStatus = Arc<Mutex<Status>>;

impl Status {
    pub fn new(address: String) -> SharedStatus {
        Arc::new(Mutex::new(Self {
            started: Instant::now(),
            address,
            players: Vec::new(),
            joining: 0,
            packets_in: 0,
            packets_out: 0,
            blocks_sent: 0,
            edits: 0,
            events: VecDeque::new(),
            busy_us: 0,
            blocks_built: 0,
            block_us: 0,
            send_failures: 0,
        }))
    }

    pub fn event(&mut self, text: String) {
        if self.events.len() == EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(text);
    }
}

/// Lock the status, carrying on even if a thread panicked while holding it
/// (the data is only for display).
pub fn lock(s: &SharedStatus) -> std::sync::MutexGuard<'_, Status> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}
