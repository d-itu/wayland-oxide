#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
//! Pure rust wayland implementation

extern crate alloc;

use core::{
    fmt::{self, Debug, Display, Formatter},
    mem,
    num::NonZeroU32,
    str::{self, Utf8Error},
};
#[cfg(feature = "std")]
use std::{env, ffi::c_int, os::unix::ffi::OsStrExt as _, path::PathBuf};

pub use bitflags::bitflags;

pub use crate::{
    buffer::Buffer,
    marshal::{Complete, Marshal, Writer},
    unmarshal::{Lazy, Reader, Unmarshal},
};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
/// Signed 24.8 decimal numbers.
pub struct Fixed(i32);

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Unix fd. This type does not encode into message buffer. It hints you where an fd is needed.
pub struct Fd;

impl Debug for Fixed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.as_f64(), f)
    }
}

impl Fixed {
    /// Create a fixed from it's underlying binary representation.
    pub const fn from_bits(value: i32) -> Self {
        Self(value)
    }
    /// Convert to underlying binary representation.
    pub const fn as_bits(self) -> i32 {
        self.0
    }
    /// Convert from f64.
    pub const fn from_f64(value: f64) -> Self {
        Self((value * 256.0) as i32)
    }
    /// Convert to f64.
    pub const fn as_f64(self) -> f64 {
        self.0 as f64 / 256.0
    }
}

impl From<f64> for Fixed {
    fn from(value: f64) -> Self {
        Self::from_f64(value)
    }
}

impl From<Fixed> for f64 {
    fn from(val: Fixed) -> Self {
        val.as_f64()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
/// 32-bit object ID. A null value is represented with an ID of 0.
pub struct Object(NonZeroU32);

impl Debug for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

/// Side of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    #[allow(missing_docs)]
    Client,
    #[allow(missing_docs)]
    Server,
}

const ID_SPACE_MASK: u32 = 0xff00_0000;

impl Object {
    /// Get underlying u32 data.
    pub const fn as_u32(self) -> u32 {
        self.0.get()
    }
    /// Construct from u32. Input must not be 0
    /// # Safety
    ///
    /// The value must not be zero.
    pub const unsafe fn from_u32(value: u32) -> Self {
        Self(unsafe { NonZeroU32::new_unchecked(value) })
    }
    /// Try to create an object from u32, return None if input is 0
    pub const fn try_from_u32(x: u32) -> Option<Self> {
        match NonZeroU32::new(x) {
            Some(x) => Some(Self(x)),
            None => None,
        }
    }
    /// Get side of an object.
    pub const fn side(self) -> Side {
        if self.as_u32() >= ID_SPACE_MASK {
            Side::Server
        } else {
            Side::Client
        }
    }
}
impl TryFrom<u32> for Object {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_from_u32(value).ok_or(())
    }
}

unsafe impl TrivialMarshal for Object {}
impl Unmarshal<'_> for Object {
    fn unmarshal(reader: &mut Reader<'_>) -> unmarshal::Result<Self> {
        let id: u32 = reader
            .read_trivial()
            .ok_or(unmarshal::Error::NotEnoughData)?;
        NonZeroU32::new(id)
            .map(Object)
            .ok_or(unmarshal::Error::NullObject)
    }
}

/// Id of a new object whose interface is inferred from the xml
pub type NewId = Object;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A new id without specified interface
pub struct GenericNewId<S> {
    /// Interface name: `&str` when encoding, [`WireString`] when decoding.
    pub interface: S,
    /// Interface version
    pub version: u32,
    /// new_id value
    pub id: Object,
}

#[doc(hidden)]
pub unsafe trait Opcode {}
unsafe impl Opcode for u16 {}

#[repr(C)]
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Header<P = Object, O = u16> {
    pub object: P,
    pub opcode: O,
    pub size: u16,
}
impl<'a, P: Unmarshal<'a>, O: Opcode> Unmarshal<'a> for Header<P, O> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        Ok(Self {
            object: reader.read()?,
            opcode: reader
                .read_trivial()
                .ok_or(unmarshal::Error::NotEnoughData)?,
            size: reader
                .read_trivial()
                .ok_or(unmarshal::Error::NotEnoughData)?,
        })
    }
}

#[doc(hidden)]
pub trait MessageBody {
    type Opcode: Opcode;
    const OPCODE: Self::Opcode;
}

/// Enum of all messages
pub trait MessageEnum<'a> {
    /// Create a message from opcode and reader
    fn new(opcode: u16, reader: Reader<'a>) -> Self;
}

#[doc(hidden)]
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

#[doc(hidden)]
pub unsafe trait TrivialMarshal {}
unsafe impl<T: Trivial> TrivialMarshal for T {}
#[doc(hidden)]
pub unsafe trait TrivialUnmarshal {}
unsafe impl<T: Trivial> TrivialUnmarshal for T {}
#[doc(hidden)]
pub unsafe trait Trivial {}
unsafe impl Trivial for i32 {}
unsafe impl Trivial for u32 {}
unsafe impl Trivial for Fixed {}
unsafe impl Trivial for Option<Object> {}
unsafe impl<P, T: Opcode> TrivialMarshal for Header<P, T> {}

impl<T: TrivialMarshal> Marshal for T {
    fn calc_size(&self) -> usize {
        mem::size_of::<Self>()
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        unsafe { writer.write_trivial(self) };
    }
}
impl<T: TrivialUnmarshal> Unmarshal<'_> for T {
    fn unmarshal(reader: &mut Reader) -> unmarshal::Result<Self> {
        reader.read_trivial().ok_or(unmarshal::Error::NotEnoughData)
    }
}

/// A borrowed wire string, excluding its terminating NUL and alignment padding.
///
/// Decoding checks the terminator but does not validate UTF-8 or reject interior
/// NUL bytes. Use [`Self::to_str`] to validate UTF-8 when text is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WireString<'a> {
    bytes: &'a [u8],
}

impl<'a> WireString<'a> {
    /// Return the payload bytes without the terminating NUL or padding.
    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Validate UTF-8 and borrow the payload as text.
    pub fn to_str(self) -> Result<&'a str, Utf8Error> {
        str::from_utf8(self.bytes)
    }

    fn read_payload(reader: &mut Reader<'a>, size: usize) -> unmarshal::Result<Self> {
        let bytes = reader
            .read_bytes(size)
            .ok_or(unmarshal::Error::NotEnoughData)?;
        let Some((&0, bytes)) = bytes.split_last() else {
            return Err(unmarshal::Error::InvalidString);
        };
        reader
            .skip(align_padding(4, size))
            .ok_or(unmarshal::Error::NotEnoughData)?;
        Ok(Self { bytes })
    }
}

impl Marshal for &str {
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
impl<'a> Unmarshal<'a> for WireString<'a> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        let size = reader.read::<u32>()? as usize;
        Self::read_payload(reader, size)
    }
}

impl Marshal for Option<&str> {
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
impl<'a> Unmarshal<'a> for Option<WireString<'a>> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        let size = reader.read::<u32>()? as usize;
        if size == 0 {
            Ok(None)
        } else {
            WireString::read_payload(reader, size).map(Some)
        }
    }
}

impl<S: Marshal> Marshal for GenericNewId<S> {
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
impl<'a> Unmarshal<'a> for GenericNewId<WireString<'a>> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        Ok(Self {
            interface: reader.read()?,
            version: reader.read()?,
            id: reader.read()?,
        })
    }
}

impl Marshal for &[u8] {
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
impl<'a> Unmarshal<'a> for &'a [u8] {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        let size = reader.read::<u32>()? as usize;
        let res = reader
            .read_bytes(size)
            .ok_or(unmarshal::Error::NotEnoughData)?;
        reader
            .skip(align_padding(4, size))
            .ok_or(unmarshal::Error::NotEnoughData)?;
        Ok(res)
    }
}

impl Marshal for Fd {
    fn calc_size(&self) -> usize {
        0
    }
    unsafe fn marshal(&self, writer: &mut Writer) {
        _ = writer;
    }
}
impl Unmarshal<'_> for Fd {
    fn unmarshal(reader: &mut Reader) -> unmarshal::Result<Self> {
        _ = reader;
        Ok(Self)
    }
}

impl<P: Marshal, T: MessageBody + Marshal> Marshal for Message<P, T> {
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
impl<'a, P: Proxy + Unmarshal<'a>, T: MessageBody + Unmarshal<'a>> Unmarshal<'a> for Message<P, T> {
    fn unmarshal(reader: &mut Reader<'a>) -> unmarshal::Result<Self> {
        Ok(Self {
            header: reader.read()?,
            body: reader.read()?,
        })
    }
}

/// Has a related `Request` enum
pub trait HasRequests<'a> {
    /// Related `Request` type
    type Request: MessageEnum<'a>;
}
/// Has a related `Event` enum
pub trait HasEvents<'a> {
    /// Related `Event` type
    type Event: MessageEnum<'a>;
}

/// Interface of a proxy
pub trait Proxy: Copy + Debug + From<Object> + Into<Object> {
    // TODO: use From and Into

    /// Create a proxy from an object
    fn new(object: Object) -> Self {
        object.into()
    }
    /// Get object id
    fn id(self) -> Object {
        self.into()
    }

    #[doc(hidden)]
    #[deprecated]
    const REQUEST_IS_DESTRUCTOR: &'static [bool];
    #[doc(hidden)]
    #[deprecated]
    const EVENT_IS_DESTRUCTOR: &'static [bool];

    /// Interface information of proxy
    const INTERFACE: Interface;
}
unsafe impl<P: Proxy> Trivial for Option<P> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Information of an interface
pub struct Interface {
    /// Interface version
    pub version: u32,
    /// Interface name
    pub name: &'static str,
}

impl Display for Interface {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}v_{}", self.name, self.version)
    }
}

#[cfg(feature = "std")]
/// Get UNIX domain stream socket path from environment variables
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
        xdg_runtime_dir.join(wayland_display)
    }
}

pub mod buffer;
pub mod slab;

pub mod marshal;
pub mod unmarshal;
