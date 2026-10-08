use std::{io::Write as _, os::unix::net::UnixStream, process};

use tuplez::tuple;
use wayland_oxide::{Buffer, Complete, MessageEnum, Object, buffer::Message};

include!(concat!(env!("OUT_DIR"), "/wayland.rs"));

const DISPLAY: WlDisplay = WlDisplay::new(Object::try_from_u32(1).unwrap());
const REGISTRY: WlRegistry = WlRegistry::new(Object::try_from_u32(2).unwrap());
const CALLBACK: WlCallback = WlCallback::new(Object::try_from_u32(3).unwrap());

fn main() {
    let mut socket = UnixStream::connect(wayland_oxide::socket_path_from_env()).unwrap();

    let msg =
        tuple!(DISPLAY.get_registry(REGISTRY), DISPLAY.sync(CALLBACK)).marshal_to_boxed_slice();
    socket.write_all(&msg).unwrap();

    let mut buffer = Buffer::new();
    loop {
        buffer.fill(&mut socket).unwrap();
        for Message {
            object,
            opcode,
            reader,
        } in buffer.drain()
        {
            match object {
                1 => match wl_display::Event::new(opcode, reader) {
                    wl_display::Event::Error(lazy) => panic!("{:?}", lazy.parse().unwrap().message),
                    wl_display::Event::DeleteId(_) => {}
                    wl_display::Event::_Unknown(_) => unreachable!(),
                },
                2 => match wl_registry::Event::new(opcode, reader) {
                    wl_registry::Event::Global(lazy) => {
                        let wl_registry::Global {
                            name,
                            interface,
                            version,
                        } = lazy.parse().unwrap();
                        println!("{} {} v{}", name, interface.to_str().unwrap(), version)
                    }
                    wl_registry::Event::GlobalRemove(_) => {}
                    wl_registry::Event::_Unknown(_) => unreachable!(),
                },
                3 => match wl_callback::Event::new(opcode, reader) {
                    wl_callback::Event::Done(_) => process::exit(0),
                    wl_callback::Event::_Unknown(_) => unreachable!(),
                },
                _ => unreachable!(),
            }
        }
    }
}
