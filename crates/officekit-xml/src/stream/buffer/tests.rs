use std::io::Write as _;

use super::{CHUNK, PartBuffer};

#[test]
fn the_bytes_come_back_in_order_and_exactly_as_long_as_written() {
    let mut buffer = PartBuffer::default();
    assert!(buffer.is_empty());
    let pieces: Vec<Vec<u8>> = (0_u8..7)
        .map(|seed| vec![seed; CHUNK / 3 + usize::from(seed)])
        .collect();
    for piece in &pieces {
        buffer.write_all(piece).unwrap();
    }
    let expected = pieces.concat();
    assert_eq!(buffer.len(), expected.len());
    let bytes = buffer.into_bytes();
    assert_eq!(bytes.capacity(), expected.len());
    assert_eq!(bytes, expected);
}

#[test]
fn a_write_larger_than_a_chunk_fills_several() {
    let mut buffer = PartBuffer::default();
    let large: Vec<u8> = (0..CHUNK * 2 + 5)
        .map(|index| u8::try_from(index % 251).unwrap())
        .collect();
    buffer.write_all(b"<").unwrap();
    buffer.write_all(&large).unwrap();
    assert_eq!(buffer.chunks.len(), 3);
    assert!(buffer.chunks.iter().all(|chunk| chunk.len() <= CHUNK));
    let mut expected = b"<".to_vec();
    expected.extend_from_slice(&large);
    assert_eq!(buffer.into_bytes(), expected);
}
