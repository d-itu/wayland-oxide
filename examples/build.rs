use std::{env, fs, path::PathBuf};

use wayland_oxide_scanner::{Scanner, Side};

fn main() {
    let code = Scanner::new().generate(
        ["./protocols/wayland.xml", "./protocols/xdg-shell.xml"],
        Side::Client,
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("wayland.rs"), code).unwrap();
    for (name, side) in [
        ("strings-client.rs", Side::Client),
        ("strings-server.rs", Side::Server),
    ] {
        let code = Scanner::new().generate(["./tests/protocols/strings.xml"], side);
        fs::write(out.join(name), code).unwrap();
    }
}
