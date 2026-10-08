use wayland_oxide::{GenericNewId, Reader, WireString, unmarshal::Error};

fn string_bytes(payload: &[u8]) -> Vec<u8> {
    let size = payload.len() + 1;
    let mut bytes = (size as u32).to_ne_bytes().to_vec();
    bytes.extend_from_slice(payload);
    bytes.push(0);
    bytes.resize(4 + size.next_multiple_of(4), 0xAA);
    bytes
}

#[test]
fn strings_borrow_bytes_and_leave_following_fields_aligned() {
    for payload in [
        b"".as_slice(),
        b"a",
        b"ab",
        b"abc",
        b"abcd",
        b"a\0b",
        b"\xff",
    ] {
        let mut bytes = string_bytes(payload);
        bytes.extend_from_slice(&0x12345678u32.to_ne_bytes());
        let mut reader = Reader::new(&bytes);
        let value: WireString = reader.read().unwrap();
        assert_eq!(value.as_bytes(), payload);
        assert_eq!(value.as_bytes().as_ptr(), bytes[4..].as_ptr());
        assert_eq!(value.to_str(), str::from_utf8(payload));
        assert_eq!(reader.read::<u32>().unwrap(), 0x12345678);
        assert!(reader.into_inner().is_empty());

        let mut reader = Reader::new(&bytes);
        let value: Option<WireString> = reader.read().unwrap();
        assert_eq!(value.unwrap().as_bytes(), payload);
        assert_eq!(reader.read::<u32>().unwrap(), 0x12345678);
        assert!(reader.into_inner().is_empty());
    }
}

#[test]
fn null_is_distinct_from_empty() {
    let mut bytes = 0u32.to_ne_bytes().to_vec();
    bytes.extend_from_slice(&42u32.to_ne_bytes());
    let mut reader = Reader::new(&bytes);
    assert_eq!(reader.read::<Option<WireString>>().unwrap(), None);
    assert_eq!(reader.read::<u32>().unwrap(), 42);
    assert!(matches!(
        Reader::new(&bytes).read::<WireString>(),
        Err(Error::InvalidString)
    ));

    let bytes = string_bytes(b"");
    assert_eq!(
        Reader::new(&bytes)
            .read::<Option<WireString>>()
            .unwrap()
            .unwrap()
            .as_bytes(),
        b""
    );
}

#[test]
fn missing_terminator_is_rejected() {
    let mut bytes = string_bytes(b"a");
    bytes[5] = b'b';
    assert!(matches!(
        Reader::new(&bytes).read::<WireString>(),
        Err(Error::InvalidString)
    ));
    assert!(matches!(
        Reader::new(&bytes).read::<Option<WireString>>(),
        Err(Error::InvalidString)
    ));
}

#[test]
fn truncated_length_payload_and_padding_are_rejected() {
    let bytes = string_bytes(b"hello");
    for end in 0..bytes.len() {
        assert!(matches!(
            Reader::new(&bytes[..end]).read::<WireString>(),
            Err(Error::NotEnoughData)
        ));
        assert!(matches!(
            Reader::new(&bytes[..end]).read::<Option<WireString>>(),
            Err(Error::NotEnoughData)
        ));
    }
    let bytes = u32::MAX.to_ne_bytes();
    assert!(matches!(
        Reader::new(&bytes).read::<WireString>(),
        Err(Error::NotEnoughData)
    ));
}

#[test]
fn generic_new_id_preserves_non_utf8_interface() {
    let mut bytes = string_bytes(b"\xff");
    bytes.extend_from_slice(&7u32.to_ne_bytes());
    bytes.extend_from_slice(&42u32.to_ne_bytes());
    let mut reader = Reader::new(&bytes);
    let value: GenericNewId<WireString> = reader.read().unwrap();
    assert_eq!(value.interface.as_bytes(), b"\xff");
    assert!(value.interface.to_str().is_err());
    assert_eq!(value.version, 7);
    assert_eq!(value.id.as_u32(), 42);
    assert!(reader.into_inner().is_empty());
}
