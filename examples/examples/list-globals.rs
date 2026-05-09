use std::{io::Write as _, os::unix::net::UnixStream, process};

use tuplez::tuple;
use wayland_oxide::{
    Buffer, Complete as _, HasEvents, ProtocolError,
    client::{Dispatch, Slab},
};

include!(concat!(env!("OUT_DIR"), "/wayland.rs"));

struct Dispatcher;

impl Dispatch<WlDisplay> for Dispatcher {
    fn dispatch(
        &mut self,
        slab: &mut Slab<Self>,
        _: WlDisplay,
        event: <WlDisplay as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_display::Event::Error(lazy) => panic!("{}", lazy.parse()?.message),
            wl_display::Event::DeleteId(lazy) => slab.drop(lazy.parse()?.id),
            wl_display::Event::_Unknown(_) => unreachable!(),
        }
    }
}
impl Dispatch<WlRegistry> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Slab<Self>,
        _: WlRegistry,
        event: <WlRegistry as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_registry::Event::Global(lazy) => {
                let global = lazy.parse()?;
                println!("{} {} v{}", global.name, global.interface, global.version)
            }
            wl_registry::Event::GlobalRemove(_) => {}
            wl_registry::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}
impl Dispatch<WlCallback> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Slab<Self>,
        _: WlCallback,
        event: <WlCallback as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_callback::Event::Done(lazy) => {
                lazy.parse()?;
                process::exit(0);
            }
            wl_callback::Event::_Unknown(_) => unreachable!(),
        }
    }
}

fn main() {
    let mut socket = UnixStream::connect(&wayland_oxide::socket_path_from_env()).unwrap();
    let mut buffer = Buffer::new();
    let mut dispatcher = Dispatcher;

    let mut slab = Slab::<Dispatcher>::new();
    let display: wl_display::Proxy = slab.new_id();
    let registry = slab.new_id();
    let callback = slab.new_id();

    socket
        .write_all(
            &tuple!(display.get_registry(registry), display.sync(callback))
                .marshal_to_boxed_slice(),
        )
        .unwrap();

    loop {
        buffer.fill(&mut socket).unwrap();
        slab.dispatch(&mut dispatcher, &mut buffer).unwrap();
    }
}
