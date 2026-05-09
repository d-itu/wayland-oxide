use core::{array, f32::consts::SQRT_2, mem::MaybeUninit, ptr};
use std::{io::Write as _, os::unix::net::UnixStream, thread};

use rustix::{
    fd::AsFd,
    fs::MemfdFlags,
    io::IoSlice,
    mm::{MapFlags, ProtFlags},
    net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags},
};
use tuplez::tuple;
use wayland_oxide::{
    Buffer, Fd, GenericNewId, HasEvents, Complete as _, ProtocolError, Proxy as _,
    client::{Arena, Dispatch},
};

include!(concat!(env!("OUT_DIR"), "/wayland.rs"));

#[derive(Default)]
struct Globals {
    xdg_wm_base: Option<XdgWmBase>,
    wl_compositor: Option<WlCompositor>,
    wl_shm: Option<WlShm>,
}

enum Event {
    Resize([u32; 2]),
    Close,
}

#[derive(Default)]
struct Dispatcher {
    globals: Globals,
    events: Vec<Event>,
    buffer: Vec<u8>,
    done: bool,
}

impl Dispatch<WlDisplay> for Dispatcher {
    fn dispatch(
        &mut self,
        arena: &mut Arena<Self>,
        _: WlDisplay,
        event: <WlDisplay as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_display::Event::Error(lazy) => panic!("{}", lazy.parse()?.message),
            wl_display::Event::DeleteId(lazy) => arena.drop(lazy.parse()?.id),
            wl_display::Event::_Unknown(_) => unreachable!(),
        }
    }
}
impl Dispatch<WlRegistry> for Dispatcher {
    fn dispatch(
        &mut self,
        arena: &mut Arena<Self>,
        registry: WlRegistry,
        event: <WlRegistry as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_registry::Event::Global(lazy) => {
                let wl_registry::Global {
                    name,
                    interface,
                    version,
                } = lazy.parse()?;
                match interface {
                    "xdg_wm_base" => {
                        let object = arena.new_id();
                        self.globals.xdg_wm_base = Some(object);
                        registry
                            .bind(
                                name,
                                GenericNewId {
                                    interface,
                                    version,
                                    id: object.id(),
                                },
                            )
                            .marshal_to(&mut self.buffer);
                    }
                    "wl_compositor" => {
                        let object = arena.new_id();
                        self.globals.wl_compositor = Some(object);
                        registry
                            .bind(
                                name,
                                GenericNewId {
                                    interface,
                                    version,
                                    id: object.id(),
                                },
                            )
                            .marshal_to(&mut self.buffer);
                    }
                    "wl_shm" => {
                        let object = arena.new_id();
                        self.globals.wl_shm = Some(object);
                        registry
                            .bind(
                                name,
                                GenericNewId {
                                    interface,
                                    version,
                                    id: object.id(),
                                },
                            )
                            .marshal_to(&mut self.buffer);
                    }
                    _ => {}
                }
            }
            wl_registry::Event::GlobalRemove(_) => {}
            wl_registry::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}
impl Dispatch<XdgWmBase> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Arena<Self>,
        wm_base: XdgWmBase,
        event: <XdgWmBase as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            xdg_wm_base::Event::Ping(lazy) => {
                let serial = lazy.parse()?.serial;
                wm_base.pong(serial).marshal_to(&mut self.buffer)
            }
            xdg_wm_base::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}
impl Dispatch<WlCallback> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Arena<Self>,
        _: WlCallback,
        event: <WlCallback as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_callback::Event::Done(_) => {
                self.done = true;
            }
            wl_callback::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}
impl Dispatch<WlSurface> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Arena<Self>,
        _: WlSurface,
        event: <WlSurface as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            wl_surface::Event::Enter(_) => {}
            wl_surface::Event::Leave(_) => {}
            wl_surface::Event::PreferredBufferScale(_) => {}
            wl_surface::Event::PreferredBufferTransform(_) => {}
            wl_surface::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}
impl Dispatch<XdgSurface> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Arena<Self>,
        xdg_surface: XdgSurface,
        event: <XdgSurface as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            xdg_surface::Event::Configure(lazy) => {
                let serial = lazy.parse()?.serial;
                xdg_surface
                    .ack_configure(serial)
                    .marshal_to(&mut self.buffer);
            }
            xdg_surface::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}
impl Dispatch<XdgToplevel> for Dispatcher {
    fn dispatch(
        &mut self,
        _: &mut Arena<Self>,
        _: XdgToplevel,
        event: <XdgToplevel as HasEvents>::Event,
    ) -> Result<(), ProtocolError> {
        match event {
            xdg_toplevel::Event::Configure(lazy) => {
                let e = lazy.parse()?;
                self.events
                    .push(Event::Resize([e.width, e.height].map(|x| x as _)));
            }
            xdg_toplevel::Event::Close(_) => {
                self.events.push(Event::Close);
            }
            xdg_toplevel::Event::ConfigureBounds(_) => {}
            xdg_toplevel::Event::WmCapabilities(_) => {}
            xdg_toplevel::Event::_Unknown(_) => unreachable!(),
        }
        Ok(())
    }
}

macro_rules! delegate_noop {
    ($($proxy:ident),* $(,)?) => {
        $(impl Dispatch<$proxy> for Dispatcher {
            fn dispatch(
                &mut self,
                _: &mut Arena<Self>,
                _: $proxy,
                _: <$proxy as HasEvents>::Event,
            ) -> Result<(), ProtocolError> {
                Ok(())
            }
        })*
    };
}

delegate_noop! {
    WlCompositor,
    WlShm,
    WlShmPool,
    WlBuffer,
}

fn main() {
    let mut socket = UnixStream::connect(&wayland_oxide::socket_path_from_env()).unwrap();
    let mut buffer = Buffer::new();
    let mut dispatcher = Dispatcher::default();

    let mut arena = Arena::<Dispatcher>::new();
    let display: wl_display::Proxy = arena.new_id();
    let registry = arena.new_id();
    let callback = arena.new_id();

    socket
        .write_all(
            &tuple!(display.get_registry(registry), display.sync(callback))
                .marshal_to_boxed_slice(),
        )
        .unwrap();

    loop {
        buffer.fill(&mut socket).unwrap();
        arena.dispatch(&mut dispatcher, &mut buffer).unwrap();
        if dispatcher.done {
            break;
        }
    }
    dispatcher.done = false;
    socket
        .write_all(dispatcher.buffer.drain(..).as_slice())
        .unwrap();

    let xdg_wm_base = dispatcher.globals.xdg_wm_base.unwrap();
    let wl_compositor = dispatcher.globals.wl_compositor.unwrap();

    let wl_surface = arena.new_id();
    let xdg_surface = arena.new_id();
    let toplevel = arena.new_id();

    socket
        .write_all(
            &tuple!(
                wl_compositor.create_surface(wl_surface),
                xdg_wm_base.get_xdg_surface(xdg_surface, wl_surface),
                xdg_surface.get_toplevel(toplevel),
                wl_surface.commit(),
            )
            .marshal_to_boxed_slice(),
        )
        .unwrap();

    let mut size = [0; 2];
    let mut win_buffer: Option<WlBuffer> = None;
    let (nproc, render) = if let Ok(nporc) = thread::available_parallelism() {
        (nporc.get(), &(render_mt as Render))
    } else {
        (0, &(render as Render))
    };
    'outer: loop {
        buffer.fill(&mut socket).unwrap();
        arena.dispatch(&mut dispatcher, &mut buffer).unwrap();
        socket
            .write_all(dispatcher.buffer.drain(..).as_slice())
            .unwrap();

        for event in dispatcher.events.drain(..) {
            match event {
                Event::Resize(new_size) => {
                    if new_size == size {
                        continue;
                    }
                    size = new_size;
                    let [width, height] = size;
                    let len = width * height * 4;
                    let shm = dispatcher.globals.wl_shm.unwrap();
                    let pool = arena.new_id();
                    let buffer = arena.new_id();
                    let fd = rustix::fs::memfd_create(c"", MemfdFlags::empty()).unwrap();
                    rustix::fs::ftruncate(&fd, len as _).unwrap();

                    let ptr = unsafe {
                        rustix::mm::mmap(
                            ptr::null_mut(),
                            len as _,
                            ProtFlags::READ | ProtFlags::WRITE,
                            MapFlags::SHARED,
                            &fd,
                            0,
                        )
                    }
                    .unwrap();
                    render(ptr as _, width, height, nproc);

                    let data0 = if let Some(x) = win_buffer {
                        x.destroy().marshal_to_boxed_slice()
                    } else {
                        Vec::new().into_boxed_slice()
                    };
                    win_buffer = Some(buffer);
                    let data1 = tuple!(
                        shm.create_pool(pool, Fd, len as _),
                        pool.create_buffer(
                            buffer,
                            0,
                            width as _,
                            height as _,
                            width as i32 * 4,
                            wl_shm::enums::Format::Argb8888,
                        ),
                        wl_surface.attach(Some(buffer), 0, 0),
                        wl_surface.commit(),
                        pool.destroy(),
                    )
                    .marshal_to_boxed_slice();
                    let socket_fd = socket.as_fd();
                    let mut ancillary_buffer =
                        [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
                    let mut send_ancillary_buffer = SendAncillaryBuffer::new(&mut ancillary_buffer);
                    let fds = [fd.as_fd()];
                    assert!(
                        send_ancillary_buffer.push(SendAncillaryMessage::ScmRights(fds.as_slice()))
                    );
                    rustix::net::sendmsg(
                        socket_fd,
                        &[IoSlice::new(&data0), IoSlice::new(&data1)],
                        &mut send_ancillary_buffer,
                        SendFlags::empty(),
                    )
                    .unwrap();
                }
                Event::Close => {
                    break 'outer;
                }
            }
        }
    }
}

fn mix(x: f32, y: f32, a: f32) -> f32 {
    x * (1.0 - a) + y * a
}

fn sample(uv: [f32; 2]) -> [u8; 4] {
    let weight = (uv[0] * uv[0] + uv[1] * uv[1]).sqrt() / SQRT_2;
    let color1 = [27., 49., 87.];
    let color2 = [112., 72., 92.];
    let [r, g, b] = array::from_fn(|i| mix(color1[i], color2[i], weight)).map(|x| x as u8);
    [b, g, r, 255]
}

type Render = fn(*mut u8, u32, u32, usize);

fn render_mt(ptr: *mut u8, width: u32, height: u32, nproc: usize) {
    let addr = ptr as usize;
    let mut threads = Vec::with_capacity(nproc);
    let nproc = nproc as _;
    for thread_id in 0..nproc {
        threads.push(thread::spawn(move || {
            let ptr = addr as *mut u8;
            let y_start = height * thread_id / nproc;
            let y_end = height * (thread_id + 1) / nproc;
            let mut offset = y_start * width * 4;
            for y in y_start..y_end {
                for x in 0..width {
                    let pixel: &mut [u8; 4] = unsafe { &mut *ptr.add(offset as _).cast() };
                    *pixel = sample([x as f32 / width as f32, y as f32 / height as f32]);
                    offset += 4;
                }
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
}

fn render(ptr: *mut u8, width: u32, height: u32, _: usize) {
    let mut offset = 0;
    for y in 0..height {
        for x in 0..width {
            let pixel: &mut [u8; 4] = unsafe { &mut *ptr.add(offset as _).cast() };
            *pixel = sample([x as f32 / width as f32, y as f32 / height as f32]);
            offset += 4;
        }
    }
}
