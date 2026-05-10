//! Items used by a server.

use core::mem;

use crate::{
    HasRequests, MessageEnum, ProtocolError, Proxy,
    buffer::Buffer,
    slab::{self, DispatchFn, DropHandle, HasVTable, VTable},
};

/// Dispatch requests from client.
pub trait Dispatch<P: Proxy + for<'a> HasRequests<'a>>: 'static {
    /// Called when a request is processed
    fn dispatch(
        &mut self,
        slab: &mut Slab<Self>,
        object: P,
        event: <P as HasRequests>::Request,
    ) -> Result<(), ProtocolError>;
}

#[doc(hidden)]
#[repr(transparent)]
pub struct Dispatcher<D: ?Sized>(D);

impl<D: Dispatch<P> + 'static, P: Proxy + for<'a> HasRequests<'a>> HasVTable<P, Dispatcher<D>>
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
    /// Create an empty allocator
    pub fn new() -> Self {
        Self(slab::Slab::new())
    }
    /// Create an new id.
    pub fn new_id<P: Proxy>(&mut self) -> P
    where
        Dispatcher<D>: HasVTable<P, Dispatcher<D>>,
    {
        self.0.server_new_id()
    }
    /// Record an id allocated by client.
    pub fn receive_id<P: Proxy>(&mut self, object: P) -> Result<(), ProtocolError>
    where
        Dispatcher<D>: HasVTable<P, Dispatcher<D>>,
    {
        self.0.receive_client_id(object)
    }
    /// Decode and dispatch all the messages in the buffer.
    pub fn dispatch<'a>(
        &mut self,
        dispatcher: &mut D,
        buffer: &mut Buffer,
    ) -> Result<(), ProtocolError> {
        self.0
            .dispatch(unsafe { mem::transmute(dispatcher) }, buffer)
    }
    /// Delete an id.
    pub fn drop<Handle: DropHandle>(&mut self, handle: Handle) -> Result<(), ProtocolError> {
        handle.server_drop(&mut self.0)
    }
}
