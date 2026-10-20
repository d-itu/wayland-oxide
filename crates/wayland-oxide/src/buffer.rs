//! Buffer for reading messages

use core::{mem::MaybeUninit, ptr};

use crate::{Reader, WireHeader};

/// Stack-allocated message buffer with a capacity of 64KB.
pub struct Buffer {
    data: [MaybeUninit<u8>; 1 << 16],
    start: usize,
    end: usize,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    /// Create an empty buffer.
    pub const fn new() -> Self {
        Self {
            data: [MaybeUninit::uninit(); _],
            start: 0,
            end: 0,
        }
    }

    /// Compact unconsumed data and return the unfilled region for I/O.
    ///
    /// Call [`Self::advance`] after initializing bytes in this region.
    pub const fn unfilled(&mut self) -> &mut [MaybeUninit<u8>] {
        self.compact();
        unsafe { self.data.split_at_mut_unchecked(self.end).1 }
    }

    /// Move unconsumed bytes to the beginning of the buffer.
    pub const fn compact(&mut self) {
        if self.start != 0 {
            let remaining = self.end - self.start;
            // SAFETY: Both ranges are within data; copy permits overlap.
            // Exclusive access to self prevents outstanding message borrows.
            unsafe {
                ptr::copy(
                    self.data.as_ptr().add(self.start),
                    self.data.as_mut_ptr(),
                    remaining,
                );
            }
            self.start = 0;
            self.end = remaining;
        }
    }

    /// Advance the filled region by `n` bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `n` does not exceed the unfilled region's length
    /// and that its first `n` bytes have been initialized.
    pub const unsafe fn advance(&mut self, n: usize) {
        self.end += n;
    }

    /// Return the initialized, unconsumed bytes.
    pub const fn filled(&self) -> &[u8] {
        // SAFETY: start..end lies within the initialized region.
        unsafe {
            self.data
                .split_at_unchecked(self.end)
                .0
                .split_at_unchecked(self.start)
                .1
                .assume_init_ref()
        }
    }

    /// Drain complete messages, consuming each message as it is returned.
    ///
    /// Unreturned messages and incomplete data remain available for the next drain.
    /// Returned messages can outlive the iterator; bytes are moved only on the next
    /// call to [`Self::unfilled`].
    pub const fn drain(&mut self) -> Iter<'_> {
        // SAFETY: start..end contains initialized bytes. The data and consumption
        // cursor are disjoint fields, so advancing the cursor cannot alter messages.
        let data = unsafe {
            self.data
                .split_at_unchecked(self.end)
                .0
                .split_at_unchecked(self.start)
                .1
                .assume_init_ref()
        };
        Iter {
            remaining: data,
            consumed: &mut self.start,
        }
    }
}

/// A message with object, opcode and body.
pub struct Message<'b> {
    /// The object id.
    pub object: u32,
    /// The opcode.
    pub opcode: u16,
    /// The body of the message.
    pub reader: Reader<'b>,
}

/// An invalid wire message length. Message boundaries cannot be recovered reliably.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid message size: {0}")]
pub struct InvalidMessageSize(pub u16);

/// Iterator of messages borrowing the buffer's data.
///
/// An invalid length yields one error and ends this iterator without consuming
/// the invalid message. The caller should terminate the connection.
pub struct Iter<'b> {
    remaining: &'b [u8],
    consumed: &'b mut usize,
}

impl<'b> Iterator for Iter<'b> {
    type Item = Result<Message<'b>, InvalidMessageSize>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = Reader::new(self.remaining);
        let header: WireHeader = reader.read_trivial()?;

        let size = header.size();
        if size < 8 || !size.is_multiple_of(4) {
            self.remaining = &[];
            return Some(Err(InvalidMessageSize(size)));
        }
        let body_size = size - 8;
        let (body, remaining) = reader.into_inner().split_at_checked(body_size as _)?;
        self.remaining = remaining;
        *self.consumed += size as usize;

        Some(Ok(Message {
            object: header.object,
            opcode: header.opcode(),
            reader: Reader::new(body),
        }))
    }
}
