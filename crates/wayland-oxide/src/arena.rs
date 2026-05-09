use core::{cmp::Ordering, num::NonZero};

use alloc::{vec, vec::Vec};

use crate::{Buffer, Header, Interface, Object, ProtocolError, Proxy, Reader, unmarshal};

use super::ID_SPACE_MASK;

pub type DispatchFn<D> =
    fn(&mut D, &mut Arena<D>, Object, u16, Reader) -> Result<(), ProtocolError>;

pub trait HasVTable<P, D: 'static> {
    const V_TABLE: VTable<D>;
}

pub struct VTable<D: 'static + ?Sized> {
    pub interface: &'static Interface,
    pub dispatch: &'static DispatchFn<D>,
}

pub struct Arena<D: 'static + ?Sized> {
    client_allocated: Vec<Option<&'static VTable<D>>>,
    server_allocated: Vec<Option<&'static VTable<D>>>,
    reusable: Vec<Object>,
}

pub trait DropHandle {
    fn client_drop<D>(self, arena: &mut Arena<D>) -> Result<(), ProtocolError>;
    fn server_drop<D>(self, arena: &mut Arena<D>) -> Result<(), ProtocolError>;
}
impl DropHandle for u32 {
    fn client_drop<D>(self, arena: &mut Arena<D>) -> Result<(), ProtocolError> {
        if let Some(id) = NonZero::new(self) {
            arena.client_drop_object(Object(id))?
        }
        Ok(())
    }
    fn server_drop<D>(self, arena: &mut Arena<D>) -> Result<(), ProtocolError> {
        if let Some(id) = NonZero::new(self) {
            arena.server_drop_object(Object(id))?
        }
        Ok(())
    }
}
impl<P: Proxy> DropHandle for P {
    fn client_drop<D>(self, arena: &mut Arena<D>) -> Result<(), ProtocolError> {
        arena.client_drop_object(self.id())
    }
    fn server_drop<D>(self, arena: &mut Arena<D>) -> Result<(), ProtocolError> {
        arena.server_drop_object(self.id())
    }
}

impl<D: 'static> Arena<D> {
    pub fn new() -> Self {
        Self {
            client_allocated: vec![],
            server_allocated: vec![],
            reusable: vec![],
        }
    }
    pub fn client_new_id<P: Proxy>(&mut self) -> P
    where
        D: HasVTable<P, D>,
    {
        if let Some(x) = self.reusable.pop() {
            *unsafe {
                self.client_allocated
                    .get_unchecked_mut(x.as_u32() as usize - 1)
            } = Some(&D::V_TABLE);
            P::new(x)
        } else {
            let new_id = (self.client_allocated.len() + 1) as u32;
            self.client_allocated.push(Some(&D::V_TABLE));
            P::new(unsafe { Object::from_u32(new_id) })
        }
    }
    pub fn server_new_id<P: Proxy>(&mut self) -> P
    where
        D: HasVTable<P, D>,
    {
        if let Some(x) = self.reusable.pop() {
            *unsafe {
                self.server_allocated
                    .get_unchecked_mut(x.as_u32() as usize - 1)
            } = Some(&D::V_TABLE);
            P::new(x)
        } else {
            let new_id = self.server_allocated.len() as u32 | ID_SPACE_MASK;
            self.server_allocated.push(Some(&D::V_TABLE));
            P::new(unsafe { Object::from_u32(new_id) })
        }
    }

    fn receive_id<P>(
        id: Object,
        idx: usize,
        vec: &mut Vec<Option<&'static VTable<D>>>,
    ) -> Result<(), ProtocolError>
    where
        D: HasVTable<P, D>,
    {
        match idx.cmp(&vec.len()) {
            Ordering::Less => {
                if unsafe { vec.get_unchecked_mut(idx) }
                    .replace(&D::V_TABLE)
                    .is_some()
                {
                    Err(ProtocolError::DuplicateId(id))?
                }
            }
            Ordering::Equal => vec.push(Some(&D::V_TABLE)),
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
    fn client_drop_object(&mut self, id: Object) -> Result<(), ProtocolError> {
        if id.as_u32() < ID_SPACE_MASK {
            match self.client_allocated.get_mut(id.as_u32() as usize - 1) {
                Some(x) => {
                    if x.take().is_none() {
                        Err(ProtocolError::UnknownObject(id))?
                    }
                }
                None => Err(ProtocolError::UnknownObject(id))?,
            }
            self.reusable.push(id);
        } else {
            match self.server_allocated.get_mut(id.as_u32() as usize - 1) {
                Some(x) => {
                    if x.take().is_none() {
                        Err(ProtocolError::UnknownObject(id))?
                    }
                }
                None => Err(ProtocolError::UnknownObject(id))?,
            }
        }
        Ok(())
    }
    fn server_drop_object(&mut self, id: Object) -> Result<(), ProtocolError> {
        if id.as_u32() < ID_SPACE_MASK {
            match self.client_allocated.get_mut(id.as_u32() as usize - 1) {
                Some(x) => {
                    if x.take().is_none() {
                        Err(ProtocolError::UnknownObject(id))?
                    }
                }
                None => Err(ProtocolError::UnknownObject(id))?,
            }
        } else {
            match self.server_allocated.get_mut(id.as_u32() as usize - 1) {
                Some(x) => {
                    if x.take().is_none() {
                        Err(ProtocolError::UnknownObject(id))?
                    }
                }
                None => Err(ProtocolError::UnknownObject(id))?,
            }
            self.reusable.push(id);
        }
        Ok(())
    }
    fn get(&self, id: Object) -> Option<&'static VTable<D>> {
        if id.as_u32() < ID_SPACE_MASK {
            self.client_allocated.get(id.as_u32() as usize - 1)
        } else {
            self.server_allocated
                .get((id.as_u32() - ID_SPACE_MASK) as usize)
        }
        .copied()
        .flatten()
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
