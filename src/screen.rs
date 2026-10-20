//! Drawing the status screen into a framebuffer. No hardware here, so the
//! layout can be rendered and checked on a PC; lcd.rs sends the result to
//! the panel.

use std::time::Duration;

use crate::font::{Font, BIG, SMALL};
use crate::status::Status;

/// Landscape: the panel is 172x320, used sideways.
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 172;

/// RGB565 from 8-bit components.
pub const fn rgb(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}

const BACKGROUND: u16 = rgb(16, 20, 24);
const HEADER: u16 = rgb(46, 125, 50);
const TEXT: u16 = rgb(232, 234, 237);
const LABEL: u16 = rgb(138, 145, 153);
const GOOD: u16 = rgb(129, 199, 132);
const EVENT: u16 = rgb(255, 213, 79);
const RULE: u16 = rgb(52, 58, 64);

/// Things the display thread measures itself.
#[derive(Clone, Copy, Default)]
pub struct System {
    pub internal_free_kb: u32,
    pub psram_free_kb: u32,
    /// Wi-Fi signal strength, if connected.
    pub rssi: Option<i8>,
    pub packets_in_per_s: f32,
    pub packets_out_per_s: f32,
}

/// How to draw text: font, colours, and the x at which to clip. Grouped
/// so the drawing functions take few arguments: on the ESP32 a seventh
/// argument word goes on the stack, which once went wrong (see
/// server::Setup).
#[derive(Clone, Copy)]
struct Pen {
    font: &'static Font,
    fg: u16,
    bg: u16,
    clip: usize,
}

impl Pen {
    const fn new(font: &'static Font, fg: u16, bg: u16, clip: usize) -> Self {
        Self { font, fg, bg, clip }
    }
}

pub struct Frame {
    pub pixels: Vec<u16>,
}

impl Frame {
    pub fn new() -> Self {
        Self { pixels: vec![BACKGROUND; WIDTH * HEIGHT] }
    }

    fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, colour: u16) {
        for row in y..(y + h).min(HEIGHT) {
            let start = row * WIDTH + x.min(WIDTH);
            let end = row * WIDTH + (x + w).min(WIDTH);
            self.pixels[start..end].fill(colour);
        }
    }

    /// Draw text with its top-left corner at (x, y) over a plain background
    /// colour, clipped at `max_x`. Returns the x after the last character.
    fn text(&mut self, pen: &Pen, x: usize, y: usize, s: &str) -> usize {
        let (font, fg, bg, max_x) = (pen.font, pen.fg, pen.bg, pen.clip);
        let mut cx = x;
        for c in s.chars() {
            if cx + font.w > max_x.min(WIDTH) {
                break;
            }
            for gy in 0..font.h {
                let py = y + gy;
                if py >= HEIGHT {
                    break;
                }
                for gx in 0..font.w {
                    let a = font.alpha(c, gx, gy);
                    if a != 0 {
                        self.pixels[py * WIDTH + cx + gx] = blend(bg, fg, a);
                    }
                }
            }
            cx += font.w;
        }
        cx
    }

    /// Text ending at `right`.
    fn text_right(&mut self, pen: &Pen, right: usize, y: usize, s: &str) {
        let font = pen.font;
        let w = s.chars().count() * font.w;
        self.text(&Pen { clip: right, ..*pen }, right.saturating_sub(w), y, s);
    }

    /// A grey label followed by its value, at (x, y).
    fn pair(&mut self, at: &(usize, usize), label: &str, value: &str) {
        let x = self.text(&Pen::new(&SMALL, LABEL, BACKGROUND, WIDTH), at.0, at.1, label);
        self.text(&Pen::new(&SMALL, TEXT, BACKGROUND, WIDTH), x, at.1, value);
    }

    /// Rows of the frame as big-endian RGB565 bytes, as the panel expects.
    pub fn write_be(&self, rows: std::ops::Range<usize>, out: &mut [u8]) {
        let pixels = &self.pixels[rows.start * WIDTH..rows.end * WIDTH];
        for (p, o) in pixels.iter().zip(out.chunks_exact_mut(2)) {
            o.copy_from_slice(&p.to_be_bytes());
        }
    }
}

/// Mix two RGB565 colours: alpha 0 gives `bg`, 15 gives `fg`.
fn blend(bg: u16, fg: u16, alpha: u8) -> u16 {
    let a = alpha as u32;
    let mix = |shift: u32, mask: u32| {
        let b = (bg as u32 >> shift) & mask;
        let f = (fg as u32 >> shift) & mask;
        ((b * (15 - a) + f * a + 7) / 15) << shift
    };
    (mix(11, 0x1F) | mix(5, 0x3F) | mix(0, 0x1F)) as u16
}

fn uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (days, h, m) = (s / 86400, s / 3600 % 24, s / 60 % 60);
    if days > 0 {
        format!("up {days}d {h}h")
    } else if h > 0 {
        format!("up {h}h {m:02}m")
    } else {
        format!("up {m}m {:02}s", s % 60)
    }
}

fn kb(v: u32) -> String {
    if v >= 1024 {
        format!("{:.1}M", v as f32 / 1024.0)
    } else {
        format!("{v}K")
    }
}

/// Draw the whole screen.
pub fn render(f: &mut Frame, st: &Status, sys: &System) {
    let margin = 6;
    let right = WIDTH - margin;
    f.fill(0, 0, WIDTH, HEIGHT, BACKGROUND);

    // Header: the address to connect to, and uptime
    let header_h = BIG.h + 4;
    f.fill(0, 0, WIDTH, header_h, HEADER);
    let up = uptime(st.started.elapsed());
    let up_x = right - up.len() * SMALL.w;
    // A long IP address plus a long uptime may not fit; the port is
    // always the default, so it goes first.
    let room = (up_x - SMALL.w - margin) / BIG.w;
    let address = if st.address.len() <= room {
        st.address.as_str()
    } else {
        st.address.rsplit_once(':').map_or(st.address.as_str(), |(ip, _)| ip)
    };
    f.text(&Pen::new(&BIG, TEXT, HEADER, up_x - SMALL.w), margin, 2, address);
    f.text_right(&Pen::new(&SMALL, TEXT, HEADER, WIDTH), right, 2 + (BIG.h - SMALL.h) / 2 + 1, &up);

    let line = SMALL.h + 1;
    let mut y = header_h + 4;

    // Players and signal
    let x = f.text(&Pen::new(&SMALL, LABEL, BACKGROUND, WIDTH), margin, y, "Players ");
    let mut count = st.players.len().to_string();
    if st.joining > 0 {
        count.push_str(&format!(" (+{} joining)", st.joining));
    }
    f.text(&Pen::new(&SMALL, TEXT, BACKGROUND, WIDTH), x, y, &count);
    if let Some(rssi) = sys.rssi {
        let colour = if rssi >= -67 { GOOD } else if rssi >= -80 { EVENT } else { rgb(239, 83, 80) };
        f.text_right(&Pen::new(&SMALL, colour, BACKGROUND, WIDTH), right, y, &format!("Wi-Fi {rssi} dBm"));
    }
    y += line;
    if st.players.is_empty() {
        f.text(&Pen::new(&SMALL, LABEL, BACKGROUND, right), margin + 2 * SMALL.w, y, "nobody yet");
    } else {
        let room = (right - margin) / SMALL.w - 2;
        f.text(&Pen::new(&SMALL, GOOD, BACKGROUND, right), margin + 2 * SMALL.w, y, &name_list(&st.players, room));
    }
    y += line + 2;

    // Traffic and world
    let col2 = WIDTH / 2 + 4;
    f.pair(&(margin, y), "In  ", &format!("{:.0} pkt/s", sys.packets_in_per_s));
    f.pair(&(col2, y), "Out ", &format!("{:.0} pkt/s", sys.packets_out_per_s));
    y += line;
    f.pair(&(margin, y), "Blocks ", &st.blocks_sent.to_string());
    f.pair(&(col2, y), "Edits ", &st.edits.to_string());
    y += line;
    f.pair(&(margin, y), "RAM ", &kb(sys.internal_free_kb));
    f.pair(&(col2, y), "PSRAM ", &kb(sys.psram_free_kb));
    y += line + 3;

    // Recent events
    f.fill(margin, y - 2, WIDTH - 2 * margin, 1, RULE);
    let room = (HEIGHT - y) / line;
    let skip = st.events.len().saturating_sub(room);
    for e in st.events.iter().skip(skip) {
        f.text(&Pen::new(&SMALL, EVENT, BACKGROUND, right), margin, y, e);
        y += line;
    }
}

/// As many names as fit in `max` characters, then "+N" for the rest.
fn name_list(names: &[String], max: usize) -> String {
    let all = names.join(", ");
    if all.len() <= max {
        return all;
    }
    let mut shown = String::new();
    for (i, n) in names.iter().enumerate() {
        let more = format!(", +{}", names.len() - i);
        let candidate = if shown.is_empty() { n.clone() } else { format!("{shown}, {n}") };
        // Keep room for the "+N" that the remaining names will need
        let tail = if i + 1 < names.len() { format!(", +{}", names.len() - i - 1).len() } else { 0 };
        if candidate.len() + tail > max {
            return if shown.is_empty() { format!("+{}", names.len()) } else { shown + &more };
        }
        shown = candidate;
    }
    shown
}
