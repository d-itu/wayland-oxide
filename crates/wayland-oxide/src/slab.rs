//! Slab allocator for objects

#[cfg(test)]
mod tests;

use core::{cmp::Ordering, hint, marker::PhantomData, mem};

use alloc::{vec, vec::Vec};
use thiserror::Error;

use crate::{ID_SPACE_MASK, Object};

/// Handle to a vacant slot
#[derive(Clone, Copy)]
pub struct Handle(u32);
impl From<()> for Handle {
    fn from(_: ()) -> Self {
        Self(ID_SPACE_MASK)
    }
}

/// A slab entry containing either a value or a vacant-slot handle.
///
/// # Safety
///
/// Implementations must uphold the following invariants, which [`Slab`] relies
/// on when accessing values and following its free list without further checks:
///
/// - [`Self::vacant`] creates a vacant entry and preserves the supplied [`Handle`]
///   exactly. [`Self::vacant_unchecked`] returns that same handle.
/// - Converting [`Self::Value`] into `Self` creates an occupied entry containing
///   that value.
/// - [`Self::is_value`] accurately reports whether the entry is occupied. Shared
///   access must not change the entry's occupied/vacant state or stored handle.
/// - For occupied entries, [`Self::take_unchecked`] returns the stored value, and
///   [`Self::get`] and [`Self::get_mut`] return references to it. For vacant
///   entries, both accessors return `None`.
/// - Calling the accessors or modifying a value through the returned references
///   must not change the entry's occupied/vacant state or its stored handle.
///
/// Handles need not be validated by the entry: the slab is responsible for
/// supplying valid free-list links, and the entry must preserve them unchanged.
pub unsafe trait Entry: Sized {
    /// Value of entry
    type Value: Into<Self>;

    /// Create a vacant entry
    fn vacant(handle: Handle) -> Self;

    /// Get vacant handle
    /// # Safety
    /// `self` must be vacant
    unsafe fn vacant_unchecked(&self) -> Handle;

    /// Check if the entry is occupied
    fn is_value(&self) -> bool;

    /// Take the value
    /// # Safety
    /// `self` must be occupied
    unsafe fn take_unchecked(self) -> Self::Value;

    /// Get mutable reference to the value
    fn get_mut(&mut self) -> Option<&mut Self::Value>;

    /// Get reference to the value
    fn get(&self) -> Option<&Self::Value>;
}

/// Tagged union of value and vacant handle
pub enum TaggedUnion<V> {
    /// Occupied with value
    Occupied(V),
    /// Vacant with handle
    Vacant(Handle),
}
impl<V> From<V> for TaggedUnion<V> {
    fn from(value: V) -> Self {
        Self::Occupied(value)
    }
}
// SAFETY: The enum discriminant consistently identifies the entry state.
// Vacant preserves its handle unchanged, and value access cannot change the
// discriminant or access the handle. From<V> always constructs Occupied.
unsafe impl<V> Entry for TaggedUnion<V> {
    type Value = V;

    fn vacant(handle: Handle) -> Self {
        Self::Vacant(handle)
    }

    unsafe fn vacant_unchecked(&self) -> Handle {
        match self {
            TaggedUnion::Occupied(_) => unsafe { hint::unreachable_unchecked() },
            &TaggedUnion::Vacant(handle) => handle,
        }
    }

    fn is_value(&self) -> bool {
        match self {
            TaggedUnion::Occupied(_) => true,
            TaggedUnion::Vacant(_) => false,
        }
    }

    unsafe fn take_unchecked(self) -> Self::Value {
        match self {
            TaggedUnion::Occupied(x) => x,
            TaggedUnion::Vacant(_) => unsafe { hint::unreachable_unchecked() },
        }
    }

    fn get_mut(&mut self) -> Option<&mut Self::Value> {
        match self {
            TaggedUnion::Occupied(x) => Some(x),
            TaggedUnion::Vacant(_) => None,
        }
    }
    fn get(&self) -> Option<&Self::Value> {
        match self {
            TaggedUnion::Occupied(x) => Some(x),
            TaggedUnion::Vacant(_) => None,
        }
    }
}

/// Client side
pub struct Client;
/// Server side
pub struct Server;

mod private {
    pub trait Side {
        const VEC_OFFSET: usize;
        const SLOT_OFFSET: usize;
        const SLOT_COUNT: usize;
        type Peer: Side;
    }
}
use private::Side;

impl private::Side for Client {
    const VEC_OFFSET: usize = 0;
    const SLOT_OFFSET: usize = 1;
    const SLOT_COUNT: usize = ID_SPACE_MASK as usize - 1;
    type Peer = Server;
}
impl private::Side for Server {
    const VEC_OFFSET: usize = 1;
    const SLOT_OFFSET: usize = ID_SPACE_MASK as _;
    const SLOT_COUNT: usize = (u32::MAX - ID_SPACE_MASK) as usize + 1;
    type Peer = Client;
}

/// Slab allocator for objects
#[derive(Default)]
pub struct Slab<E, S> {
    /// `storage[0]` -> client
    ///
    /// `storage[1]` -> server
    storage: [Vec<E>; 2],
    next_free: usize,
    marker: PhantomData<S>,
}

impl<E, S> Slab<E, S> {
    /// Create a new empty slab
    pub const fn new() -> Self {
        Self {
            storage: [const { vec![] }; 2],
            next_free: 0,
            marker: PhantomData,
        }
    }
}

fn slab_try_remove<E: Entry>(
    entries: &mut [E],
    next_free: &mut usize,
    index: usize,
) -> Option<E::Value> {
    if let Some(entry) = entries.get_mut(index)
        && entry.is_value()
    {
        let value =
            unsafe { mem::replace(entry, E::vacant(Handle(*next_free as _))).take_unchecked() };
        *next_free = index;
        return Some(value);
    }
    None
}

fn object_id<S: Side>(index: usize) -> Option<Object> {
    if index >= S::SLOT_COUNT {
        return None;
    }
    // SAFETY: The checked index and side offset produce a nonzero u32 within
    // this side's namespace, without overflowing usize or narrowing the value.
    Some(unsafe { Object::from_u32((index + S::SLOT_OFFSET) as u32) })
}

impl<E: Entry, S: private::Side> Slab<E, S> {
    fn push(&mut self, value: E::Value) {
        let index = self.next_free;
        let entries = unsafe { self.storage.get_unchecked_mut(S::VEC_OFFSET) };

        if index == entries.len() {
            entries.push(value.into());
            self.next_free += 1;
        } else {
            let entry = unsafe { entries.get_unchecked_mut(index) };
            self.next_free = unsafe { entry.vacant_unchecked().0 } as usize;
            *entry = value.into();
        }
    }
    /// Insert a new object, reusing a vacant slot when available.
    ///
    /// Returns `None` without changing the slab if its object ID namespace is
    /// exhausted. The supplied value is dropped in that case.
    pub fn insert(&mut self, data: E::Value) -> Option<Object> {
        let object = object_id::<S>(self.next_free)?;
        self.push(data);
        Some(object)
    }
    /// Receive an object from the peer.
    ///
    /// Reject IDs outside the peer's namespace, skipped slots, and occupied slots.
    /// Vacant slots may be reused.
    pub fn receive(&mut self, object: Object, data: E::Value) -> Result<(), Error> {
        let id = object.as_u32();
        if (id >= ID_SPACE_MASK) != (S::Peer::VEC_OFFSET == Server::VEC_OFFSET) {
            return Err(Error::InvalidId(object));
        }
        let index = id as usize - S::Peer::SLOT_OFFSET;
        let vec = unsafe { self.storage.get_unchecked_mut(S::Peer::VEC_OFFSET) };
        match index.cmp(&vec.len()) {
            Ordering::Less => {
                let entry = unsafe { vec.get_unchecked_mut(index) };
                if entry.is_value() {
                    return Err(Error::OccupiedId(object));
                }
                *entry = data.into();
            }
            Ordering::Equal => vec.push(data.into()),
            Ordering::Greater => Err(Error::InvalidId(object))?,
        }
        Ok(())
    }
    /// Get reference to the client-allocated value
    /// # Safety
    /// `object` must be client-allocated
    pub unsafe fn client_get_unchecked(&self, object: Object) -> Option<&E::Value> {
        let id = object.as_u32();
        debug_assert!(id < ID_SPACE_MASK);
        let index = id as usize - 1;
        unsafe { self.storage.get_unchecked(Client::VEC_OFFSET) }
            .get(index)
            .and_then(E::get)
    }
    /// Get reference to the server-allocated value
    /// # Safety
    /// `object` must be server-allocated
    pub unsafe fn server_get_unchecked(&self, object: Object) -> Option<&E::Value> {
        let id = object.as_u32();
        debug_assert!(id >= ID_SPACE_MASK);
        let index = (id - ID_SPACE_MASK) as usize;
        unsafe { self.storage.get_unchecked(Server::VEC_OFFSET) }
            .get(index)
            .and_then(E::get)
    }
    /// Get reference to the value
    pub fn get(&self, object: Object) -> Option<&E::Value> {
        match object.side() {
            crate::Side::Client => unsafe { self.client_get_unchecked(object) },
            crate::Side::Server => unsafe { self.server_get_unchecked(object) },
        }
    }
    /// Get mutable reference to the client-allocated value
    /// # Safety
    /// `object` must be client-allocated
    pub unsafe fn client_get_unchecked_mut(&mut self, object: Object) -> Option<&mut E::Value> {
        let id = object.as_u32();
        debug_assert!(id < ID_SPACE_MASK);
        let index = id as usize - 1;
        unsafe { self.storage.get_unchecked_mut(Client::VEC_OFFSET) }
            .get_mut(index)
            .and_then(E::get_mut)
    }
    /// Get mutable reference to the server-allocated value
    /// # Safety
    /// `object` must be server-allocated
    pub unsafe fn server_get_unchecked_mut(&mut self, object: Object) -> Option<&mut E::Value> {
        let id = object.as_u32();
        debug_assert!(id >= ID_SPACE_MASK);
        let index = (id - ID_SPACE_MASK) as usize;
        unsafe { self.storage.get_unchecked_mut(Server::VEC_OFFSET) }
            .get_mut(index)
            .and_then(E::get_mut)
    }
    /// Get mutable reference to the value
    pub fn get_mut(&mut self, object: Object) -> Option<&mut E::Value> {
        match object.side() {
            crate::Side::Client => unsafe { self.client_get_unchecked_mut(object) },
            crate::Side::Server => unsafe { self.server_get_unchecked_mut(object) },
        }
    }
}
impl<E: Entry> Slab<E, Client> {
    /// Remove client-allocated value
    /// # Safety
    /// `object` must be client-allocated
    pub unsafe fn client_remove_unchecked(&mut self, object: Object) -> Option<E::Value> {
        let id = object.as_u32();
        let index = id as usize - 1;
        slab_try_remove(
            unsafe { self.storage.get_unchecked_mut(Client::VEC_OFFSET) },
            &mut self.next_free,
            index,
        )
    }
    /// Remove server-allocated value
    /// # Safety
    /// `object` must be server-allocated
    pub unsafe fn server_remove_unchecked(&mut self, object: Object) -> Option<E::Value> {
        let id = object.as_u32();
        let index = (id - ID_SPACE_MASK) as usize;
        slab_try_remove(
            unsafe { self.storage.get_unchecked_mut(Server::VEC_OFFSET) },
            &mut 0,
            index,
        )
    }
    /// Remove value
    pub fn remove(&mut self, object: Object) -> Option<E::Value> {
        match object.side() {
            crate::Side::Client => unsafe { self.client_remove_unchecked(object) },
            crate::Side::Server => unsafe { self.server_remove_unchecked(object) },
        }
    }
}
impl<E: Entry> Slab<E, Server> {
    /// Remove client-allocated value
    /// # Safety
    /// `object` must be client-allocated
    pub unsafe fn client_remove_unchecked(&mut self, object: Object) -> Option<E::Value> {
        let id = object.as_u32();
        let index = id as usize - 1;
        slab_try_remove(
            unsafe { self.storage.get_unchecked_mut(Client::VEC_OFFSET) },
            &mut 0,
            index,
        )
    }
    /// Remove server-allocated value
    /// # Safety
    /// `object` must be server-allocated
    pub unsafe fn server_remove_unchecked(&mut self, object: Object) -> Option<E::Value> {
        let id = object.as_u32();
        let index = (id - ID_SPACE_MASK) as usize;
        slab_try_remove(
            unsafe { self.storage.get_unchecked_mut(Server::VEC_OFFSET) },
            &mut self.next_free,
            index,
        )
    }
    /// Remove value
    pub fn remove(&mut self, object: Object) -> Option<E::Value> {
        match object.side() {
            crate::Side::Client => unsafe { self.client_remove_unchecked(object) },
            crate::Side::Server => unsafe { self.server_remove_unchecked(object) },
        }
    }
}

#[derive(Error, Debug)]
/// Errors of protocol violation by the peer
pub enum Error {
    /// The ID is outside the peer's namespace or skips an unallocated slot.
    #[error("Invalid Id: {0:?}")]
    InvalidId(Object),
    /// The object ID is already occupied.
    #[error("object ID is already occupied: {0:?}")]
    OccupiedId(Object),
}
