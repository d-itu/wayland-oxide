use wayland_oxide::{Buffer, buffer::InvalidMessageSize};

fn append(buffer: &mut Buffer, bytes: &[u8]) {
    for (slot, byte) in buffer.unfilled()[..bytes.len()].iter_mut().zip(bytes) {
        slot.write(*byte);
    }
    // SAFETY: The first bytes.len() bytes were initialized above.
    unsafe { buffer.advance(bytes.len()) };
}

fn header(size: u16) -> Vec<u8> {
    [1u32, u32::from(size) << 16]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect()
}

#[test]
fn invalid_lengths_report_once_without_consuming_invalid_data() {
    for size in [0, 1, 4, 7, 9, 10, 11, 65533, 65534, 65535] {
        let mut buffer = Buffer::new();
        let invalid = header(size);
        append(&mut buffer, &header(8));
        append(&mut buffer, &invalid);
        append(&mut buffer, &header(8));
        {
            let mut iter = buffer.drain();
            assert!(iter.next().unwrap().is_ok());
            assert!(matches!(iter.next(), Some(Err(InvalidMessageSize(actual))) if actual == size));
            assert!(iter.next().is_none());
        }
        assert_eq!(&buffer.filled()[..8], invalid);
        assert_eq!(buffer.filled().len(), 16);
        assert!(
            matches!(buffer.drain().next(), Some(Err(InvalidMessageSize(actual))) if actual == size)
        );
    }
}

#[test]
fn incomplete_headers_are_retained() {
    let bytes = header(8);
    for split in 0..8 {
        let mut buffer = Buffer::new();
        append(&mut buffer, &bytes[..split]);
        assert!(buffer.drain().next().is_none());
        assert_eq!(buffer.filled(), &bytes[..split]);
        append(&mut buffer, &bytes[split..]);
        assert!(buffer.drain().next().unwrap().is_ok());
        assert!(buffer.filled().is_empty());
    }
}

#[test]
fn maximum_aligned_message_waits_for_complete_body() {
    let mut buffer = Buffer::new();
    append(&mut buffer, &header(65532));
    append(&mut buffer, &vec![0xAA; 65532 - 9]);
    assert!(buffer.drain().next().is_none());
    append(&mut buffer, &[0xBB]);
    let message = buffer.drain().next().unwrap().unwrap();
    let body = message.reader.into_inner();
    assert_eq!(body.len(), 65532 - 8);
    assert_eq!(body.last(), Some(&0xBB));
    assert!(buffer.filled().is_empty());
}
