//! What the board's RGB LED shows: a slow green
//! "breathing" while the server runs (a little quicker with players on),
//! flashes when players join (blue) or leave (purple) and on chat (white),
//! red flashes on problems, and amber blinking while looking for Wi-Fi.
//!
//! Pure logic, so it can be checked on a PC; lcd.rs drives the LED.

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::time::Duration;

/// The LED is very bright: it never goes above this share of full power.
pub const MAX_BRIGHTNESS: f32 = 0.6;
/// The dimmest point of a breath, as a share of the brightest.
const BREATH_FLOOR: f32 = 0.08;
const BREATH_IDLE: Duration = Duration::from_millis(4000);
const BREATH_PLAYERS: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

const OFF: Rgb = Rgb(0.0, 0.0, 0.0);
const GREEN: Rgb = Rgb(0.0, 1.0, 0.0);
const BLUE: Rgb = Rgb(0.0, 0.0, 1.0);
const PURPLE: Rgb = Rgb(0.6, 0.0, 1.0);
const WHITE: Rgb = Rgb(1.0, 1.0, 1.0);
const RED: Rgb = Rgb(1.0, 0.0, 0.0);
const AMBER: Rgb = Rgb(1.0, 0.45, 0.0);

/// Things worth a flash.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Join,
    Leave,
    Chat,
    Problem,
}

/// What the server is doing, for the LED's resting pattern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// Not on Wi-Fi yet (or lost it).
    NoWifi,
    Running { players: usize },
}

pub struct Indicator {
    /// Flash steps still to show: colour (OFF for a gap) and how long.
    flashes: VecDeque<(Rgb, Duration)>,
    /// Time into the current flash step.
    in_step: Duration,
    /// Time into the resting pattern.
    clock: Duration,
}

impl Indicator {
    pub fn new() -> Self {
        Self { flashes: VecDeque::new(), in_step: Duration::ZERO, clock: Duration::ZERO }
    }

    pub fn event(&mut self, e: Event) {
        let ms = Duration::from_millis;
        // A dark gap first, so a flash stands out from the breathing.
        let steps: &[(Rgb, u64)] = match e {
            Event::Join => &[(OFF, 150), (BLUE, 120), (OFF, 120), (BLUE, 120), (OFF, 250)],
            Event::Leave => &[(OFF, 150), (PURPLE, 120), (OFF, 120), (PURPLE, 120), (OFF, 250)],
            Event::Chat => &[(OFF, 150), (WHITE, 150), (OFF, 250)],
            Event::Problem => &[(OFF, 150), (RED, 100), (OFF, 100), (RED, 100), (OFF, 100), (RED, 100), (OFF, 250)],
        };
        // Don't let a burst (say, a busy chat) queue up minutes of flashing.
        if self.flashes.len() < 40 {
            self.flashes.extend(steps.iter().map(|&(c, t)| (c, ms(t))));
        }
    }

    /// Move on by `dt` and give the colour to show (0..=1 per channel, as
    /// seen; `to_byte` turns that into the LED's power level).
    pub fn colour(&mut self, dt: Duration, state: State) -> Rgb {
        self.clock += dt;
        let raw = if let Some(&(c, len)) = self.flashes.front() {
            self.in_step += dt;
            if self.in_step >= len {
                self.in_step = Duration::ZERO;
                self.flashes.pop_front();
            }
            c
        } else {
            self.resting(state)
        };
        raw
    }

    fn resting(&self, state: State) -> Rgb {
        match state {
            State::NoWifi => {
                // Blink: half a second on, half off.
                if self.clock.as_millis() % 1000 < 500 { AMBER } else { OFF }
            }
            State::Running { players } => {
                let period = if players > 0 { BREATH_PLAYERS } else { BREATH_IDLE };
                let phase = (self.clock.as_secs_f32() / period.as_secs_f32()).fract();
                // A smooth rise and fall, squared so the dark part lingers
                // the way a breathing light usually looks.
                let s = 0.5 - 0.5 * (2.0 * PI * phase).cos();
                let level = BREATH_FLOOR + (1.0 - BREATH_FLOOR) * s * s;
                Rgb(GREEN.0 * level, GREEN.1 * level, GREEN.2 * level)
            }
        }
    }
}

/// An LED level (0..=1, as seen) as the 8-bit power value the LED takes:
/// gamma-corrected so equal steps look like equal changes, and capped at
/// MAX_BRIGHTNESS of full power.
pub fn to_byte(level: f32) -> u8 {
    (level.clamp(0.0, 1.0).powf(2.2) * 255.0 * MAX_BRIGHTNESS).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_too_bright() {
        assert_eq!(to_byte(1.0), 153);
        assert_eq!(to_byte(0.0), 0);
    }
}
