use core::mem::size_of;

use wayland_oxide::{Complete, Object, Reader, Unmarshal, WireEnum, unmarshal::Error};

#[allow(dead_code)]
mod client {
    include!(concat!(env!("OUT_DIR"), "/enums-client.rs"));
}

#[allow(dead_code)]
mod server {
    include!(concat!(env!("OUT_DIR"), "/enums-server.rs"));
}

#[test]
fn known_variants_decode_and_encode_in_both_bindings() {
    for (wire, client_mode, server_mode) in [
        (
            0,
            client::enum_test::enums::Mode::Off,
            server::enum_test::enums::Mode::Off,
        ),
        (
            7,
            client::enum_test::enums::Mode::_2d,
            server::enum_test::enums::Mode::_2d,
        ),
        (
            u32::MAX,
            client::enum_test::enums::Mode::Maximum,
            server::enum_test::enums::Mode::Maximum,
        ),
    ] {
        let bytes = wire.to_ne_bytes();
        let value: WireEnum<client::enum_test::enums::Mode> = Reader::new(&bytes).read().unwrap();
        assert_eq!(value.as_u32(), wire);
        assert_eq!(value.try_into_enum().unwrap(), client_mode);
        let value: WireEnum<server::enum_test::enums::Mode> = Reader::new(&bytes).read().unwrap();
        assert_eq!(value.as_u32(), wire);
        assert_eq!(value.try_into_enum().unwrap(), server_mode);
        assert_eq!(
            client::enum_test::enums::Mode::try_from(wire).unwrap(),
            client_mode
        );

        let object = Object::try_from_u32(1).unwrap();
        let mut encoded = Vec::new();
        client::EnumTest::new(object)
            .send(client_mode, client::enum_test::enums::Flags::empty())
            .marshal_to(&mut encoded);
        assert_eq!(&encoded[8..12], &bytes);
        encoded.clear();
        server::EnumTest::new(object)
            .receive(server_mode, server::enum_test::enums::Flags::empty())
            .marshal_to(&mut encoded);
        assert_eq!(&encoded[8..12], &bytes);
    }
    assert_eq!(
        size_of::<WireEnum<client::enum_test::enums::Mode>>(),
        size_of::<u32>()
    );
}

#[test]
fn unknown_values_are_checked_only_on_conversion() {
    for value in [1u32, 6, 8, 0x80000000] {
        let mut bytes = value.to_ne_bytes().to_vec();
        bytes.extend_from_slice(&3u32.to_ne_bytes());
        bytes.extend_from_slice(&42u32.to_ne_bytes());

        let mut reader = Reader::new(&bytes);
        let request: server::enum_test::Send = reader.read().unwrap();
        assert_eq!(request.mode.as_u32(), value);
        assert_eq!(request.flags.bits(), 3);
        assert_eq!(reader.read::<u32>().unwrap(), 42);
        assert!(reader.into_inner().is_empty());
        assert!(matches!(request.mode.try_into_enum(), Err(actual) if actual == value));

        let mut reader = Reader::new(&bytes);
        let event: client::enum_test::Receive = reader.read().unwrap();
        assert_eq!(event.mode.as_u32(), value);
        assert_eq!(event.flags.bits(), 3);
        assert_eq!(reader.read::<u32>().unwrap(), 42);
        assert!(reader.into_inner().is_empty());
        assert!(matches!(event.mode.try_into_enum(), Err(actual) if actual == value));
    }
}

fn reject_truncated<T: for<'a> Unmarshal<'a>>() {
    let bytes = 7u32.to_ne_bytes();
    for end in 0..4 {
        assert!(matches!(
            Reader::new(&bytes[..end]).read::<T>(),
            Err(Error::NotEnoughData)
        ));
    }
}

#[test]
fn truncated_enums_still_return_errors() {
    reject_truncated::<WireEnum<client::enum_test::enums::Mode>>();
    reject_truncated::<WireEnum<server::enum_test::enums::Mode>>();
    reject_truncated::<client::enum_test::Receive>();
    reject_truncated::<server::enum_test::Send>();
}

#[test]
fn message_decoding_preserves_unknown_flag_bits() {
    for bits in [0u32, 3, 0x80000001, u32::MAX] {
        let mut bytes = 7u32.to_ne_bytes().to_vec();
        bytes.extend_from_slice(&bits.to_ne_bytes());
        bytes.extend_from_slice(&42u32.to_ne_bytes());

        let mut reader = Reader::new(&bytes);
        let request: server::enum_test::Send = reader.read().unwrap();
        assert_eq!(
            request.mode.try_into_enum().unwrap(),
            server::enum_test::enums::Mode::_2d
        );
        assert_eq!(request.flags.bits(), bits);
        assert_eq!(reader.read::<u32>().unwrap(), 42);
        assert!(reader.into_inner().is_empty());

        let mut reader = Reader::new(&bytes);
        let event: client::enum_test::Receive = reader.read().unwrap();
        assert_eq!(
            event.mode.try_into_enum().unwrap(),
            client::enum_test::enums::Mode::_2d
        );
        assert_eq!(event.flags.bits(), bits);
        assert_eq!(reader.read::<u32>().unwrap(), 42);
        assert!(reader.into_inner().is_empty());
    }
}
