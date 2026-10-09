use std::{error::Error as _, path::Path};
use wayland_oxide_scanner::{Error, Scanner, Side};

#[test]
fn missing_input_reports_path_and_source() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/missing.xml");
    let error = Scanner::new().generate([&path], Side::Client).unwrap_err();
    assert!(matches!(&error, Error::Read { path: actual, .. } if actual == &path));
    assert!(error.source().is_some());
    assert!(error.to_string().contains("missing.xml"));
}

#[test]
fn invalid_number_returns_parse_error_in_both_modes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/invalid-number.xml");
    for pretty in [false, true] {
        let error = Scanner::new()
            .pretty(pretty)
            .generate([&path], Side::Server)
            .unwrap_err();
        assert!(matches!(&error, Error::Parse { path: actual, .. } if actual == &path));
        assert!(error.source().is_some());
        assert!(error.to_string().contains("invalid protocol number"));
    }
}

#[test]
fn empty_input_generates_valid_rust_in_both_modes() {
    for pretty in [false, true] {
        let code = Scanner::new()
            .pretty(pretty)
            .generate([] as [&Path; 0], Side::Client)
            .unwrap();
        syn::parse_file(&code).unwrap();
    }
}
