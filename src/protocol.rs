//! Luanti low-level network protocol: datagram framing, the reliability
//! layer, and control packets.
//!
//! Every datagram starts with a 7-byte header:
//!   u32 protocol id (0x4f457403), u16 sender peer id, u8 channel
//! followed by one packet:
//!   0 CONTROL   u8 control type, then type-specific data
//!   1 ORIGINAL  the payload (a TOSERVER_* / TOCLIENT_* command)
//!   2 SPLIT     u16 seqnum, u16 chunk count, u16 chunk number, data
//!   3 RELIABLE  u16 seqnum, then another (non-reliable) packet

pub const PROTOCOL_ID: u32 = 0x4f45_7403;
pub const PEER_ID_INEXISTENT: u16 = 0;
pub const PEER_ID_SERVER: u16 = 1;
pub const SEQNUM_INITIAL: u16 = 65500;
pub const CHANNEL_COUNT: usize = 3;
pub const HEADER_SIZE: usize = 7;

/// Luanti's maximum UDP datagram size.
pub const MAX_PACKET_SIZE: usize = 512;
/// The largest body (from the packet type byte on) that fits in one reliable
/// datagram: 512 minus the 7-byte header and 3-byte reliable header.
pub const MAX_RELIABLE_BODY: usize = MAX_PACKET_SIZE - HEADER_SIZE - 3;
const SPLIT_HEADER_SIZE: usize = 7;

const TYPE_CONTROL: u8 = 0;
const TYPE_ORIGINAL: u8 = 1;
const TYPE_SPLIT: u8 = 2;
const TYPE_RELIABLE: u8 = 3;

const CONTROL_ACK: u8 = 0;
const CONTROL_SET_PEER_ID: u8 = 1;
const CONTROL_PING: u8 = 2;
const CONTROL_DISCO: u8 = 3;

#[derive(Debug)]
pub enum Control {
    Ack(u16),
    SetPeerId(u16),
    Ping,
    Disco,
    Unknown(u8),
}

impl std::fmt::Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Control::Ack(seqnum) => write!(f, "ACK {seqnum}"),
            Control::SetPeerId(id) => write!(f, "SET_PEER_ID {id}"),
            Control::Ping => write!(f, "PING"),
            Control::Disco => write!(f, "DISCO"),
            Control::Unknown(kind) => write!(f, "unknown control type {kind}"),
        }
    }
}

#[derive(Debug)]
pub enum Packet<'a> {
    Control(Control),
    Original(&'a [u8]),
    Split {
        seqnum: u16,
        chunk_count: u16,
        chunk_num: u16,
        data: &'a [u8],
    },
    Reliable {
        seqnum: u16,
        inner: Box<Packet<'a>>,
        /// The inner packet's bytes, for keeping a packet that arrived
        /// early until its turn (see `parse_inner`).
        raw: &'a [u8],
    },
}

#[derive(Debug)]
pub struct Datagram<'a> {
    pub peer_id: u16,
    pub channel: u8,
    pub packet: Packet<'a>,
}

#[derive(Debug)]
pub enum ParseError {
    TooShort,
    BadProtocolId(u32),
    BadChannel(u8),
    UnknownType(u8),
    NestedReliable,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::TooShort => write!(f, "too short"),
            ParseError::BadProtocolId(id) => write!(f, "protocol id {id:#010x} isn't Luanti's"),
            ParseError::BadChannel(ch) => write!(f, "channel {ch} doesn't exist"),
            ParseError::UnknownType(t) => write!(f, "unknown packet type {t}"),
            ParseError::NestedReliable => write!(f, "reliable packet inside a reliable packet"),
        }
    }
}

pub fn parse_datagram(buf: &[u8]) -> Result<Datagram<'_>, ParseError> {
    if buf.len() < HEADER_SIZE + 1 {
        return Err(ParseError::TooShort);
    }
    let id = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if id != PROTOCOL_ID {
        return Err(ParseError::BadProtocolId(id));
    }
    let peer_id = u16::from_be_bytes([buf[4], buf[5]]);
    let channel = buf[6];
    if channel as usize >= CHANNEL_COUNT {
        return Err(ParseError::BadChannel(channel));
    }
    let packet = parse_packet(&buf[HEADER_SIZE..], true)?;
    Ok(Datagram {
        peer_id,
        channel,
        packet,
    })
}

/// Parse the inner packet of a reliable packet, from its `raw` bytes.
pub fn parse_inner(buf: &[u8]) -> Result<Packet<'_>, ParseError> {
    parse_packet(buf, false)
}

fn parse_packet(buf: &[u8], allow_reliable: bool) -> Result<Packet<'_>, ParseError> {
    let (&ty, rest) = buf.split_first().ok_or(ParseError::TooShort)?;
    match ty {
        TYPE_CONTROL => {
            let (&ct, rest) = rest.split_first().ok_or(ParseError::TooShort)?;
            let control = match ct {
                CONTROL_ACK => Control::Ack(be16(rest)?),
                CONTROL_SET_PEER_ID => Control::SetPeerId(be16(rest)?),
                CONTROL_PING => Control::Ping,
                CONTROL_DISCO => Control::Disco,
                other => Control::Unknown(other),
            };
            Ok(Packet::Control(control))
        }
        TYPE_ORIGINAL => Ok(Packet::Original(rest)),
        TYPE_SPLIT => {
            if rest.len() < 6 {
                return Err(ParseError::TooShort);
            }
            Ok(Packet::Split {
                seqnum: be16(&rest[0..])?,
                chunk_count: be16(&rest[2..])?,
                chunk_num: be16(&rest[4..])?,
                data: &rest[6..],
            })
        }
        TYPE_RELIABLE => {
            if !allow_reliable {
                return Err(ParseError::NestedReliable);
            }
            let seqnum = be16(rest)?;
            let raw = &rest[2..];
            let inner = parse_packet(raw, false)?;
            Ok(Packet::Reliable {
                seqnum,
                inner: Box::new(inner),
                raw,
            })
        }
        other => Err(ParseError::UnknownType(other)),
    }
}

fn be16(b: &[u8]) -> Result<u16, ParseError> {
    b.get(0..2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or(ParseError::TooShort)
}

/// True if sequence number `a` comes after `b`, allowing for wrap-around.
pub fn seqnum_higher(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

// ---------------------------------------------------------------------------
// Building outgoing packets. A "body" starts at the packet type byte; a
// "datagram" is a complete UDP payload including the 7-byte header.
// ---------------------------------------------------------------------------

fn header(peer_id: u16, channel: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity(64);
    v.extend_from_slice(&PROTOCOL_ID.to_be_bytes());
    v.extend_from_slice(&peer_id.to_be_bytes());
    v.push(channel);
    v
}

/// Body of an ORIGINAL packet carrying a command payload.
pub fn original_body(payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 1);
    v.push(TYPE_ORIGINAL);
    v.extend_from_slice(payload);
    v
}

/// Bodies of SPLIT packets carrying `payload` in chunks, each small enough to
/// go out as one reliable datagram. The client reassembles them by
/// `split_seqnum`.
pub fn split_bodies(payload: &[u8], split_seqnum: u16) -> Vec<Vec<u8>> {
    let chunk_size = MAX_RELIABLE_BODY - SPLIT_HEADER_SIZE;
    let count = payload.len().div_ceil(chunk_size) as u16;
    payload
        .chunks(chunk_size)
        .enumerate()
        .map(|(i, chunk)| {
            let mut v = Vec::with_capacity(SPLIT_HEADER_SIZE + chunk.len());
            v.push(TYPE_SPLIT);
            v.extend_from_slice(&split_seqnum.to_be_bytes());
            v.extend_from_slice(&count.to_be_bytes());
            v.extend_from_slice(&(i as u16).to_be_bytes());
            v.extend_from_slice(chunk);
            v
        })
        .collect()
}

/// Body of a SET_PEER_ID control packet.
pub fn set_peer_id_body(peer_id: u16) -> Vec<u8> {
    let id = peer_id.to_be_bytes();
    vec![TYPE_CONTROL, CONTROL_SET_PEER_ID, id[0], id[1]]
}

/// Complete datagram telling a client the connection is over (DISCO).
pub fn disco_datagram() -> Vec<u8> {
    let mut v = header(PEER_ID_SERVER, 0);
    v.push(TYPE_CONTROL);
    v.push(CONTROL_DISCO);
    v
}

/// Complete datagram acknowledging a reliable packet on `channel`.
pub fn ack_datagram(channel: u8, seqnum: u16) -> Vec<u8> {
    let mut v = header(PEER_ID_SERVER, channel);
    v.push(TYPE_CONTROL);
    v.push(CONTROL_ACK);
    v.extend_from_slice(&seqnum.to_be_bytes());
    v
}

/// Complete datagram carrying `payload` unreliably (ORIGINAL packet).
pub fn unreliable_datagram(channel: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = header(PEER_ID_SERVER, channel);
    v.push(TYPE_ORIGINAL);
    v.extend_from_slice(payload);
    v
}

/// Complete datagram wrapping `body` in a RELIABLE packet.
pub fn reliable_datagram(channel: u8, seqnum: u16, body: &[u8]) -> Vec<u8> {
    let mut v = header(PEER_ID_SERVER, channel);
    v.push(TYPE_RELIABLE);
    v.extend_from_slice(&seqnum.to_be_bytes());
    v.extend_from_slice(body);
    v
}
