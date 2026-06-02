use std::{env, fs, path::PathBuf};

use wayland_oxide_scanner::{Scanner, Side};

fn main() {
    let code = Scanner::new().generate(
        [
            "/usr/share/wayland/wayland.xml",
            "/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml",
        ],
        Side::Client,
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("wayland.rs"), code).unwrap()
}
