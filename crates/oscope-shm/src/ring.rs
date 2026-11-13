//! The preview ring (ADR-0015): a header, then a number of slots, each
//! holding one message behind a seqlock, so that one writer and any number
//! of readers, in any processes, share it without locks.
//!
//! Everything is a little-endian 64-bit word, read and written atomically:
//!
//! | Word | Header |
//! |---|---|
//! | 0 | `OSRING01` |
//! | 1 | the layout's version: 1 |
//! | 2 | the slots |
//! | 3 | each slot's bytes for a message, a multiple of 8 |
//! | 4 | the messages written: the newest's number, from 1 |
//! | 5 | the writer's process ID |
//! | 6, 7 | 0 |
//!
//! | Word | Each slot |
//! |---|---|
//! | 0 | its sequence: odd while the writer writes it |
//! | 1 | the number of the message it holds |
//! | 2 | the message's length in bytes |
//! | 3… | the message, 8 bytes a word |
//!
//! **Writing** (Boehm 2012, "Can seqlocks get along with programming language
//! memory models?"): the slot's sequence goes odd, a release fence, then the
//! message, then the sequence goes even with release ordering, then the
//! header's count. **Reading:** the newest slot's sequence with acquire
//! ordering, the message, an acquire fence, then the sequence again. A read
//! is whole only if the sequence was even and didn't change; otherwise the
//! writer overtook it, and it is read again.

use std::sync::atomic::{AtomicU64, Ordering, fence};

use crate::ShmError;
use crate::map::{Mapping, check_name};

const MAGIC: u64 = u64::from_le_bytes(*b"OSRING01");
const VERSION: u64 = 1;
/// The header's words.
const HEADER: usize = 8;
/// A slot's words before its message.
const SLOT_HEADER: usize = 3;
/// How many times a read is tried before the writer is said to keep
/// overtaking it.
const TRIES: u32 = 100;

/// A ring's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingConfig {
    /// How many messages it holds: at least 2.
    pub slots: u32,
    /// The largest message, in bytes.
    pub slot_bytes: u32,
}

impl Default for RingConfig {
    /// 3 slots of 8 MiB: a full-resolution JPEG of the MIChrome 20 at high
    /// quality fits.
    fn default() -> Self {
        Self {
            slots: 3,
            slot_bytes: 8 << 20,
        }
    }
}

/// The ring's layout, from its header.
#[derive(Clone, Copy)]
struct Layout {
    slots: u64,
    /// Words a slot takes, its header included.
    slot_words: usize,
    slot_bytes: usize,
}

impl Layout {
    fn new(slots: u64, slot_bytes: usize) -> Layout {
        let slot_bytes = slot_bytes.next_multiple_of(8);
        Layout {
            slots,
            slot_words: SLOT_HEADER + slot_bytes / 8,
            slot_bytes,
        }
    }

    fn bytes(self) -> usize {
        8 * (HEADER + self.slots as usize * self.slot_words)
    }

    /// The first word of the slot that message `number` goes in.
    fn slot(self, number: u64) -> usize {
        HEADER + ((number - 1) % self.slots) as usize * self.slot_words
    }
}

/// The ring's words, in the mapping.
struct Words {
    map: Mapping,
}

impl Words {
    /// Word `index`.
    fn at(&self, index: usize) -> &AtomicU64 {
        assert!(
            8 * (index + 1) <= self.map.len(),
            "word {index} is outside the ring"
        );
        // SAFETY: the mapping is page-aligned, so every word is 8-aligned;
        // the assert keeps it inside the mapping, which lives as long as
        // `self`; and the memory is only ever accessed atomically.
        unsafe { &*self.map.base().cast::<AtomicU64>().add(index) }
    }
}

/// The writing end: one per ring, in the process that made it. The ring's
/// name goes when this is dropped.
pub struct RingWriter {
    words: Words,
    layout: Layout,
    name: String,
    written: u64,
}

impl RingWriter {
    /// A new ring named `name`; refused if one by that name exists.
    pub fn create(name: &str, config: RingConfig) -> Result<RingWriter, ShmError> {
        check_name(name)?;
        if config.slots < 2 || config.slot_bytes == 0 {
            return Err(ShmError::Config(config));
        }
        let layout = Layout::new(u64::from(config.slots), config.slot_bytes as usize);
        let map = Mapping::create(name, layout.bytes()).map_err(|e| ShmError::io(name, e))?;
        let words = Words { map };
        let header = [
            MAGIC,
            VERSION,
            layout.slots,
            layout.slot_bytes as u64,
            0,
            u64::from(std::process::id()),
        ];
        for (i, value) in header.into_iter().enumerate() {
            words.at(i).store(value, Ordering::Relaxed);
        }
        fence(Ordering::Release);
        Ok(RingWriter {
            words,
            layout,
            name: name.to_owned(),
            written: 0,
        })
    }

    /// Its name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The largest message it takes.
    pub fn slot_bytes(&self) -> usize {
        self.layout.slot_bytes
    }

    /// How many messages it has written.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Writes `message` as the newest: its number.
    pub fn write(&mut self, message: &[u8]) -> Result<u64, ShmError> {
        self.write_watched(message, |_| {})
    }

    /// [`write`](Self::write), calling `watch` with the ring's words once
    /// the message is in its slot, before the slot is marked whole: for the
    /// tests.
    fn write_watched(
        &mut self,
        message: &[u8],
        watch: impl FnOnce(&Words),
    ) -> Result<u64, ShmError> {
        if message.len() > self.layout.slot_bytes {
            return Err(ShmError::TooLarge {
                length: message.len(),
                most: self.layout.slot_bytes,
            });
        }
        let number = self.written + 1;
        let slot = self.layout.slot(number);
        let sequence = self.words.at(slot);
        let even = sequence.load(Ordering::Relaxed);
        sequence.store(even + 1, Ordering::Relaxed);
        fence(Ordering::Release);
        self.words.at(slot + 1).store(number, Ordering::Relaxed);
        self.words
            .at(slot + 2)
            .store(message.len() as u64, Ordering::Relaxed);
        let (words, tail) = message.as_chunks::<8>();
        for (i, word) in words.iter().enumerate() {
            let at = slot + SLOT_HEADER + i;
            self.words
                .at(at)
                .store(u64::from_le_bytes(*word), Ordering::Relaxed);
        }
        if !tail.is_empty() {
            let mut last = [0; 8];
            last[..tail.len()].copy_from_slice(tail);
            let at = slot + SLOT_HEADER + words.len();
            self.words
                .at(at)
                .store(u64::from_le_bytes(last), Ordering::Relaxed);
        }
        watch(&self.words);
        sequence.store(even + 2, Ordering::Release);
        self.words.at(4).store(number, Ordering::Release);
        self.written = number;
        Ok(number)
    }
}

/// What a read found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// A message newer than the last one read: its number.
    New(u64),
    /// Nothing newer.
    Same,
}

/// A reading end: any number, in any process.
pub struct RingReader {
    words: Words,
    layout: Layout,
    last: u64,
    torn: u64,
}

impl RingReader {
    /// The ring named `name`.
    pub fn open(name: &str) -> Result<RingReader, ShmError> {
        check_name(name)?;
        let map = Mapping::open(name).map_err(|e| ShmError::io(name, e))?;
        let words = Words { map };
        if words.map.len() < 8 * HEADER || words.at(0).load(Ordering::Acquire) != MAGIC {
            return Err(ShmError::NotARing(name.to_owned()));
        }
        let version = words.at(1).load(Ordering::Relaxed);
        if version != VERSION {
            return Err(ShmError::Version(version));
        }
        let slots = words.at(2).load(Ordering::Relaxed);
        let slot_bytes = words.at(3).load(Ordering::Relaxed) as usize;
        let layout = Layout::new(slots, slot_bytes);
        if slots < 2 || layout.bytes() > words.map.len() {
            return Err(ShmError::NotARing(name.to_owned()));
        }
        Ok(RingReader {
            words,
            layout,
            last: 0,
            torn: 0,
        })
    }

    /// The writer's process ID.
    pub fn writer(&self) -> u32 {
        self.words.at(5).load(Ordering::Relaxed) as u32
    }

    /// How many messages the writer has written.
    pub fn written(&self) -> u64 {
        self.words.at(4).load(Ordering::Acquire)
    }

    /// How many reads the writer has overtaken, and were read again.
    pub fn torn(&self) -> u64 {
        self.torn
    }

    /// Copies the newest message into `out`, if it is newer than the last
    /// one read.
    pub fn read(&mut self, out: &mut Vec<u8>) -> Result<Read, ShmError> {
        for _ in 0..TRIES {
            let newest = self.words.at(4).load(Ordering::Acquire);
            if newest == 0 || newest == self.last {
                return Ok(Read::Same);
            }
            let slot = self.layout.slot(newest);
            let before = self.words.at(slot).load(Ordering::Acquire);
            if before % 2 == 1 {
                self.torn += 1;
                continue;
            }
            let number = self.words.at(slot + 1).load(Ordering::Relaxed);
            let length = (self.words.at(slot + 2).load(Ordering::Relaxed) as usize)
                .min(self.layout.slot_bytes);
            out.clear();
            for i in 0..length.div_ceil(8) {
                let word = self
                    .words
                    .at(slot + SLOT_HEADER + i)
                    .load(Ordering::Relaxed);
                out.extend_from_slice(&word.to_le_bytes());
            }
            out.truncate(length);
            fence(Ordering::Acquire);
            if self.words.at(slot).load(Ordering::Relaxed) != before {
                self.torn += 1;
                continue;
            }
            // Whole: the slot may hold a message newer than `newest`, if the
            // writer went round the ring meanwhile.
            self.last = number;
            return Ok(Read::New(number));
        }
        Err(ShmError::Overtaken(TRIES))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_marked_while_it_is_written() {
        let name = format!("unit-{}-marked", std::process::id());
        let config = RingConfig {
            slots: 2,
            slot_bytes: 64,
        };
        let mut writer = RingWriter::create(&name, config).unwrap();
        writer.write(b"one").unwrap();
        writer.write(b"two").unwrap();
        // The third message goes in the first's slot, whose sequence is 2.
        let slot = writer.layout.slot(3);
        let mut seen = None;
        writer
            .write_watched(b"three", |words| {
                seen = Some(words.at(slot).load(Ordering::Relaxed))
            })
            .unwrap();
        assert_eq!(seen, Some(3), "odd while it is written");
        assert_eq!(
            writer.words.at(slot).load(Ordering::Relaxed),
            4,
            "even once whole"
        );
    }

    #[test]
    fn a_slot_being_written_is_never_read() {
        let name = format!("unit-{}-half", std::process::id());
        let config = RingConfig {
            slots: 2,
            slot_bytes: 64,
        };
        let mut writer = RingWriter::create(&name, config).unwrap();
        writer.write(b"whole").unwrap();
        // The writer, stopped between marking the slot and finishing it.
        let slot = writer.layout.slot(1);
        let sequence = writer.words.at(slot);
        sequence.fetch_add(1, Ordering::Relaxed);
        let half = u64::from_le_bytes(*b"halfdone");
        writer
            .words
            .at(slot + SLOT_HEADER)
            .store(half, Ordering::Relaxed);
        let (mut reader, mut out) = (RingReader::open(&name).unwrap(), Vec::new());
        let refused = reader.read(&mut out);
        assert!(
            matches!(refused, Err(ShmError::Overtaken(TRIES))),
            "{refused:?}"
        );
        assert_eq!(reader.torn(), u64::from(TRIES));
        // Finished, it is read.
        sequence.fetch_add(1, Ordering::Relaxed);
        assert_eq!(reader.read(&mut out).unwrap(), Read::New(1));
        assert_eq!(out, b"halfd");
    }
}
