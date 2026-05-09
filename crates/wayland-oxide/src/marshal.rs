use core::{
    marker::PhantomData,
    mem::{self, MaybeUninit},
    slice,
};

use alloc::boxed::Box;
use tuplez::{Tuple, Unit};

pub unsafe trait Marshal {
    fn calc_size(&self) -> usize;
    unsafe fn marshal(&self, writer: &mut Writer);
}

#[derive(Clone, Copy)]
pub struct Writer<'a> {
    ptr: *mut u8,
    marker: PhantomData<&'a ()>,
}

impl<'a> Writer<'a> {
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
        cfg_select! {
            debug_assertions => {
                const ZEROS: [u8; 3] = [0; _];
                unsafe { self.write_bytes(slice::from_raw_parts(&ZEROS[0], n)) }
            }
            _ => {
                unsafe { self.ptr = self.ptr.add(n) }
            }
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
    pub unsafe fn write<T: Marshal + ?Sized>(&mut self, value: &T) {
        unsafe {
            value.marshal(self);
        }
    }
}

unsafe impl Marshal for Unit {
    fn calc_size(&self) -> usize {
        0
    }

    unsafe fn marshal(&self, writer: &mut Writer) {
        _ = writer;
    }
}
unsafe impl<X: Marshal, Xs: Marshal> Marshal for Tuple<X, Xs> {
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

pub trait MarshalExt: Marshal {
    fn marshal_to_boxed_slice(&self) -> Box<[u8]> {
        let size = self.calc_size();
        let mut buf = Box::new_uninit_slice(size);
        unsafe {
            Writer::new(&mut buf).write(self);
            buf.assume_init()
        }
    }
    fn marshal_to(&self, vec: &mut Vec<u8>) {
        let size = self.calc_size();
        vec.reserve(size);
        unsafe {
            Writer::new(vec.spare_capacity_mut()).write(self);
            vec.set_len(vec.len() + size);
        };
    }
}

impl<T: Marshal> MarshalExt for T {}
