use super::*;

fn object(id: u32) -> Object {
    Object::try_from_u32(id).unwrap()
}

macro_rules! receive_tests {
    ($name:ident, $side:ty, $peer_first:expr, $local_first:expr) => {
        #[test]
        fn $name() {
            let mut slab = Slab::<TaggedUnion<u32>, $side>::new();
            let first = object($peer_first);
            let second = object($peer_first + 1);

            // Invalid namespaces and gaps must not change allocation state.
            for id in [$local_first, $peer_first + 1, u32::MAX] {
                let id = object(id);
                assert!(matches!(slab.receive(id, 99), Err(Error::InvalidId(actual)) if actual == id));
                assert_eq!(slab.get(id), None);
            }

            slab.receive(first, 11).unwrap();
            assert!(matches!(slab.receive(first, 99), Err(Error::OccupiedId(actual)) if actual == first));
            assert_eq!(slab.get(first), Some(&11));
            slab.receive(second, 22).unwrap();

            // Reusing a peer slot must not affect the local free list.
            let local = slab.insert(33).unwrap();
            assert_eq!(local, object($local_first));
            assert!(matches!(slab.receive(local, 99), Err(Error::InvalidId(actual)) if actual == local));
            assert_eq!(slab.get(local), Some(&33));
            assert_eq!(slab.remove(local), Some(33));
            assert_eq!(slab.remove(first), Some(11));
            slab.receive(first, 44).unwrap();
            assert_eq!(slab.get(first), Some(&44));
            assert_eq!(slab.get(second), Some(&22));
            assert_eq!(slab.insert(55), Some(local));
            assert_eq!(slab.get(local), Some(&55));
        }
    };
}

receive_tests!(client_receives_server_ids, Client, ID_SPACE_MASK, 1);
receive_tests!(server_receives_client_ids, Server, 1, ID_SPACE_MASK);

#[test]
fn allocation_ids_stay_within_each_namespace() {
    assert_eq!(object_id::<Client>(0).unwrap().as_u32(), 1);
    assert_eq!(
        object_id::<Client>(Client::SLOT_COUNT - 1)
            .unwrap()
            .as_u32(),
        ID_SPACE_MASK - 1
    );
    assert_eq!(object_id::<Server>(0).unwrap().as_u32(), ID_SPACE_MASK);
    assert_eq!(
        object_id::<Server>(Server::SLOT_COUNT - 1)
            .unwrap()
            .as_u32(),
        u32::MAX
    );
    for index in [Client::SLOT_COUNT, usize::MAX] {
        assert_eq!(object_id::<Client>(index), None);
    }
    for index in [Server::SLOT_COUNT, usize::MAX] {
        assert_eq!(object_id::<Server>(index), None);
    }
}

fn check_exhaustion<S: Side>() {
    let mut slab = Slab::<TaggedUnion<u32>, S>::new();
    slab.storage[S::VEC_OFFSET].push(TaggedUnion::Occupied(11));
    // Model an exhausted cursor without allocating the entire namespace.
    // insert must reject it before accessing storage at this index.
    slab.next_free = S::SLOT_COUNT;
    assert_eq!(slab.insert(99), None);
    assert_eq!(slab.next_free, S::SLOT_COUNT);
    assert_eq!(slab.storage[S::VEC_OFFSET].len(), 1);
    assert_eq!(slab.storage[S::VEC_OFFSET][0].get(), Some(&11));
    assert!(slab.storage[S::Peer::VEC_OFFSET].is_empty());

    // A freed slot remains reusable even when the free-list tail is exhausted.
    assert_eq!(
        slab_try_remove(&mut slab.storage[S::VEC_OFFSET], &mut slab.next_free, 0),
        Some(11)
    );
    assert_eq!(slab.insert(22), object_id::<S>(0));
    assert_eq!(slab.next_free, S::SLOT_COUNT);
    assert_eq!(slab.storage[S::VEC_OFFSET][0].get(), Some(&22));
    assert_eq!(slab.insert(99), None);
    assert_eq!(slab.storage[S::VEC_OFFSET][0].get(), Some(&22));
}

#[test]
fn client_exhaustion_preserves_state_and_allows_reuse() {
    check_exhaustion::<Client>();
}

#[test]
fn server_exhaustion_preserves_state_and_allows_reuse() {
    check_exhaustion::<Server>();
}
