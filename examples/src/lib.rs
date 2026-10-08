use std::{io, os::unix::net::UnixStream};

use rustix::net::{RecvFlags, recv};
use wayland_oxide::Buffer;

/// Receive bytes from the socket and append them to the message buffer.
pub fn receive(socket: &UnixStream, buffer: &mut Buffer) -> io::Result<usize> {
    let (_, n) = recv(socket, buffer.unfilled(), RecvFlags::empty())?;
    // SAFETY: recv initialized the first n bytes and, without MSG_TRUNC,
    // n does not exceed the supplied buffer's length.
    unsafe { buffer.advance(n) };
    Ok(n)
}
