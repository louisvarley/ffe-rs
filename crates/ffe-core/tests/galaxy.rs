use ffe_core::galaxy::{
    Tables, GALAXY_OFFSET, RECORDS, RECORD_BYTES, SPECIAL_OFFSET, TABLES_BYTES,
};

fn blob() -> Vec<u8> {
    let mut bytes = vec![0u8; TABLES_BYTES];
    for i in 0..RECORDS {
        let at = GALAXY_OFFSET + i * RECORD_BYTES;
        bytes[at..at + 4].copy_from_slice(&(0x100 + i as u32).to_le_bytes());
        bytes[at + 44] = 0x21;
        bytes[at + 10..at + 12].copy_from_slice(&(1u16 << 6).to_le_bytes());
    }
    bytes[SPECIAL_OFFSET..SPECIAL_OFFSET + 4].copy_from_slice(&0x0123_4567u32.to_le_bytes());
    bytes
}

#[test]
fn tables_are_validated_and_codes_are_classed() {
    assert!(Tables::new(&[0; 10]).is_err());
    let mut broken = blob();
    broken[GALAXY_OFFSET..GALAXY_OFFSET + 4].fill(0);
    assert!(Tables::new(&broken).is_err());
    let bytes = blob();
    let tables = Tables::new(&bytes).unwrap();
    assert_eq!(tables.index(0x100), 0);
    assert_eq!(tables.index(0x100 + 328), 328);
    // A special code matches under the two low bits of its top byte only.
    assert_eq!(tables.index(0x0123_4567), -1);
    assert_eq!(tables.index(0x4123_4567 | 0xfc00_0000), -1);
    // The centre is class -2; far away is -8.
    assert_eq!(tables.index(0x1718 | 0x1524 << 13), -2);
    assert_eq!(tables.index(0x1fff | 0x1fff << 13), -8);
}

#[test]
fn control_takes_the_greater_nibble_and_swaps_the_ranks() {
    let bytes = blob();
    let tables = Tables::new(&bytes).unwrap();
    // Imperial 2, federal 1: imperial is greater, the first rank is the minor.
    let control = tables.control(0x100, [7, 9]);
    assert_eq!(
        (control.imperial, control.federal, control.maximum),
        (2, 1, 2)
    );
    assert_eq!((control.minor_rank, control.major_rank), (7, 9));
    assert_eq!(control.allegiance, 1);
    let market = tables.market(0x100);
    assert_eq!(market.stock[32], 0x0d);
}
