use std::{
    env, fs,
    path::{Path, PathBuf},
};

use wayland_oxide_scanner::{Scanner, Side};

fn main() {
    let code = Scanner::new().generate(
        [
            Path::new("/usr/share/wayland/wayland.xml"),
            Path::new("/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml"),
            Path::new(
                "/usr/share/wayland-protocols/staging/ext-transient-seat/ext-transient-seat-v1.xml",
            ),
        ],
        Side::Client,
    );

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out.join("wayland.rs"), code).unwrap()
}
