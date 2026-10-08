use core::mem::MaybeUninit;

use wayland_oxide::{GenericNewId, Marshal, Object, Reader, WireString, Writer};

fn encode(message: impl Marshal) -> Vec<u8> {
    let mut bytes = vec![MaybeUninit::new(0); message.calc_size()];
    // The buffer is fully initialized and sized for this generated message.
    unsafe { Writer::new(&mut bytes).write(&message) };
    bytes
        .into_iter()
        .map(|byte| unsafe { byte.assume_init() })
        .collect()
}

#[allow(dead_code)]
mod client {
    include!(concat!(env!("OUT_DIR"), "/strings-client.rs"));
}

#[allow(dead_code)]
mod server {
    include!(concat!(env!("OUT_DIR"), "/strings-server.rs"));
}

fn check(
    text: WireString<'_>,
    optional: Option<WireString<'_>>,
    id: GenericNewId<WireString<'_>>,
    expected: Option<&str>,
    marker: u32,
) {
    assert_eq!(text.as_bytes(), b"hello");
    assert_eq!(optional.map(|value| value.to_str().unwrap()), expected);
    assert_eq!(id.interface.to_str().unwrap(), "test_interface");
    assert_eq!(id.version, 2);
    assert_eq!(id.id.as_u32(), 3);
    assert_eq!(marker, 42);
}

#[test]
fn generated_strings_round_trip_in_both_directions() {
    let object = Object::try_from_u32(1).unwrap();
    let id = GenericNewId {
        interface: "test_interface",
        version: 2,
        id: Object::try_from_u32(3).unwrap(),
    };
    for optional in [None, Some(""), Some("a"), Some("ab"), Some("abc")] {
        let bytes = encode(client::StringTest::new(object).send("hello", optional, id, 42));
        let mut reader = Reader::new(&bytes[8..]);
        let value: server::string_test::Send = reader.read().unwrap();
        check(value.text, value.optional, value.id, optional, value.marker);
        assert!(reader.into_inner().is_empty());

        let bytes = encode(server::StringTest::new(object).receive("hello", optional, id, 42));
        let mut reader = Reader::new(&bytes[8..]);
        let value: client::string_test::Receive = reader.read().unwrap();
        check(value.text, value.optional, value.id, optional, value.marker);
        assert!(reader.into_inner().is_empty());
    }
}
