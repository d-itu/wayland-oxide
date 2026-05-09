use core::{
    marker::PhantomData,
    mem::{self, MaybeUninit},
    result,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("not enough data")]
    NotEnoughData,

    #[error("null object")]
    NullObject,
}

pub type Result<T> = result::Result<T, Error>;

pub unsafe trait Unmarshal<'a>: Sized {
    fn unmarshal(reader: &mut Reader<'a>) -> Result<Self>;
}

#[derive(Clone, Copy)]
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> From<&'a [u8]> for Reader<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::new(value)
    }
}

impl<'a> Into<&'a [u8]> for Reader<'a> {
    fn into(self) -> &'a [u8] {
        self.into_inner()
    }
}

impl<'a> Reader<'a> {
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }
    pub const fn into_inner(self) -> &'a [u8] {
        self.buf
    }
    pub(crate) const fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if let Some((read, remaining)) = self.buf.split_at_checked(n) {
            self.buf = remaining;
            Ok(read)
        } else {
            Err(Error::NotEnoughData)
        }
    }
    pub(crate) const fn skip(&mut self, n: usize) -> Result<()> {
        match self.read_bytes(n) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub(crate) const fn read_trivial<T>(&mut self) -> Result<T> {
        let bytes = match self.read_bytes(mem::size_of::<T>()) {
            Ok(x) => x,
            Err(e) => return Err(e),
        };
        let mut value = MaybeUninit::uninit();
        unsafe {
            bytes
                .as_ptr()
                .copy_to_nonoverlapping(value.as_mut_ptr() as _, mem::size_of::<T>());
            Ok(value.assume_init())
        }
    }
    pub fn read<T: Unmarshal<'a>>(&mut self) -> Result<T> {
        T::unmarshal(self)
    }
}

#[repr(transparent)]
pub struct Lazy<'a, T> {
    reader: Reader<'a>,
    marker: PhantomData<T>,
}

impl<'a, T> Lazy<'a, T> {
    pub const fn new(reader: Reader<'a>) -> Self {
        Self {
            reader,
            marker: PhantomData,
        }
    }
    pub fn parse(mut self) -> Result<T>
    where
        T: Unmarshal<'a>,
    {
        self.reader.read()
    }
}
