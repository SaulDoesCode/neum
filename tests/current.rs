//! the one open memory: opening, swapping and closing like game saves. One test, since
//! the open memory is shared by the whole test binary.

use neum::{Mode, gematria::byte_sum, store};

#[test]
fn saves() {
    let d = tempfile::tempdir().unwrap();
    let (one, two) = (d.path().join("one"), d.path().join("two"));

    assert!(!store::is_open());
    assert!(store::saturate(b"nowhere", None).is_err(), "nothing open, nothing done");
    assert!(store::current().unwrap_err().to_string().contains("no memory open"));

    store::open(&one, false).unwrap();
    assert_eq!(store::current_path().as_deref(), Some(one.as_path()));
    let a = store::saturate(b"in one", None).unwrap();

    // the same file again is the same memory
    let again = store::open(&one, false).unwrap();
    assert_eq!(again.path(), one.as_path());
    // a handle kept past a swap keeps its file locked, so let it go
    drop(again);

    // a second save, then back
    store::open(&two, false).unwrap();
    assert!(store::remember(&a).is_err(), "two has nothing yet");
    let b = store::saturate(b"in two", None).unwrap();
    assert_eq!(a, b, "each save counts its own ids");
    store::open(&one, false).unwrap();
    assert_eq!(store::remember(&a).unwrap(), b"in one");
    assert!(store::gsearch(byte_sum(b"in one"), Mode::Bytes).unwrap().contains_key(&a));

    // one that can't open leaves the open one be
    assert!(store::open(d.path(), false).is_err());
    assert_eq!(store::current_path().as_deref(), Some(one.as_path()));

    assert_eq!(store::close().as_deref(), Some(one.as_path()));
    assert!(!store::is_open());
    assert_eq!(store::close(), None);

    // closed means the file is free for anyone
    let m = neum::Memory::open(&one, false).unwrap();
    assert_eq!(m.remember(&a).unwrap(), b"in one");
}
