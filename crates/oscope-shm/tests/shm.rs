//! The preview ring and the private file: messages read back whole, what is
//! refused, the ring gone with its writer, the ring consistent while it is
//! read and written at once, and the file its owner's alone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use oscope_shm::{Read, RingConfig, RingReader, RingWriter, ShmError, write_private};

/// A name of this test's own, so that tests running at once don't meet.
fn name(tag: &str) -> String {
    format!("test-{}-{tag}", std::process::id())
}

fn small(slots: u32, slot_bytes: u32) -> RingConfig {
    RingConfig { slots, slot_bytes }
}

#[test]
fn messages_read_back_whole_and_a_reader_sees_only_newer_ones() {
    let name = name("whole");
    let mut writer = RingWriter::create(&name, small(3, 4096)).unwrap();
    let mut reader = RingReader::open(&name).unwrap();
    let mut out = Vec::new();
    assert_eq!(
        reader.read(&mut out).unwrap(),
        Read::Same,
        "nothing written yet"
    );
    assert_eq!(reader.writer(), std::process::id());
    for (i, length) in [0, 1, 7, 8, 9, 1000, 4096].into_iter().enumerate() {
        let message: Vec<u8> = (0..length).map(|b| (b * 7 + i) as u8).collect();
        let number = writer.write(&message).unwrap();
        assert_eq!(number, i as u64 + 1);
        assert_eq!(reader.read(&mut out).unwrap(), Read::New(number));
        assert_eq!(out, message, "{length} bytes");
        assert_eq!(reader.read(&mut out).unwrap(), Read::Same);
    }
    // A reader opened later, and one that fell behind, get the newest.
    for message in [b"one".as_slice(), b"two", b"three", b"four"] {
        writer.write(message).unwrap();
    }
    let mut late = RingReader::open(&name).unwrap();
    for reader in [&mut reader, &mut late] {
        assert_eq!(reader.read(&mut out).unwrap(), Read::New(11));
        assert_eq!(out, b"four");
        assert_eq!(reader.written(), 11);
    }
}

#[test]
fn what_doesn_t_fit_is_refused() {
    let name = name("refused");
    let mut writer = RingWriter::create(&name, small(2, 100)).unwrap();
    // A slot holds whole words: 100 bytes become 104.
    assert_eq!(writer.slot_bytes(), 104);
    assert!(writer.write(&[1; 104]).is_ok());
    let error = writer.write(&[1; 105]).unwrap_err();
    assert!(
        matches!(
            error,
            ShmError::TooLarge {
                length: 105,
                most: 104
            }
        ),
        "{error}"
    );
    assert_eq!(writer.written(), 1);
    // A name in use, names that aren't names, rings too small, and a ring
    // nobody made.
    let taken = RingWriter::create(&name, small(2, 100)).err().unwrap();
    assert!(
        matches!(&taken, ShmError::Io { .. }) && !taken.is_not_found(),
        "{taken}"
    );
    for bad in ["", "has space", "slash/es", "back\\slash", &"x".repeat(65)] {
        let error = RingWriter::create(bad, small(2, 8)).err().unwrap();
        assert!(matches!(error, ShmError::Name(_)), "{bad:?}: {error}");
    }
    for config in [small(1, 8), small(2, 0)] {
        let error = RingWriter::create(&self::name("tiny"), config)
            .err()
            .unwrap();
        assert!(matches!(error, ShmError::Config(_)), "{error}");
    }
    let missing = RingReader::open(&self::name("nobody")).err().unwrap();
    assert!(missing.is_not_found(), "{missing}");
}

#[test]
fn the_ring_goes_with_its_writer() {
    let name = name("gone");
    let writer = RingWriter::create(&name, small(2, 64)).unwrap();
    assert!(RingReader::open(&name).is_ok());
    drop(writer);
    let error = RingReader::open(&name).err().unwrap();
    assert!(error.is_not_found(), "{error}");
    // And the name can be used again.
    assert!(RingWriter::create(&name, small(2, 64)).is_ok());
}

/// Message `number`: its length, and every word of it `number`, with a tail
/// of its low byte.
fn message(number: u64) -> Vec<u8> {
    let length = 1000 + (number * 37 % 3000) as usize;
    let mut bytes: Vec<u8> = number.to_le_bytes().repeat(length / 8 + 1);
    bytes.truncate(length);
    bytes
}

#[test]
fn the_ring_stays_consistent_while_it_is_read_and_written_at_once() {
    // Two slots, the fewest, so that the writer laps readers often; two
    // readers, each with its own view, as another process would have.
    let name = name("race");
    let mut writer = RingWriter::create(&name, small(2, 4096)).unwrap();
    let readers: Vec<RingReader> = (0..2).map(|_| RingReader::open(&name).unwrap()).collect();
    let (start, done) = (Arc::new(Barrier::new(3)), Arc::new(AtomicBool::new(false)));
    let threads: Vec<_> = readers
        .into_iter()
        .map(|mut reader| {
            let (start, done) = (Arc::clone(&start), Arc::clone(&done));
            thread::spawn(move || {
                start.wait();
                let (mut out, mut read, mut last, mut overtaken) = (Vec::new(), 0_u64, 0, 0);
                while !done.load(Ordering::Relaxed) {
                    match reader.read(&mut out) {
                        Ok(Read::New(number)) => {
                            assert!(number > last, "message {number} after {last}");
                            assert_eq!(out, message(number), "message {number}, torn");
                            (read, last) = (read + 1, number);
                        }
                        Ok(Read::Same) => {}
                        // A writer that never pauses can overtake a hundred
                        // tries in a row: refused, never returned torn.
                        Err(ShmError::Overtaken(_)) => overtaken += 1,
                        Err(error) => panic!("{error}"),
                    }
                }
                (read, reader.torn(), overtaken)
            })
        })
        .collect();
    start.wait();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline || writer.written() < 20_000 {
        let number = writer.written() + 1;
        writer.write(&message(number)).unwrap();
        thread::yield_now();
    }
    done.store(true, Ordering::Relaxed);
    for thread in threads {
        let (read, torn, overtaken) = thread.join().unwrap();
        println!("{read} read whole; {torn} torn and read again; {overtaken} overtaken");
        assert!(read > 100, "{read}");
    }
}

#[test]
fn the_private_file_is_its_owner_s_alone() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("local.token");
    std::fs::write(&path, b"an older token, anyone's").unwrap();
    write_private(&path, b"secret").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"secret");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
    #[cfg(windows)]
    {
        // icacls lists the file's entries: one, this account's, full
        // control, and nothing inherited.
        let output = std::process::Command::new("icacls")
            .arg(&path)
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        let entries: Vec<&str> = listing
            .lines()
            .take_while(|line| !line.trim().is_empty())
            .map(|line| line.trim_start_matches(path.to_str().unwrap()).trim())
            .collect();
        let me = std::env::var("USERNAME").unwrap().to_lowercase();
        assert_eq!(entries.len(), 1, "{listing}");
        assert!(entries[0].to_lowercase().contains(&me), "{listing}");
        assert!(entries[0].ends_with(":(F)"), "{listing}");
    }
}
