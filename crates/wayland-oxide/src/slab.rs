use core::{cmp::Ordering, marker::PhantomData, num::NonZero, ptr};

use alloc::{vec, vec::Vec};

use crate::{Buffer, Header, Interface, Object, ProtocolError, Proxy, Reader, unmarshal};

use super::ID_SPACE_MASK;

pub type DispatchFn<D> = fn(&mut D, &mut Slab<D>, Object, u16, Reader) -> Result<(), ProtocolError>;

pub trait HasVTable<P, D: 'static> {
    const V_TABLE: VTable<D>;
}

pub struct VTable<D: 'static + ?Sized> {
    pub interface: &'static Interface,
    pub dispatch: &'static DispatchFn<D>,
}

/// A Tagged union of `value` and `next_free` represented by `usize`.
/// `value` is 8-byte (or 4-byte on 32-bit machines) aligned pointer, so we can use lsb as tag.
/// Entry is `value` when lsb is 1; Entry is `next_free` when lsb is 0.
///
/// Moreover, `Vec` len will not exceeds u32 limit, so we use high 32 bits to store `next_free`
struct Entry<D: ?Sized> {
    repr: u64,
    marker: PhantomData<D>,
}

impl<D> Clone for Entry<D> {
    fn clone(&self) -> Self {
        Self::new(self.repr)
    }
}
impl<D> Copy for Entry<D> {}

impl<D> Entry<D> {
    fn new(repr: u64) -> Self {
        Self {
            repr,
            marker: PhantomData,
        }
    }
    fn value(self) -> Option<&'static VTable<D>> {
        (self.repr & 1 == 0).then(|| unsafe { self.value_unchecked() })
    }
    unsafe fn value_unchecked(self) -> &'static VTable<D> {
        unsafe { ptr::with_exposed_provenance::<VTable<D>>(self.repr as usize).as_ref_unchecked() }
    }
    fn next_free(self) -> usize {
        (self.repr >> 32) as _
    }
    fn from_value(value: &'static VTable<D>) -> Self {
        let repr = value as *const _ as usize as _;
        debug_assert!(repr & 1 == 0);
        Self::new(repr)
    }
    fn from_next_free(index: usize) -> Self {
        debug_assert!(index <= ID_SPACE_MASK as _);
        Self::new((index as u64) << 32 | 1)
    }
}

/// Slab allocator of object data.
pub struct Slab<D: 'static + ?Sized> {
    client_allocated: Vec<Entry<D>>,
    server_allocated: Vec<Entry<D>>,
    next: usize,
}

fn push<D>(entries: &mut Vec<Entry<D>>, next: &mut usize, value: &'static VTable<D>) {
    let index = *next;
    if index == entries.len() {
        entries.push(Entry::from_value(value));
        *next += 1;
    } else {
        let entry = unsafe { entries.get_unchecked_mut(index) };
        *next = entry.next_free();
        *entry = Entry::from_value(value);
    }
}
fn try_remove<D>(
    entries: &mut Vec<Entry<D>>,
    next: &mut usize,
    index: usize,
) -> Option<&'static VTable<D>> {
    if let Some(entry) = entries.get_mut(index) {
        if let Some(value) = entry.value() {
            *entry = Entry::from_next_free(*next);
            *next = index;
            return Some(value);
        }
    }
    None
}
fn vec_try_remove<D>(entries: &mut Vec<Entry<D>>, index: usize) -> Option<&'static VTable<D>> {
    if let Some(entry) = entries.get_mut(index) {
        if let Some(value) = entry.value() {
            return Some(value);
        }
    }
    None
}

impl<D: 'static> Slab<D> {
    pub fn new() -> Self {
        Self {
            client_allocated: vec![],
            server_allocated: vec![],
            next: 0,
        }
    }
    pub fn client_new_id<P: Proxy>(&mut self) -> P
    where
        D: HasVTable<P, D>,
    {
        let id = self.next as u32 + 1;
        push(&mut self.client_allocated, &mut self.next, &D::V_TABLE);
        P::new(unsafe { Object::from_u32(id) })
    }
    pub fn server_new_id<P: Proxy>(&mut self) -> P
    where
        D: HasVTable<P, D>,
    {
        let id = self.next as u32 + ID_SPACE_MASK;
        push(&mut self.server_allocated, &mut self.next, &D::V_TABLE);
        P::new(unsafe { Object::from_u32(id) })
    }

    fn receive_id<P>(id: Object, index: usize, vec: &mut Vec<Entry<D>>) -> Result<(), ProtocolError>
    where
        D: HasVTable<P, D>,
    {
        match index.cmp(&vec.len()) {
            Ordering::Less => {
                let entry = unsafe { vec.get_unchecked_mut(index) };
                if entry.value().is_some() {
                    Err(ProtocolError::DuplicateId(id))?
                }
                *entry = Entry::from_value(&D::V_TABLE)
            }
            Ordering::Equal => vec.push(Entry::from_value(&D::V_TABLE)),
            Ordering::Greater => Err(ProtocolError::IdNotDense(id))?,
        }
        Ok(())
    }
    pub fn receive_server_id<P: Proxy>(&mut self, object: P) -> Result<(), ProtocolError>
    where
        D: HasVTable<P, D>,
    {
        let id = object.id();
        Self::receive_id(
            id,
            (id.as_u32() - ID_SPACE_MASK) as usize,
            &mut self.client_allocated,
        )
    }
    pub fn receive_client_id<P: Proxy>(&mut self, object: P) -> Result<(), ProtocolError>
    where
        D: HasVTable<P, D>,
    {
        let id = object.id();
        Self::receive_id(id, id.as_u32() as usize - 1, &mut self.client_allocated)
    }
    fn client_drop_object(&mut self, object: Object) -> Result<(), ProtocolError> {
        let id = object.as_u32();
        let index = id as usize - 1;
        if id < ID_SPACE_MASK {
            try_remove(&mut self.client_allocated, &mut self.next, index)
        } else {
            vec_try_remove(&mut self.server_allocated, index)
        }
        .map(|_| {})
        .ok_or_else(|| ProtocolError::UnknownObject(object))
    }
    fn server_drop_object(&mut self, object: Object) -> Result<(), ProtocolError> {
        let id = object.as_u32();
        let index = (id - ID_SPACE_MASK) as usize;
        if id < ID_SPACE_MASK {
            vec_try_remove(&mut self.client_allocated, index)
        } else {
            try_remove(&mut self.server_allocated, &mut self.next, index)
        }
        .map(|_| {})
        .ok_or_else(|| ProtocolError::UnknownObject(object))
    }
    fn get(&self, id: Object) -> Option<&'static VTable<D>> {
        if id.as_u32() < ID_SPACE_MASK {
            self.client_allocated.get(id.as_u32() as usize - 1)
        } else {
            self.server_allocated
                .get((id.as_u32() - ID_SPACE_MASK) as usize)
        }
        .copied()
        .and_then(Entry::value)
    }
    pub fn dispatch<'a>(
        &mut self,
        dispatcher: &mut D,
        buffer: &mut Buffer,
    ) -> Result<(), ProtocolError> {
        let mut reader: Reader = buffer.filled().into();
        let n = loop {
            let Header {
                object,
                opcode,
                size,
            }: Header = match reader.read() {
                Ok(x) => x,
                Err(unmarshal::Error::NotEnoughData) => break reader.into_inner().len(),
                Err(e) => Err(e)?,
            };
            let size = size - 8;
            let (_, remaining) = match reader.into_inner().split_at_checked(size as _) {
                Some(x) => x,
                None => break reader.into_inner().len() + 8,
            };
            let dispatch = self
                .get(object)
                .ok_or_else(|| ProtocolError::UnknownObject(object))?
                .dispatch;
            dispatch(dispatcher, self, object, opcode, reader)?;

            reader = remaining.into();
        };
        buffer.end(n);
        Ok(())
    }
}

/// Type can be used to call [`crate::client::Slab::drop`] or [`crate::server::Slab::drop`]
pub trait DropHandle {
    /// drop function used by clients
    fn client_drop<D>(self, slab: &mut Slab<D>) -> Result<(), ProtocolError>;
    /// drop function used by servers
    fn server_drop<D>(self, slab: &mut Slab<D>) -> Result<(), ProtocolError>;
}
impl DropHandle for u32 {
    fn client_drop<D>(self, slab: &mut Slab<D>) -> Result<(), ProtocolError> {
        if let Some(id) = NonZero::new(self) {
            slab.client_drop_object(Object(id))?
        }
        Ok(())
    }
    fn server_drop<D>(self, slab: &mut Slab<D>) -> Result<(), ProtocolError> {
        if let Some(id) = NonZero::new(self) {
            slab.server_drop_object(Object(id))?
        }
        Ok(())
    }
}
impl<P: Proxy> DropHandle for P {
    fn client_drop<D>(self, slab: &mut Slab<D>) -> Result<(), ProtocolError> {
        slab.client_drop_object(self.id())
    }
    fn server_drop<D>(self, slab: &mut Slab<D>) -> Result<(), ProtocolError> {
        slab.server_drop_object(self.id())
    }
}
