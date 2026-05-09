#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::{
    fmt::{self, Debug, Display, Formatter},
    mem,
    num::NonZeroU32,
};
#[cfg(feature = "std")]
use std::{env, ffi::c_int, os::unix::ffi::OsStrExt as _, path::PathBuf};

pub use bitflags::bitflags;
use thiserror::Error;

pub use crate::{
    buffer::Buffer,
    marshal::{Marshal, MarshalExt, Writer},
    unmarshal::{Lazy, Reader, Unmarshal},
};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
/// Signed 24.8 decimal numbers.
pub struct Fixed(i32);

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fd;

impl Debug for Fixed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.as_f64(), f)
    }
}

impl Fixed {
    pub const fn from_bits(value: i32) -> Self {
        Self(value)
    }
    pub const fn as_bits(self) -> i32 {
        self.0
    }
    pub const fn from_f64(value: f64) -> Self {
        Self((value * 256.0) as i32)
    }
    pub const fn as_f64(self) -> f64 {
        self.0 as f64 / 256.0
    }
}

impl From<f64> for Fixed {
    fn from(value: f64) -> Self {
        Self::from_f64(value)
    }
}

impl Into<f64> for Fixed {
    fn into(self) -> f64 {
        self.as_f64()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Object(NonZeroU32);

impl Debug for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Object {
    pub fn as_u32(self) -> u32 {
        self.0.get()
    }
    pub unsafe fn from_u32(x: u32) -> Self {
        Self(unsafe { NonZeroU32::new_unchecked(x) })
    }
}

pub type NewId = Object;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenericNewId<'a> {
    pub interface: &'a str,
    pub version: u32,
    pub id: Object,
}

pub unsafe trait Opcode {}
unsafe impl Opcode for u16 {}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Header<P = Object, O = u16> {
    pub object: P,
    pub opcode: O,
    pub size: u16,
}

pub trait MessageBody {
    type Opcode: Opcode;
    const OPCODE: Self::Opcode;
}

pub trait MessageEnum<'a> {
    fn new(opcode: u16, reader: Reader<'a>) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Message<P, T: MessageBody> {
    pub header: Header<P, T::Opcode>,
    pub body: T,
}

impl<P, T: MessageBody + Marshal> Message<P, T> {
    pub fn new(id: P, body: T) -> Self {
        Self {
            header: Header {
                object: id,
                opcode: T::OPCODE,
                size: body.calc_size() as u16 + 8,
            },
            body,
        }
    }
}

const fn align_padding(align: usize, size: usize) -> usize {
    (align - (size % align)) % align
}

const fn aligned(align: usize, size: usize) -> usize {
    size + align_padding(align, size)
}

pub unsafe trait Trivial {}
unsafe impl Trivial for i32 {}
unsafe impl Trivial for u32 {}
unsafe impl Trivial for Fixed {}
unsafe impl Trivial for Object {}
unsafe impl Trivial for Option<Object> {}
unsafe impl<P: Trivial, T: Opcode> Trivial for Header<P, T> {}

unsafe impl<T: Trivial> Marshal for T {
    fn calc_size(&self) -> usize {
        mem::size_of::<Self>()
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        unsafe { writer.write_trivial(self) };
    }
}
unsafe impl<T: Trivial> Unmarshal<'_> for T {
    fn unmarshal(reader: &mut Reader) -> unmarshal::Result<Self> {
        reader.read_trivial()
    }
}

unsafe impl Marshal for &str {
    fn calc_size(&self) -> usize {
        4 + aligned(4, self.len() + 1)
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        let size = self.len() + 1;
        unsafe {
            writer.write(&(size as u32));
            writer.write_bytes(self.as_bytes());
            writer.write_trivial(&0u8);
            writer.pad(align_padding(4, size))
        }
    }
}
unsafe impl<'a> Unmarshal<'a> for &'a str {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        let size = reader.read::<u32>()? as usize;
        let res = unsafe { str::from_utf8_unchecked(reader.read_bytes(size - 1)?) };
        reader.skip(1 + align_padding(4, size))?;
        Ok(res)
    }
}

unsafe impl Marshal for Option<&str> {
    fn calc_size(&self) -> usize {
        if let Some(value) = self {
            value.calc_size()
        } else {
            4
        }
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        if let Some(value) = self {
            unsafe { writer.write(value) }
        } else {
            unsafe { writer.write(&0u32) }
        }
    }
}
unsafe impl<'a> Unmarshal<'a> for Option<&'a str> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        let size = reader.read::<u32>()? as usize;
        Ok((size != 0).then_some(unsafe { str::from_utf8_unchecked(reader.read_bytes(size)?) }))
    }
}

unsafe impl<'a> Marshal for GenericNewId<'a> {
    fn calc_size(&self) -> usize {
        self.interface.calc_size() + 8
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        unsafe {
            writer.write(&self.interface);
            writer.write(&self.version);
            writer.write(&self.id);
        }
    }
}
unsafe impl<'a> Unmarshal<'a> for GenericNewId<'a> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        Ok(Self {
            interface: reader.read()?,
            version: reader.read()?,
            id: reader.read()?,
        })
    }
}

unsafe impl Marshal for &[u8] {
    fn calc_size(&self) -> usize {
        4 + aligned(4, self.len())
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        unsafe {
            writer.write(&(self.len() as u32));
            writer.write_bytes(self);
            writer.pad(align_padding(4, self.len()))
        }
    }
}
unsafe impl<'a> Unmarshal<'a> for &'a [u8] {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        let size = reader.read::<u32>()? as usize;
        let res = reader.read_bytes(size)?;
        reader.skip(align_padding(4, size))?;
        Ok(res)
    }
}

unsafe impl Marshal for Fd {
    fn calc_size(&self) -> usize {
        0
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        _ = writer;
    }
}
unsafe impl Unmarshal<'_> for Fd {
    fn unmarshal(reader: &mut Reader) -> unmarshal::Result<Self> {
        _ = reader;
        Ok(Self)
    }
}

unsafe impl<P: Trivial, T: MessageBody + Marshal> Marshal for Message<P, T> {
    fn calc_size(&self) -> usize {
        self.header.calc_size() + self.body.calc_size()
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        unsafe {
            writer.write(&self.header);
            writer.write(&self.body);
        }
    }
}
unsafe impl<'a, P: Proxy, T: MessageBody + Unmarshal<'a>> Unmarshal<'a> for Message<P, T> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        Ok(Self {
            header: reader.read()?,
            body: reader.read()?,
        })
    }
}

pub unsafe trait HasRequests<'a> {
    type Request: MessageEnum<'a>;
}
pub unsafe trait HasEvents<'a> {
    type Event: MessageEnum<'a>;
}
pub unsafe trait Proxy: Copy + Debug {
    fn new(object: Object) -> Self;
    fn id(self) -> Object;

    const REQUEST_IS_DESTRUCTOR: &'static [bool];
    const EVENT_IS_DESTRUCTOR: &'static [bool];

    const INTERFACE: Interface;
}
unsafe impl<P: Proxy> Trivial for P {}
unsafe impl<P: Proxy> Trivial for Option<P> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Interface {
    pub version: u32,
    pub name: &'static str,
}

impl Display for Interface {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}v_{}", self.name, self.version)
    }
}

#[cfg(feature = "std")]
pub fn socket_path_from_env() -> PathBuf {
    unsafe extern "C" {
        fn getuid() -> c_int;
    }
    let wayland_display = env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    if wayland_display.as_bytes().starts_with(b"/") {
        PathBuf::from(wayland_display)
    } else {
        let xdg_runtime_dir = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| format!("/run/user/{}", unsafe { getuid() }).into());
        PathBuf::from(xdg_runtime_dir).join(wayland_display)
    }
}

const ID_SPACE_MASK: u32 = 0xff00_0000;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("IDs not densely packed: {0:?}")]
    IdNotDense(Object),
    #[error("ID overflow: {0:?}")]
    IdOverflow(Object),
    #[error("ID duplicated: {0:?}")]
    DuplicateId(Object),
    #[error(transparent)]
    Unmarshal(#[from] unmarshal::Error),
    #[error("unknown object: {0:?}")]
    UnknownObject(Object),
    #[error("unknown opcode {opcode} for {interface}(opcode)")]
    UnknownOpcode {
        object: Object,
        interface: Interface,
        opcode: u16,
    },
}

pub mod buffer;
pub mod client;
pub mod marshal;
pub mod unmarshal;
