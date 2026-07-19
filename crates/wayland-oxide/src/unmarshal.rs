//! Items used for decoding messages

use core::{
    marker::PhantomData,
    mem::{self, MaybeUninit},
    result,
};

use thiserror::Error;

/// Errors may occur when decoding
#[derive(Error, Debug)]
pub enum Error {
    /// Not enough data to decode
    #[error("not enough data")]
    NotEnoughData,

    /// Null object when an object is not optional
    #[error("null object")]
    NullObject,
}

/// Result of decoding
pub type Result<T> = result::Result<T, Error>;

/// A type that can be decoded from a buffer of encoded messages.
pub trait Unmarshal<'a>: Sized {
    /// Decode a type from a buffer of encoded messages.
    fn unmarshal(reader: &mut Reader<'a>) -> Result<Self>;
}

/// A reader of encoded messages.
#[derive(Clone, Copy)]
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> From<&'a [u8]> for Reader<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::new(value)
    }
}

impl<'a> From<Reader<'a>> for &'a [u8] {
    fn from(val: Reader<'a>) -> Self {
        val.into_inner()
    }
}

impl<'a> Reader<'a> {
    /// Create a reader from a buffer of encoded messages.
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }
    /// Returns buffer of remaining undecoded data.
    pub const fn into_inner(self) -> &'a [u8] {
        self.buf
    }
    pub(crate) const fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if let Some((read, remaining)) = self.buf.split_at_checked(n) {
            self.buf = remaining;
            return Some(read);
        }
        None
    }
    pub(crate) const fn skip(&mut self, n: usize) -> Option<()> {
        match self.read_bytes(n) {
            Some(_) => Some(()),
            None => None,
        }
    }
    pub(crate) const fn read_trivial<T>(&mut self) -> Option<T> {
        let bytes = match self.read_bytes(mem::size_of::<T>()) {
            Some(x) => x,
            None => return None,
        };
        let mut value = MaybeUninit::uninit();
        unsafe {
            bytes
                .as_ptr()
                .copy_to_nonoverlapping(value.as_mut_ptr() as _, mem::size_of::<T>());
            Some(value.assume_init())
        }
    }
    /// Decode a type implementing [`Unmarshal`] and move the begining pointer of the internal buffer.
    pub fn read<T: Unmarshal<'a>>(&mut self) -> Result<T> {
        T::unmarshal(self)
    }
}

/// Unparsed data of type `T`
#[repr(transparent)]
pub struct Lazy<'a, T> {
    reader: Reader<'a>,
    marker: PhantomData<T>,
}

impl<'a, T> Lazy<'a, T> {
    /// Create a [`Lazy`] from `reader`
    pub const fn new(reader: Reader<'a>) -> Self {
        Self {
            reader,
            marker: PhantomData,
        }
    }
    /// Decode `T`.
    pub fn parse(mut self) -> Result<T>
    where
        T: Unmarshal<'a>,
    {
        self.reader.read()
    }
}
