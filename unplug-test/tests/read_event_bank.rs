use anyhow::Result;
use log::info;
use std::io::BufReader;
use unplug::audio::sem::{Action, Command, Event, EventBank};
use unplug::common::ReadFrom;
use unplug::dvd::OpenFile;
use unplug_test as common;

const EVENT_BANK_PATH: &str = "qp/sfx_sample.sem";

#[test]
fn test_read_event_bank() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    info!("Reading {}", EVENT_BANK_PATH);
    let mut reader = BufReader::new(iso.open_file_at(EVENT_BANK_PATH)?);
    let sem = EventBank::read_from(&mut reader)?;
    assert_eq!(sem.group_bases, EXPECTED_GROUP_BASES);
    assert_eq!(sem.events.len(), 1120);
    assert_eq!(
        sem.events[0x2d6], // randomly-chosen event with 3 actions
        Event {
            actions: vec![
                Action { command: Command::Sound, delay: 0, data: 0x02d1 },
                Action { command: Command::Unk6, delay: 0, data: 0x99 },
                Action { command: Command::End1, delay: 0, data: 0 },
            ],
        }
    );

    Ok(())
}

/// The expected base indexes for each group in the bank.
const EXPECTED_GROUP_BASES: &[u32] = &[
    0x0000, 0x01ba, 0x01e8, 0x020b, 0x0231, 0x0259, 0x0283, 0x02a7, 0x02d2, 0x02fe, 0x031f, 0x0320,
    0x0326, 0x0338, 0x0352, 0x0363, 0x038b, 0x03ab, 0x03e1, 0x040e, 0x0416, 0x041e, 0x0420, 0x0446,
    0x0450,
];
