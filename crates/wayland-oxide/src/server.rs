use core::mem;

use crate::{
    HasRequests, MessageEnum, ProtocolError, Proxy,
    arena::{self, DispatchFn, DropHandle, HasVTable, VTable},
    buffer::Buffer,
};

pub trait Dispatch<P: Proxy + for<'a> HasRequests<'a>>: 'static {
    fn dispatch(
        &mut self,
        arena: &mut Arena<Self>,
        object: P,
        event: <P as HasRequests>::Request,
    ) -> Result<(), ProtocolError>;
}

#[repr(transparent)]
pub struct Dispatcher<D: ?Sized>(D);

impl<D: Dispatch<P> + 'static, P: Proxy + for<'a> HasRequests<'a>> HasVTable<P, Dispatcher<D>>
    for Dispatcher<D>
{
    const V_TABLE: VTable<Dispatcher<D>> = VTable {
        interface: &P::INTERFACE,
        dispatch: &((|dispatch, arena, object, opcode, reader| {
            dispatch.0.dispatch(
                unsafe { mem::transmute(arena) },
                Proxy::new(object),
                MessageEnum::new(opcode, reader),
            )
        }) as DispatchFn<Dispatcher<D>>),
    };
}

#[repr(transparent)]
pub struct Arena<D: 'static + ?Sized>(arena::Arena<Dispatcher<D>>);

impl<D: 'static> Arena<D> {
    pub fn new() -> Self {
        Self(arena::Arena::new())
    }
    pub fn new_id<P: Proxy>(&mut self) -> P
    where
        Dispatcher<D>: HasVTable<P, Dispatcher<D>>,
    {
        self.0.server_new_id()
    }
    pub fn receive_id<P: Proxy>(&mut self, object: P) -> Result<(), ProtocolError>
    where
        Dispatcher<D>: HasVTable<P, Dispatcher<D>>,
    {
        self.0.receive_client_id(object)
    }
    pub fn dispatch<'a>(
        &mut self,
        dispatcher: &mut D,
        buffer: &mut Buffer,
    ) -> Result<(), ProtocolError> {
        self.0
            .dispatch(unsafe { mem::transmute(dispatcher) }, buffer)
    }
    pub fn drop<Handle: DropHandle>(&mut self, handle: Handle) -> Result<(), ProtocolError> {
        handle.server_drop(&mut self.0)
    }
}
