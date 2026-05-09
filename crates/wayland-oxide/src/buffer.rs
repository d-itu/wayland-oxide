//!

use core::{mem::MaybeUninit, ptr};

/// Stack-allocated message buffer, with a capaciy of 64KB, which is the maximum possible size of a message.
/// Works like [`core::io::BorrowedBuf::filled`]
pub struct Buffer {
    data: [MaybeUninit<u8>; 1 << 16],
    filled: usize,
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
    /// ```
    /// let n = rustix::io::read(socket.as_fd(), &mut buffer.unfilled()).unwrap();
    /// unsafe { buffer.advance(n) };
    /// ```
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
    /// Copy data from `self.filled - n` to the begining.
    /// Call after dispatching messages since buffer may contain truncated message.
    pub(crate) const fn end(&mut self, n: usize) {
        unsafe {
            ptr::copy(
                self.data.as_ptr().add(self.filled - n),
                self.data.as_mut_ptr(),
                n,
            )
        };
        self.filled = n;
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
}
