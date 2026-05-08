use core::{cmp::Ordering, num::NonZero};

use alloc::{vec, vec::Vec};

use crate::{
    Buffer, HasEvents, Header, Interface, MessageEnum, Object, ProtocolError, Proxy, Reader,
    unmarshal,
};

use super::ID_SPACE_MASK;

pub trait Dispatch<P: Proxy + for<'a> HasEvents<'a>>: 'static {
    fn dispatch(
        &mut self,
        arena: &mut Arena<Self>,
        object: P,
        event: <P as HasEvents>::Event,
    ) -> Result<(), ProtocolError>;
}

type DispatchFn<D> = fn(&mut D, &mut Arena<D>, Object, u16, Reader) -> Result<(), ProtocolError>;

trait HasVTable<P, D: 'static> {
    const V_TABLE: VTable<D>;
}

impl<D: Dispatch<P> + 'static, P: Proxy + for<'a> HasEvents<'a>> HasVTable<P, D> for D {
    const V_TABLE: VTable<D> = VTable {
        interface: &P::INTERFACE,
        dispatch: &((|dispatch, arena, object, opcode, reader| {
            dispatch.dispatch(arena, Proxy::new(object), MessageEnum::new(opcode, reader))
        }) as DispatchFn<D>),
    };
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

impl<D: 'static> Arena<D> {
    pub fn new() -> Self {
        Self {
            client_allocated: vec![],
            server_allocated: vec![],
            reusable: vec![],
        }
    }
    pub fn new_id<P: Proxy + for<'a> HasEvents<'a>>(&mut self) -> P
    where
        D: Dispatch<P>,
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
    pub fn new_server_id<P: Proxy + for<'a> HasEvents<'a>>(
        &mut self,
        object: P,
    ) -> Result<(), ProtocolError>
    where
        D: Dispatch<P>,
    {
        let id = object.id();
        let idx = id
            .as_u32()
            .checked_sub(ID_SPACE_MASK)
            .ok_or_else(|| ProtocolError::IdOverflow(id))? as usize;
        match idx.cmp(&self.server_allocated.len()) {
            Ordering::Less => {
                if unsafe { self.server_allocated.get_unchecked_mut(idx) }
                    .replace(&D::V_TABLE)
                    .is_some()
                {
                    Err(ProtocolError::DuplicateId(id))?
                }
            }
            Ordering::Equal => self.server_allocated.push(Some(&D::V_TABLE)),
            Ordering::Greater => Err(ProtocolError::IdNotDense(id))?,
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
    pub fn drop(&mut self, id: u32) -> Result<(), ProtocolError> {
        if let Some(id) = NonZero::new(id) {
            self.drop_object(Object(id))?
        }
        Ok(())
    }
    fn drop_object(&mut self, id: Object) -> Result<(), ProtocolError> {
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
