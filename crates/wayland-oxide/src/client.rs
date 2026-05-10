//! Items used by client

use core::mem;

use crate::{
    HasEvents, MessageEnum, ProtocolError, Proxy,
    buffer::Buffer,
    slab::{self, DispatchFn, DropHandle, HasVTable, VTable},
};

/// Event dispatcher
pub trait Dispatch<P: Proxy + for<'a> HasEvents<'a>>: 'static {
    /// Handles an event, event payload is decoded lazily
    fn dispatch(
        &mut self,
        slab: &mut Slab<Self>,
        object: P,
        event: <P as HasEvents>::Event,
    ) -> Result<(), ProtocolError>;
}

#[repr(transparent)]
#[doc(hidden)]
pub struct Dispatcher<D: ?Sized>(D);

impl<D: Dispatch<P> + 'static, P: Proxy + for<'a> HasEvents<'a>> HasVTable<P, Dispatcher<D>>
    for Dispatcher<D>
{
    const V_TABLE: VTable<Dispatcher<D>> = VTable {
        interface: &P::INTERFACE,
        dispatch: &((|dispatch, slab, object, opcode, reader| {
            dispatch.0.dispatch(
                unsafe { mem::transmute(slab) },
                Proxy::new(object),
                MessageEnum::new(opcode, reader),
            )
        }) as DispatchFn<Dispatcher<D>>),
    };
}

/// Slab allocator of object data.
#[repr(transparent)]
pub struct Slab<D: 'static + ?Sized>(slab::Slab<Dispatcher<D>>);

impl<D: 'static> Slab<D> {
    /// Create an empty instance.
    pub fn new() -> Self {
        Self(slab::Slab::new())
    }
    /// Allocate new-id
    pub fn new_id<P: Proxy>(&mut self) -> P
    where
        Dispatcher<D>: HasVTable<P, Dispatcher<D>>,
    {
        self.0.client_new_id()
    }
    /// Call when receive an object created by server.
    pub fn receive_id<P: Proxy>(&mut self, object: P) -> Result<(), ProtocolError>
    where
        Dispatcher<D>: HasVTable<P, Dispatcher<D>>,
    {
        self.0.receive_server_id(object)
    }
    /// Decode all the events from `buffer` and dispatch them with dispatchers.
    pub fn dispatch<'a>(
        &mut self,
        dispatcher: &mut D,
        buffer: &mut Buffer,
    ) -> Result<(), ProtocolError> {
        self.0
            .dispatch(unsafe { mem::transmute(dispatcher) }, buffer)
    }
    /// Delete an Id.
    pub fn drop<Handle: DropHandle>(&mut self, handle: Handle) -> Result<(), ProtocolError> {
        handle.client_drop(&mut self.0)
    }
}
