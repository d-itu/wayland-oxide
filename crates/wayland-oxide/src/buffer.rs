use core::{mem::MaybeUninit, ptr};

pub struct Buffer {
    data: [MaybeUninit<u8>; 1 << 16],
    filled: usize,
}

// TODO: support core::io::BorrowedCursor
impl Buffer {
    pub const fn new() -> Self {
        Self {
            data: [MaybeUninit::uninit(); _],
            filled: 0,
        }
    }
    pub const fn unfilled(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { self.data.split_at_mut_unchecked(self.filled).1 }
    }
    // call after filling the buffer.
    pub const unsafe fn advance(&mut self, n: usize) {
        self.filled += n;
    }
    pub const fn filled(&self) -> &[u8] {
        unsafe {
            self.data
                .split_at_unchecked(self.filled)
                .0
                .assume_init_ref()
        }
    }
    /// call after dispatching messages. copy n data from filled-n to begin
    pub const fn end(&mut self, n: usize) {
        unsafe {
            ptr::copy(
                self.data.as_ptr().add(self.filled - n),
                self.data.as_mut_ptr(),
                n,
            )
        };
        self.filled = n;
    }
    #[cfg(feature = "std")]
    pub fn fill(&mut self, reader: &mut impl std::io::Read) -> std::io::Result<()> {
        unsafe {
            let n = reader.read(self.unfilled().assume_init_mut())?;
            self.advance(n);
        }
        Ok(())
    }
}
