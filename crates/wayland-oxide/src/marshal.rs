//! Items used for encoding messages

use core::{
    marker::PhantomData,
    mem::{self, MaybeUninit},
    slice,
};

use alloc::{boxed::Box, vec::Vec};
use tuplez::{Tuple, Unit};

use crate::{Message, MessageBody, Object};

/// Types can be encoded to a message buffer.
pub trait Marshal {
    /// Calculate byte size of a message. Size is used for allocating message buffer.
    fn calc_size(&self) -> usize;
    /// Marshal to a [`Writer`].
    ///
    /// # Safety
    ///
    /// Caller should ensure internal pointer of `writer` points to a buffer with capacity not smaller than `self.calc_size()`
    unsafe fn marshal(&self, writer: &mut Writer);
}

/// A writer for encoding messages.
#[derive(Clone, Copy)]
pub struct Writer<'a> {
    ptr: *mut u8,
    marker: PhantomData<&'a ()>,
}

impl<'a> Writer<'a> {
    /// Create a writer from buffer.
    pub const fn new(buf: &'a mut [MaybeUninit<u8>]) -> Self {
        Self {
            ptr: buf.as_mut_ptr() as _,
            marker: PhantomData,
        }
    }
}
impl Writer<'_> {
    pub(crate) const unsafe fn write_bytes(&mut self, bytes: &[u8]) {
        unsafe {
            self.ptr
                .copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
            self.ptr = self.ptr.add(bytes.len())
        }
    }
    pub(crate) const unsafe fn pad(&mut self, n: usize) {
        // Initialize padding before exposing the output as bytes.
        unsafe {
            self.ptr.write_bytes(0, n);
            self.ptr = self.ptr.add(n);
        }
    }
    pub(crate) const unsafe fn write_trivial<T>(&mut self, value: &T) {
        unsafe {
            self.write_bytes(slice::from_raw_parts(
                value as *const _ as _,
                mem::size_of::<T>(),
            ))
        }
    }
    /// Write a value
    /// # Safety
    ///
    /// Caller should ensure internal pointer of `self` points to a buffer with capacity not smaller than `value.calc_size()`
    pub unsafe fn write<T: Marshal + ?Sized>(&mut self, value: &T) {
        unsafe {
            value.marshal(self);
        }
    }
}

impl Marshal for Unit {
    fn calc_size(&self) -> usize {
        0
    }

    unsafe fn marshal(&self, writer: &mut Writer) {
        _ = writer;
    }
}
impl<X: Marshal, Xs: Marshal> Marshal for Tuple<X, Xs> {
    fn calc_size(&self) -> usize {
        self.0.calc_size() + self.1.calc_size()
    }

    unsafe fn marshal(&self, writer: &mut Writer) {
        unsafe {
            writer.write(&self.0);
            writer.write(&self.1);
        }
    }
}

/// A type contains zero or more complete messages.
pub trait Complete: Marshal {
    #[must_use]
    /// Returns a `Box<[u8]>` of encoded data
    fn marshal_to_boxed_slice(&self) -> Box<[u8]> {
        let size = self.calc_size();
        let mut buf = Box::new_uninit_slice(size);
        unsafe {
            Writer::new(&mut buf).write(self);
            buf.assume_init()
        }
    }
    /// Write encoded message to a `Vec<u8>`
    fn marshal_to(&self, vec: &mut Vec<u8>) {
        let size = self.calc_size();
        vec.reserve(size);
        unsafe {
            Writer::new(vec.spare_capacity_mut()).write(self);
            vec.set_len(vec.len() + size);
        };
    }
}

impl<P: Copy + Into<Object>, T: Marshal + MessageBody> Complete for Message<P, T> {}
impl Complete for Unit {}
impl<X: Complete, Xs: Complete> Complete for Tuple<X, Xs> {}
