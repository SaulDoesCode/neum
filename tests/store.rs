//! the memory, end to end, on files in a temporary folder

use neum::{Memory, Mode, gematria::{byte_sum, sums}, store};
use tempfile::TempDir;

fn fresh() -> (TempDir, Memory) {
    let dir = tempfile::tempdir().unwrap();
    let m = Memory::open(dir.path().join("memory"), false).unwrap();
    (dir, m)
}

#[test]
fn saturate_remember_forget() {
    let (_d, m) = fresh();
    let a = m.saturate(b"count your says", None).unwrap();
    let b = m.saturate(b"just a regular friday", None).unwrap();
    assert_eq!(a, vec![0]);
    assert_eq!(b, vec![1]);
    assert_eq!(m.remember(&a).unwrap(), b"count your says");
    assert_eq!(m.forget(&a).unwrap(), b"count your says");
    assert!(m.remember(&a).is_err());
    assert!(m.forget(&a).is_err(), "forgetting twice");
    // the freed id is handed out again before a new one
    assert_eq!(m.saturate(b"again", None).unwrap(), vec![0]);
    assert_eq!(m.saturate(b"new", None).unwrap(), vec![2]);
}

#[test]
fn ids_grow_a_byte_when_they_run_out() {
    assert_eq!(store::next_id(&[0]).unwrap(), vec![1]);
    assert_eq!(store::next_id(&[255]).unwrap(), vec![0, 0]);
    assert_eq!(store::next_id(&[0, 255]).unwrap(), vec![1, 0]);
    assert_eq!(store::next_id(&[255, 255, 255]).unwrap(), vec![0, 0, 0, 0]);
    assert!(store::next_id(&[255, 255, 255, 255]).is_err());
    assert!(store::id_order(&[0, 0]) > store::id_order(&[255]));
}

#[test]
fn saturate_at_an_id_moves_the_index() {
    let (_d, m) = fresh();
    let id = m.saturate(b"abc", None).unwrap();
    m.saturate(b"zzz", Some(id.clone())).unwrap();
    assert!(m.gsearch(sums(b"abc").0, Mode::Ordinal).unwrap().is_empty());
    assert_eq!(m.gsearch(sums(b"zzz").0, Mode::Ordinal).unwrap()[&id], b"zzz");
    // a hand-written id past the counter moves the counter
    m.saturate(b"far", Some(vec![9, 9])).unwrap();
    assert_eq!(m.saturate(b"next", None).unwrap(), vec![9, 10]);
    assert!(m.saturate(b"bad", Some(vec![])).is_err());
    assert!(m.saturate(b"bad", Some(vec![1, 2, 3, 4, 5])).is_err());
}

#[test]
fn every_sum_has_a_bucket() {
    let (_d, m) = fresh();
    let id = m.saturate(b"ordinary cunt", None).unwrap();
    let other = m.saturate("ünï".as_bytes(), None).unwrap();
    let (o, r) = sums(b"ordinary cunt");
    assert!(m.gsearch(o, Mode::Ordinal).unwrap().contains_key(&id));
    assert!(m.gsearch(r, Mode::Reversed).unwrap().contains_key(&id));
    assert!(m.gsearch(byte_sum(b"ordinary cunt"), Mode::Bytes).unwrap().contains_key(&id));
    // non-letters still have a byte sum
    assert!(m.gsearch(byte_sum("ünï".as_bytes()), Mode::Bytes).unwrap().contains_key(&other));
    assert!(m.gsearch(12345, Mode::Bytes).unwrap().is_empty());
    // raw, non UTF-8 bytes too
    let raw = m.saturate(&[0, 159, 146, 150], None).unwrap();
    assert!(m.gsearch(byte_sum(&[0, 159, 146, 150]), Mode::Bytes).unwrap().contains_key(&raw));
}

#[test]
fn search_by_words() {
    let (_d, m) = fresh();
    let a = m.saturate(b"the cat sat", None).unwrap();
    let b = m.saturate(b"the dog sat", None).unwrap();
    let (found, that_one) = m.search(b"the cat sat", false).unwrap();
    assert!(found.contains_key(&a) && !found.contains_key(&b), "3 of 3 words needed at 75%");
    assert_eq!(that_one, Some(a.clone()));
    let (found, _) = m.search_with(b"the cat sat", false, 0.5).unwrap();
    assert!(found.contains_key(&a) && found.contains_key(&b), "2 of 3 is enough at 50%");
    // saturating search stores what wasn't there, once
    let (found, new) = m.search(b"a bird", true).unwrap();
    let new = new.unwrap();
    assert_eq!(found[&new], b"a bird");
    let (_, again) = m.search(b"a bird", true).unwrap();
    assert_eq!(again, Some(new));
    assert!(m.search(b"", false).is_err());
}

#[test]
fn find_and_find_or_saturate() {
    let (_d, m) = fresh();
    assert_eq!(m.find(b"x").unwrap(), None);
    let id = m.find_or_saturate(b"x").unwrap();
    assert_eq!(m.find(b"x").unwrap(), Some(id.clone()));
    assert_eq!(m.find_or_saturate(b"x").unwrap(), id);
}

#[test]
fn lists() {
    let (_d, m) = fresh();
    let list = m.saturate(b"songs", None).unwrap();
    let a = m.saturate(b"wasn't our flow", None).unwrap();
    let b = m.saturate(b"say what you want", None).unwrap();
    m.add_to_list(&list, &a).unwrap();
    m.add_to_list(&list, &b).unwrap();
    assert!(m.add_to_list(&list, &[77]).is_err(), "lists never point at nothing");
    assert_eq!(m.get_list(&list).unwrap().len(), 2);
    assert_eq!(m.lists_of(&a).unwrap(), vec![list.clone()]);
    m.rm_from_list(&list, &a).unwrap();
    assert_eq!(m.get_list(&list).unwrap().len(), 1);
    assert!(m.lists_of(&a).unwrap().is_empty());
    // forgetting a member takes it off its lists
    m.forget(&b).unwrap();
    assert!(m.get_list(&list).unwrap().is_empty());
    // forgetting the list lets its members go
    let c = m.saturate(b"c", None).unwrap();
    m.add_to_list(&list, &c).unwrap();
    m.forget(&list).unwrap();
    assert!(m.lists_of(&c).unwrap().is_empty());
}

#[test]
fn notes() {
    let (_d, m) = fresh();
    let on = m.saturate(b"friday", None).unwrap();
    let n1 = m.saturate(b"get very nervous", None).unwrap();
    let n2 = m.saturate(b"can't have it both ways", None).unwrap();
    m.note_on(&n1, &on).unwrap();
    m.notes_on([(on.as_slice(), n2.as_slice())]).unwrap();
    assert!(m.note_on(&on, &on).is_err(), "not a note on itself");
    assert!(m.note_on(&[99], &on).is_err(), "notes must exist");
    let notes = m.get_notes_on(&on).unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[&n1], b"get very nervous");
    assert_eq!(m.noted_on(&n1).unwrap(), vec![on.clone()]);
    m.rm_note(&n1, &on).unwrap();
    assert_eq!(m.get_notes_on(&on).unwrap().len(), 1);
    assert!(m.noted_on(&n1).unwrap().is_empty());
    // forgetting what notes are on keeps the notes, unlinked
    m.forget(&on).unwrap();
    assert_eq!(m.remember(&n2).unwrap(), b"can't have it both ways");
    assert!(m.noted_on(&n2).unwrap().is_empty());
}

#[test]
fn stats_count_what_is_there() {
    let (_d, m) = fresh();
    let s = m.stats().unwrap();
    assert_eq!((s.memories, s.lists, s.notes, s.reusable), (0, 0, 0, 0));
    assert_eq!(s.schema, store::SCHEMA);
    assert!(s.file_bytes > 0);
    let l = m.saturate(b"l", None).unwrap();
    let a = m.saturate(b"aa", None).unwrap();
    let b = m.saturate(b"bbb", None).unwrap();
    m.add_to_list(&l, &a).unwrap();
    m.add_to_list(&l, &b).unwrap();
    m.note_on(&a, &b).unwrap();
    let gone = m.saturate(b"gone", None).unwrap();
    m.forget(&gone).unwrap();
    let s = m.stats().unwrap();
    assert_eq!(s.memories, 3);
    assert_eq!(s.stored_bytes, 6);
    assert_eq!((s.lists, s.listed), (1, 2));
    assert_eq!((s.notes, s.noted), (1, 1));
    assert_eq!(s.reusable, 1);
    assert_eq!(s.allocated, Some(gone));
}

#[test]
fn backup_and_restore() {
    let (d, m) = fresh();
    let l = m.saturate(b"list", None).unwrap();
    let a = m.saturate(b"text", None).unwrap();
    let raw = m.saturate(&[0xff, 0x00, 0xfe], None).unwrap();
    m.add_to_list(&l, &a).unwrap();
    m.note_on(&raw, &a).unwrap();
    let gone = m.saturate(b"gone", None).unwrap();
    m.forget(&gone).unwrap();
    let json = d.path().join("backups").join("all.json");
    assert_eq!(m.backup_to_file(&json).unwrap(), 3);

    let other = Memory::open(d.path().join("other"), false).unwrap();
    assert_eq!(other.restore_from_file(&json).unwrap(), 3);
    assert_eq!(other.remember(&raw).unwrap(), vec![0xff, 0x00, 0xfe]);
    assert_eq!(other.get_list(&l).unwrap().len(), 1);
    assert_eq!(other.noted_on(&raw).unwrap(), vec![a.clone()]);
    assert!(other.gsearch(byte_sum(b"text"), Mode::Bytes).unwrap().contains_key(&a), "indexes rebuilt");
    assert_eq!(other.stats().unwrap(), neum::Stats { path: other.path().into(), file_bytes: other.stats().unwrap().file_bytes, ..m.stats().unwrap() });
    // the freed id came along
    assert_eq!(other.saturate(b"new", None).unwrap(), gone);
    // and it won't mix into a memory that has things in it
    assert!(other.restore_from_file(&json).is_err());
}

#[test]
fn repair_rebuilds_and_drops() {
    let (_d, m) = fresh();
    let a = m.saturate(b"alpha beta", None).unwrap();
    m.repair().unwrap();
    assert!(m.search(b"alpha beta", false).unwrap().0.contains_key(&a));
    assert!(m.gsearch(byte_sum(b"alpha beta"), Mode::Bytes).unwrap().contains_key(&a));
}

#[test]
fn reopening_keeps_everything_and_compacts() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("nested").join("folder").join("memory");
    let id = {
        let m = Memory::open(&p, true).unwrap();
        m.saturate(b"kept", None).unwrap()
    };
    let m = Memory::open(&p, true).unwrap();
    assert_eq!(m.remember(&id).unwrap(), b"kept");
    assert!(Memory::open(&p, false).is_err(), "a memory opens once");
    assert!(Memory::open(d.path(), false).is_err(), "a folder isn't a memory");
    assert!(Memory::open("", false).is_err());
}

#[test]
fn a_schema_1_memory_gets_its_byte_bucket() {
    use redb::{Database, TableDefinition};
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("old");
    {
        // what 0.1 wrote: memories, no byte sum bucket, no version
        const MEMORIES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("MEMORIES");
        const STATE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("STATE");
        let db = Database::create(&p).unwrap();
        let wx = db.begin_write().unwrap();
        wx.open_table(MEMORIES).unwrap().insert([0u8].as_slice(), b"old times".as_slice()).unwrap();
        wx.open_table(STATE).unwrap().insert(b"allocated".as_slice(), [0u8].as_slice()).unwrap();
        wx.commit().unwrap();
    }
    let m = Memory::open(&p, false).unwrap();
    assert_eq!(m.stats().unwrap().schema, store::SCHEMA);
    assert!(m.gsearch(byte_sum(b"old times"), Mode::Bytes).unwrap().contains_key(&vec![0]));
    assert!(m.gsearch(sums(b"old times").0, Mode::Ordinal).unwrap().contains_key(&vec![0]));
    assert!(m.search(b"old times", false).unwrap().0.contains_key(&vec![0]));
    assert_eq!(m.saturate(b"new", None).unwrap(), vec![1], "the counter carried on");
}
