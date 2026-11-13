//! How a client's preview adapts to its link (D6).

/// The quality each step down takes off.
const QUALITY_STEP: u8 = 15;
/// The steps down that lower the quality; those after them lower the size.
const QUALITY_STEPS: u32 = 4;
/// The lowest quality adapting goes to, unless the client asks for less.
const LOWEST_QUALITY: u8 = 30;
/// The share of the size each step down keeps.
const SIZE_STEP: f32 = 0.75;
/// The smallest share of the viewport adapting goes to.
const SMALLEST: f32 = 0.25;
/// The deepest step: the size's floor reached.
const DEEPEST: u32 = QUALITY_STEPS + 5;
/// Messages counted before deciding.
const WINDOW: u32 = 10;
/// Drops among them that take a step down at once.
const TOO_MANY: u32 = 3;
/// Windows in a row without a drop that take a step back up.
const CALM: u32 = 5;

/// How a client's preview adapts to its link. While the client keeps
/// dropping messages (3 of 10), each step down takes 15 off its quality,
/// down to 30, then a quarter off its size, down to a quarter of its
/// viewport. After 50 messages in a row without a drop, it takes a step
/// back up: size first, then quality.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adaptation {
    step: u32,
    sent: u32,
    dropped: u32,
    calm: u32,
}

impl Adaptation {
    /// Counts a message sent, and whether it replaced one the client
    /// hadn't taken.
    pub fn record(&mut self, dropped: bool) {
        self.sent += 1;
        self.dropped += u32::from(dropped);
        if self.dropped >= TOO_MANY {
            self.step = (self.step + 1).min(DEEPEST);
            self.calm = 0;
        } else if self.sent >= WINDOW {
            self.calm = if self.dropped == 0 { self.calm + 1 } else { 0 };
            if self.calm >= CALM {
                self.step = self.step.saturating_sub(1);
                self.calm = 0;
            }
        } else {
            return;
        }
        self.sent = 0;
        self.dropped = 0;
    }

    /// How many steps down it is: 0 when the link keeps up.
    pub fn step(&self) -> u32 {
        self.step
    }

    /// The quality to send, for a client that asks for `asked`.
    pub fn quality(&self, asked: u8) -> u8 {
        let cut = QUALITY_STEP * self.step.min(QUALITY_STEPS) as u8;
        asked.saturating_sub(cut).max(LOWEST_QUALITY.min(asked))
    }

    /// The share of its viewport to send.
    pub fn scale(&self) -> f32 {
        let steps = self.step.saturating_sub(QUALITY_STEPS) as i32;
        SIZE_STEP.powi(steps).max(SMALLEST)
    }
}
