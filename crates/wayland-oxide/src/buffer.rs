//! Buffer for reading messages

use core::{
    marker::PhantomData,
    mem::MaybeUninit,
    ptr::{self, NonNull},
};

use crate::{Reader, WireHeader};

/// Stack-allocated message buffer, with a capaciy of 64KB, which is the maximum possible size of a message.
/// Works like [`core::io::BorrowedBuf::filled`]
pub struct Buffer {
    data: [MaybeUninit<u8>; 1 << 16],
    filled: usize,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

// TODO: support core::io::BorrowedCursor
impl Buffer {
    /// Create an empty buffer
    pub const fn new() -> Self {
        Self {
            data: [MaybeUninit::uninit(); _],
            filled: 0,
        }
    }
    /// Return a slice of unfilled part. Useful for io syscalls.
    ///
    /// Remember to call advance after read operation.
    pub const fn unfilled(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { self.data.split_at_mut_unchecked(self.filled).1 }
    }

    /// Advances the cursor by asserting that `n` bytes have been filled.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the first `n` bytes of the cursor have been properly
    /// initialised.
    pub const unsafe fn advance(&mut self, n: usize) {
        self.filled += n;
    }
    /// Returns a shared reference to the filled portion of the buffer.
    pub const fn filled(&self) -> &[u8] {
        unsafe {
            self.data
                .split_at_unchecked(self.filled)
                .0
                .assume_init_ref()
        }
    }
    /// Copy data from `offset` to the begining.
    /// Call after consuming all the data since buffer may contain truncated message.
    pub(crate) const fn end(&mut self, offset: usize) {
        let count = self.filled - offset;
        unsafe {
            ptr::copy(
                self.data.as_ptr().add(offset),
                self.data.as_mut_ptr(),
                count,
            )
        };
        self.filled = count;
    }
    /// Fill the buffer with a reader implementing [`std::io::Read`]
    #[cfg(feature = "std")]
    pub fn fill(&mut self, reader: &mut impl std::io::Read) -> std::io::Result<()> {
        unsafe {
            let n = reader.read(self.unfilled().assume_init_mut())?;
            self.advance(n);
        }
        Ok(())
    }

    /// Drain the buffer into an iterator of messages
    pub fn drain<'b>(&'b mut self) -> Iter<'b> {
        Iter {
            buffer: NonNull::from_ref(self),
            offset: 0,
            marker: PhantomData,
        }
    }
}

/// A message with object, opcode and body
pub struct Message<'b> {
    /// The object id
    pub object: u32,
    /// The opcode
    pub opcode: u16,
    /// The body of the message
    pub reader: Reader<'b>,
}

/// Iterator of messages
pub struct Iter<'b> {
    buffer: NonNull<Buffer>,
    offset: usize,
    marker: PhantomData<&'b Buffer>,
}

impl Drop for Iter<'_> {
    fn drop(&mut self) {
        unsafe { self.buffer.as_mut() }.end(self.offset)
    }
}

impl<'b> Iterator for Iter<'b> {
    type Item = Message<'b>;

    fn next(&mut self) -> Option<Self::Item> {
        let buffer = unsafe { self.buffer.as_ref() };
        let data = unsafe { buffer.filled().get_unchecked(self.offset..) };
        let mut reader = Reader::new(data);
        let header: WireHeader = reader.read_trivial()?;
        let object = header.object;
        let opcode = header.opcode();
        let size = header.size();

        let body_size = size - 8;
        let (body, _) = reader.into_inner().split_at_checked(body_size as _)?;
        self.offset += size as usize;

        Some(Message {
            object,
            opcode,
            reader: Reader::new(body),
        })
    }
}
