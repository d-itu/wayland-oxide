use wayland_oxide::{Buffer, MessageEnum};

#[allow(dead_code)]
mod client {
    include!(concat!(env!("OUT_DIR"), "/enums-client.rs"));
}

#[test]
fn buffer_keeps_unknown_opcodes_available_for_dispatch() {
    let mut bytes = 1u32.to_ne_bytes().to_vec();
    bytes.extend_from_slice(&((8u32 << 16) | u32::from(u16::MAX)).to_ne_bytes());
    let mut buffer = Buffer::new();
    for (slot, byte) in buffer.unfilled().iter_mut().zip(&bytes) {
        slot.write(*byte);
    }
    // All bytes being advanced over were initialized above.
    unsafe { buffer.advance(bytes.len()) };
    let mut iter = buffer.drain();
    let message = iter.next().unwrap().unwrap();
    assert_eq!(message.object, 1);
    assert_eq!(message.opcode, u16::MAX);
    assert!(message.reader.into_inner().is_empty());
    assert!(matches!(
        client::enum_test::Event::new(message.opcode, message.reader),
        client::enum_test::Event::_Unknown(_)
    ));
    assert!(iter.next().is_none());
}
