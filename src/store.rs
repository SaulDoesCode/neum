//! the memory: a redb file of memories, their indexes, lists and notes.
//!
//! One memory is open at a time, like a game save: [`open`] swaps it for another file,
//! [`close`] puts it away. [`Memory`] is the same thing without the global, which is what
//! tests use.

use anyhow::anyhow;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use rayon::prelude::*;
use redb::{
    Database, MultimapTableDefinition, ReadableDatabase, ReadableMultimapTable, ReadableTable,
    ReadableTableMetadata, TableDefinition, WriteTransaction,
};

use serde_json::{Value, json};

use crate::gematria::{Mode, byte_sum, sums, unique_tokens};

const ALLOCATED: &[u8] = b"allocated".as_slice();
const VERSION: &[u8] = b"version".as_slice();

/// 1: memories, indexes, lists and notes. 2: the byte sum bucket.
pub const SCHEMA: u8 = 2;

/// how much of a query's distinct tokens a memory must hold to be found, unless told otherwise
pub const DEFAULT_THRESHOLD: f32 = 0.75;

macro_rules! multimap {
    ($i:ident, $k:ty, $v:ty) => {
        const $i: MultimapTableDefinition<$k, $v> = MultimapTableDefinition::new(stringify!($i));
    };
}

macro_rules! map {
    ($i:ident, $k:ty, $v:ty) => {
        const $i: TableDefinition<$k, $v> = TableDefinition::new(stringify!($i));
    };
}

map!(STATE, &[u8], &[u8]);
map!(MEMORIES, &[u8], &[u8]);
multimap!(MEMORY_INDEX, &[u8], &[u8]);

multimap!(TO_REUSE, &[u8], &[u8]);

multimap!(LISTS, &[u8], &[u8]);
multimap!(LISTED_IN, &[u8], &[u8]);

multimap!(NOTES, &[u8], &[u8]);
multimap!(NOTED_ON, &[u8], &[u8]);

multimap!(MEMORY_GEMATRIA, u64, &[u8]);
multimap!(MEMORY_GEMATRIA_REVERSED, u64, &[u8]);
multimap!(MEMORY_BYTE_SUM, u64, &[u8]);

/// memories by id
pub type Found = HashMap<Vec<u8>, Vec<u8>>;

/// what the escape menu says about a memory
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub path: PathBuf,
    /// the file's size on disk
    pub file_bytes: u64,
    pub memories: u64,
    /// the bytes of every memory added up
    pub stored_bytes: u64,
    /// distinct lists, and how many memories are in them altogether
    pub lists: u64,
    pub listed: u64,
    /// note links, and how many memories have at least one note on them
    pub notes: u64,
    pub noted: u64,
    /// the last id handed out, and how many freed ids wait to be handed out again
    pub allocated: Option<Vec<u8>>,
    pub reusable: u64,
    pub schema: u8,
}

pub struct Memory {
    db: Database,
    path: PathBuf,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memory").field("path", &self.path).finish()
    }
}

fn ensure_tables(wx: &WriteTransaction) -> anyhow::Result<()> {
    wx.open_table(STATE)?;
    wx.open_table(MEMORIES)?;
    for t in [MEMORY_INDEX, TO_REUSE, LISTS, LISTED_IN, NOTES, NOTED_ON] {
        wx.open_multimap_table(t)?;
    }
    for t in [MEMORY_GEMATRIA, MEMORY_GEMATRIA_REVERSED, MEMORY_BYTE_SUM] {
        wx.open_multimap_table(t)?;
    }
    Ok(())
}

fn schema_of(wx: &WriteTransaction) -> anyhow::Result<u8> {
    Ok(wx.open_table(STATE)?.get(VERSION)?.and_then(|g| g.value().first().copied()).unwrap_or(1))
}

// ---- ids ----

/// ids are big-endian counters that grow a byte when they run out: [0]..[255], [0,0]..,
/// up to 4 bytes (the same sequence your `saturate` made)
pub fn next_id(prev: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut b = prev.to_vec();
    for i in (0..b.len()).rev() {
        if b[i] == u8::MAX {
            b[i] = 0;
        } else {
            b[i] += 1;
            return Ok(b);
        }
    }
    if b.len() >= 4 {
        return Err(anyhow!("ran out of 4-byte ids"));
    }
    Ok(vec![0; b.len() + 1])
}

/// which of two ids came later in that sequence
pub fn id_order(id: &[u8]) -> (usize, &[u8]) {
    (id.len(), id)
}

fn exists(wx: &WriteTransaction, id: &[u8]) -> anyhow::Result<bool> {
    Ok(wx.open_table(MEMORIES)?.get(id)?.is_some())
}

/// a fresh id: the smallest one to reuse, else one past ALLOCATED, else [0]
fn allocate(wx: &WriteTransaction) -> anyhow::Result<Vec<u8>> {
    let mut reusables = wx.open_multimap_table(TO_REUSE)?;
    let reuse = match reusables.get(ALLOCATED)?.next() {
        Some(r) => Some(r?.value().to_vec()),
        None => None,
    };
    if let Some(id) = reuse {
        reusables.remove(ALLOCATED, id.as_slice())?;
        return Ok(id);
    }
    let mut state = wx.open_table(STATE)?;
    let last = state.get(ALLOCATED)?.map(|g| g.value().to_vec());
    let id = match last {
        Some(last) => next_id(&last)?,
        None => vec![0],
    };
    state.insert(ALLOCATED, id.as_slice())?;
    Ok(id)
}

// ---- indexing ----

fn index(wx: &WriteTransaction, id: &[u8], value: &[u8]) -> anyhow::Result<()> {
    let (f, r) = sums(value);
    wx.open_multimap_table(MEMORY_GEMATRIA)?.insert(f, id)?;
    wx.open_multimap_table(MEMORY_GEMATRIA_REVERSED)?.insert(r, id)?;
    wx.open_multimap_table(MEMORY_BYTE_SUM)?.insert(byte_sum(value), id)?;
    let mut memi = wx.open_multimap_table(MEMORY_INDEX)?;
    for tk in unique_tokens(value) {
        memi.insert(tk, id)?;
    }
    Ok(())
}

fn unindex(wx: &WriteTransaction, id: &[u8], value: &[u8]) -> anyhow::Result<()> {
    let (f, r) = sums(value);
    wx.open_multimap_table(MEMORY_GEMATRIA)?.remove(f, id)?;
    wx.open_multimap_table(MEMORY_GEMATRIA_REVERSED)?.remove(r, id)?;
    wx.open_multimap_table(MEMORY_BYTE_SUM)?.remove(byte_sum(value), id)?;
    let mut memi = wx.open_multimap_table(MEMORY_INDEX)?;
    for tk in unique_tokens(value) {
        memi.remove(tk, id)?;
    }
    Ok(())
}

/// every value of a multimap key, removed, collected (so the table can be used again)
fn take_all(wx: &WriteTransaction, t: MultimapTableDefinition<&[u8], &[u8]>, k: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
    let mut table = wx.open_multimap_table(t)?;
    let mut out = vec![];
    for r in table.remove_all(k)? {
        out.push(r?.value().to_vec());
    }
    Ok(out)
}

fn link_note(wx: &WriteTransaction, note: &[u8], on: &[u8]) -> anyhow::Result<()> {
    if !exists(wx, note)? || !exists(wx, on)? {
        return Err(anyhow!("a note and what it is on must both exist"));
    }
    if note == on {
        return Err(anyhow!("a memory can't be a note on itself"));
    }
    wx.open_multimap_table(NOTES)?.insert(on, note)?;
    wx.open_multimap_table(NOTED_ON)?.insert(note, on)?;
    Ok(())
}

impl Memory {
    /// open the memory at `path`, making it (and its folder) if it isn't there. Older
    /// schemas are brought up to date; with `compact`, the file is compacted too.
    pub fn open(path: impl AsRef<Path>, compact: bool) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.as_os_str().is_empty() {
            return Err(anyhow!("a memory needs a path"));
        }
        if path.is_dir() {
            return Err(anyhow!("{} is a folder, a memory is a file", path.display()));
        }
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty() && !dir.exists()
        {
            std::fs::create_dir_all(dir)?;
        }
        let mut db = Database::create(&path).map_err(|e| match e {
            redb::DatabaseError::DatabaseAlreadyOpen => anyhow!("{} is already open (in another neum?)", path.display()),
            e => anyhow!("couldn't open {}: {e}", path.display()),
        })?;
        let wx = db.begin_write()?;
        ensure_tables(&wx)?;
        if schema_of(&wx)? < SCHEMA {
            // the byte sum bucket (and anything else an index can be rebuilt from)
            repair_in(&wx)?;
            wx.open_table(STATE)?.insert(VERSION, [SCHEMA].as_slice())?;
        }
        wx.commit()?;
        if compact {
            db.compact()?;
        }
        Ok(Memory { db, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    // ---- memories ----

    /// with `None`, store a new memory under a fresh id; with `Some(id)`, put this
    /// value at that id, moving its index entries from the old value (if there was one) to
    /// the new. Returns the id.
    pub fn saturate(&self, quidity: &[u8], t: Option<Vec<u8>>) -> anyhow::Result<Vec<u8>> {
        let wx = self.db.begin_write()?;
        let id = match t {
            None => allocate(&wx)?,
            Some(id) => {
                if id.is_empty() || id.len() > 4 {
                    return Err(anyhow!("ids are 1 to 4 bytes"));
                }
                let old = wx.open_table(MEMORIES)?.get(id.as_slice())?.map(|g| g.value().to_vec());
                if let Some(old) = old {
                    unindex(&wx, &id, &old)?;
                } // an id written by hand is in use now: not to be handed out again
                wx.open_multimap_table(TO_REUSE)?.remove(ALLOCATED, id.as_slice())?;
                let mut state = wx.open_table(STATE)?;
                let last = state.get(ALLOCATED)?.map(|g| g.value().to_vec());
                if last.as_deref().is_none_or(|l| id_order(&id) > id_order(l)) {
                    state.insert(ALLOCATED, id.as_slice())?;
                }
                id
            }
        };
        wx.open_table(MEMORIES)?.insert(id.as_slice(), quidity)?;
        index(&wx, &id, quidity)?;
        wx.commit()?;
        Ok(id)
    }

    pub fn remember(&self, this: &[u8]) -> anyhow::Result<Vec<u8>> {
        match self.db.begin_read()?.open_table(MEMORIES)?.get(this)? {
            Some(ag) => Ok(ag.value().to_vec()),
            None => Err(anyhow!("didn't find that")),
        }
    }

    /// forget a memory and every link to it. Notes are memories of their own, so the
    /// notes on it stay; they are only no longer linked to it.
    pub fn forget(&self, this: &[u8]) -> anyhow::Result<Vec<u8>> {
        let wx = self.db.begin_write()?;
        let old = wx.open_table(MEMORIES)?.remove(this)?.map(|g| g.value().to_vec());
        let Some(old) = old else {
            wx.abort()?;
            return Err(anyhow!("there was nothing with that name anyways"));
        };
        unindex(&wx, this, &old)?;
        wx.open_multimap_table(TO_REUSE)?.insert(ALLOCATED, this)?;
        // the notes on it
        for note in take_all(&wx, NOTES, this)? {
            wx.open_multimap_table(NOTED_ON)?.remove(note.as_slice(), this)?;
        }
        // what it was a note on
        for on in take_all(&wx, NOTED_ON, this)? {
            wx.open_multimap_table(NOTES)?.remove(on.as_slice(), this)?;
        }
        // the lists it was in
        for list in take_all(&wx, LISTED_IN, this)? {
            wx.open_multimap_table(LISTS)?.remove(list.as_slice(), this)?;
        }
        // and, if it was a list, its members' way back to it
        for member in take_all(&wx, LISTS, this)? {
            wx.open_multimap_table(LISTED_IN)?.remove(member.as_slice(), this)?;
        }
        wx.commit()?;
        Ok(old)
    }

    // ---- lists ----

    /// put a memory into a list (it must exist, so lists never point at nothing)
    pub fn add_to_list(&self, ln: &[u8], v: &[u8]) -> anyhow::Result<()> {
        let wx = self.db.begin_write()?;
        if !exists(&wx, v)? {
            wx.abort()?;
            return Err(anyhow!("no memory with that id to list"));
        }
        wx.open_multimap_table(LISTS)?.insert(ln, v)?;
        wx.open_multimap_table(LISTED_IN)?.insert(v, ln)?;
        wx.commit()?;
        Ok(())
    }

    pub fn rm_from_list(&self, ln: &[u8], v: &[u8]) -> anyhow::Result<()> {
        let wx = self.db.begin_write()?;
        wx.open_multimap_table(LISTS)?.remove(ln, v)?;
        wx.open_multimap_table(LISTED_IN)?.remove(v, ln)?;
        wx.commit()?;
        Ok(())
    }

    /// a list's memories by id (empty if the list is empty or unknown)
    pub fn get_list(&self, ln: &[u8]) -> anyhow::Result<Found> {
        let rx = self.db.begin_read()?;
        let lists = rx.open_multimap_table(LISTS)?;
        let memories = rx.open_table(MEMORIES)?;
        let mut hm = HashMap::new();
        for r in lists.get(ln)? {
            let k = r?;
            if let Some(v) = memories.get(k.value())? {
                hm.insert(k.value().to_vec(), v.value().to_vec());
            }
        }
        Ok(hm)
    }

    /// the lists a memory is in
    pub fn lists_of(&self, v: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
        let rx = self.db.begin_read()?;
        let listed = rx.open_multimap_table(LISTED_IN)?;
        let mut out = vec![];
        for r in listed.get(v)? {
            out.push(r?.value().to_vec());
        }
        Ok(out)
    }

    // ---- notes ----

    /// `note` (a memory) is now a note on `on` (another)
    pub fn note_on(&self, note: &[u8], on: &[u8]) -> anyhow::Result<()> {
        let wx = self.db.begin_write()?;
        link_note(&wx, note, on)?;
        wx.commit()?;
        Ok(())
    }

    /// many (on, note) pairs in one transaction (one commit, so one fsync: use this for imports)
    pub fn notes_on<'a>(&self, pairs: impl IntoIterator<Item = (&'a [u8], &'a [u8])>) -> anyhow::Result<()> {
        let wx = self.db.begin_write()?;
        for (on, note) in pairs {
            link_note(&wx, note, on)?;
        }
        wx.commit()?;
        Ok(())
    }

    pub fn rm_note(&self, note: &[u8], on: &[u8]) -> anyhow::Result<()> {
        let wx = self.db.begin_write()?;
        wx.open_multimap_table(NOTES)?.remove(on, note)?;
        wx.open_multimap_table(NOTED_ON)?.remove(note, on)?;
        wx.commit()?;
        Ok(())
    }

    /// get notes on a memory, by key (empty if none)
    pub fn get_notes_on(&self, this: &[u8]) -> anyhow::Result<Found> {
        let rx = self.db.begin_read()?;
        let notes = rx.open_multimap_table(NOTES)?;
        let memories = rx.open_table(MEMORIES)?;
        let mut found = HashMap::new();
        for r in notes.get(this)? {
            let k = r?;
            if let Some(v) = memories.get(k.value())? {
                found.insert(k.value().to_vec(), v.value().to_vec());
            }
        }
        Ok(found)
    }

    /// what a note is on
    pub fn noted_on(&self, note: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
        let rx = self.db.begin_read()?;
        let noted = rx.open_multimap_table(NOTED_ON)?;
        let mut out = vec![];
        for r in noted.get(note)? {
            out.push(r?.value().to_vec());
        }
        Ok(out)
    }

    // ---- search ----

    /// [`Memory::search_with`] at the default threshold
    pub fn search(&self, query: &[u8], also_saturate: bool) -> anyhow::Result<(Found, Option<Vec<u8>>)> {
        self.search_with(query, also_saturate, DEFAULT_THRESHOLD)
    }

    /// memories holding at least `threshold` (0..=1) of the query's distinct tokens. With
    /// `also_saturate`, the query is stored as a memory too unless one already equals it.
    /// The id of the memory equal to the query, if there is one (new or old), comes back
    /// second (and it is in the results).
    pub fn search_with(&self, query: &[u8], also_saturate: bool, threshold: f32) -> anyhow::Result<(Found, Option<Vec<u8>>)> {
        if query.is_empty() { return Err(anyhow!("can't look for nothing")); }
        let tkns = unique_tokens(query);
        let mut out = HashMap::new();
        let mut that_one: Option<Vec<u8>> = None;
        if !tkns.is_empty() {
            let threshold = ((tkns.len() as f32 * threshold.clamp(0.0, 1.0)).ceil() as usize).max(1);
            let rx = self.db.begin_read()?;
            let mi = rx.open_multimap_table(MEMORY_INDEX)?;
            let mem = rx.open_table(MEMORIES)?;
            let mut scores: HashMap<Vec<u8>, usize> = HashMap::new();
            for tkn in &tkns {
                for r in mi.get(*tkn)? {
                    *scores.entry(r?.value().to_vec()).or_insert(0) += 1;
                }
            }
            for (k, hits) in scores {
                if hits >= threshold
                    && let Some(v) = mem.get(k.as_slice())?
                {
                    let val = v.value();
                    if val == query { that_one = Some(k.clone()); }
                    out.insert(k, val.to_vec());
                }
            }
        }
        if that_one.is_none() {
            that_one = if also_saturate && out.par_iter().find_any(|v| v.1 == query).is_none() {
                let id = self.saturate(query, None)?;
                out.insert(id.clone(), query.to_vec());
                Some(id)
            } else { None };
        }
        Ok((out, that_one))
    }

    /// memories whose ordinal, reversed or byte sum is `sum`
    pub fn gsearch(&self, sum: u64, mode: Mode) -> anyhow::Result<Found> {
        let rx = self.db.begin_read()?;
        let mem = rx.open_table(MEMORIES)?;
        let memg = rx.open_multimap_table(match mode {
            Mode::Ordinal => MEMORY_GEMATRIA,
            Mode::Reversed => MEMORY_GEMATRIA_REVERSED,
            Mode::Bytes => MEMORY_BYTE_SUM,
        })?;
        let mut out = HashMap::new();
        for r in memg.get(sum)? {
            let k = r?;
            if let Some(v) = mem.get(k.value())? {
                out.insert(k.value().to_vec(), v.value().to_vec());
            }
        }
        Ok(out)
    }

    /// the id of the memory that is exactly `value`, if there is one
    pub fn find(&self, value: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        let found = self.gsearch(byte_sum(value), Mode::Bytes)?;
        Ok(found.into_iter().find(|(_, v)| v == value).map(|(k, _)| k))
    }

    /// the id of the memory that is exactly `value`, saturating it first if there isn't one
    pub fn find_or_saturate(&self, value: &[u8]) -> anyhow::Result<Vec<u8>> {
        match self.find(value)? {
            Some(id) => Ok(id),
            None => self.saturate(value, None),
        }
    }

    // ---- repair ----

    /// rebuild what can be rebuilt and drop what points at nothing
    pub fn repair(&self) -> anyhow::Result<()> {
        let wx = self.db.begin_write()?;
        repair_in(&wx)?;
        wx.commit()?;
        Ok(())
    }

    /// compact the file, if redb finds anything to give back
    pub fn compact(&mut self) -> anyhow::Result<bool> {
        Ok(self.db.compact()?)
    }

    // ---- stats ----

    pub fn stats(&self) -> anyhow::Result<Stats> {
        let rx = self.db.begin_read()?;
        let mem = rx.open_table(MEMORIES)?;
        let mut stored_bytes = 0;
        for r in mem.iter()? {
            stored_bytes += r?.1.value().len() as u64;
        }
        let keys_and_len = |t: MultimapTableDefinition<&[u8], &[u8]>| -> anyhow::Result<(u64, u64)> {
            let table = rx.open_multimap_table(t)?;
            let (mut keys, mut len) = (0, 0);
            for r in table.iter()? {
                let (_, vs) = r?;
                keys += 1;
                len += vs.len();
            }
            Ok((keys, len))
        };
        let (lists, listed) = keys_and_len(LISTS)?;
        let (noted, notes) = keys_and_len(NOTES)?;
        let (_, reusable) = keys_and_len(TO_REUSE)?;
        let state = rx.open_table(STATE)?;
        let allocated = state.get(ALLOCATED)?.map(|g| g.value().to_vec());
        let schema = state.get(VERSION)?.and_then(|g| g.value().first().copied()).unwrap_or(1);
        Ok(Stats {
            path: self.path.clone(),
            file_bytes: std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0),
            memories: mem.len()?,
            stored_bytes,
            lists,
            listed,
            notes,
            noted,
            allocated,
            reusable,
            schema,
        })
    }

    // ---- backup ----

    /// everything needed to restore, as JSON at `path`. Returns how many memories were written.
    pub fn backup_to_file(&self, path: impl AsRef<Path>) -> anyhow::Result<u64> {
        let rx = self.db.begin_read()?;
        let mem = rx.open_table(MEMORIES)?;
        let mut memories = vec![];
        for r in mem.iter()? {
            let (k, v) = r?;
            memories.push(json!({ "id": hex(k.value()), "value": enc(v.value()) }));
        }
        let pairs = |t: MultimapTableDefinition<&[u8], &[u8]>| -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            let table = rx.open_multimap_table(t)?;
            let mut out = vec![];
            for r in table.iter()? {
                let (k, vs) = r?;
                for v in vs {
                    out.push((k.value().to_vec(), v?.value().to_vec()));
                }
            }
            Ok(out)
        };
        let lists: Vec<Value> = pairs(LISTS)?.iter().map(|(ln, id)| json!({ "list": enc(ln), "id": hex(id) })).collect();
        let notes: Vec<Value> = pairs(NOTES)?.iter().map(|(on, note)| json!({ "on": hex(on), "note": hex(note) })).collect();
        let reuse: Vec<String> = pairs(TO_REUSE)?.iter().map(|(_, id)| hex(id)).collect();
        let allocated = rx.open_table(STATE)?.get(ALLOCATED)?.map(|g| hex(g.value()));
        let all = json!({
            "neum": SCHEMA,
            "allocated": allocated,
            "reuse": reuse,
            "memories": memories,
            "lists": lists,
            "notes": notes,
        });
        if let Some(dir) = path.as_ref().parent()
            && !dir.as_os_str().is_empty() && !dir.exists()
        {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string(&all)?)?;
        Ok(mem.len()?)
    }

    /// read a backup into an empty memory (refuses to mix one into memories already there)
    pub fn restore_from_file(&self, path: impl AsRef<Path>) -> anyhow::Result<u64> {
        let all: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let wx = self.db.begin_write()?;
        if !wx.open_table(MEMORIES)?.is_empty()? {
            wx.abort()?;
            return Err(anyhow!("restore wants an empty memory (open a new one first)"));
        }
        let arr = |k: &str| all.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        let field = |v: &Value, k: &str| -> anyhow::Result<String> {
            Ok(v.get(k).and_then(Value::as_str).ok_or_else(|| anyhow!("missing {k}"))?.to_string())
        };
        let mut n = 0;
        for m in arr("memories") {
            let id = unhex(&field(&m, "id")?)?;
            let v = dec(m.get("value").ok_or_else(|| anyhow!("missing value"))?)?;
            wx.open_table(MEMORIES)?.insert(id.as_slice(), v.as_slice())?;
            n += 1;
        }
        for l in arr("lists") {
            let ln = dec(l.get("list").ok_or_else(|| anyhow!("missing list"))?)?;
            wx.open_multimap_table(LISTS)?.insert(ln.as_slice(), unhex(&field(&l, "id")?)?.as_slice())?;
        }
        for x in arr("notes") {
            let (on, note) = (unhex(&field(&x, "on")?)?, unhex(&field(&x, "note")?)?);
            wx.open_multimap_table(NOTES)?.insert(on.as_slice(), note.as_slice())?;
        }
        for id in arr("reuse") {
            let id = unhex(id.as_str().ok_or_else(|| anyhow!("reuse ids are hex strings"))?)?;
            wx.open_multimap_table(TO_REUSE)?.insert(ALLOCATED, id.as_slice())?;
        }
        if let Some(a) = all.get("allocated").and_then(Value::as_str) {
            wx.open_table(STATE)?.insert(ALLOCATED, unhex(a)?.as_slice())?;
        }
        // indexes, reverse tables and the counter follow from what was read
        repair_in(&wx)?;
        wx.commit()?;
        Ok(n)
    }
}

fn repair_in(wx: &WriteTransaction) -> anyhow::Result<()> {
    let mut all: Vec<(Vec<u8>, Vec<u8>)> = vec![];
    for r in wx.open_table(MEMORIES)?.iter()? {
        let (k, v) = r?;
        all.push((k.value().to_vec(), v.value().to_vec()));
    }
    let live: HashSet<Vec<u8>> = all.iter().map(|(k, _)| k.clone()).collect();

    // indexes from scratch
    wx.delete_multimap_table(MEMORY_INDEX)?;
    wx.delete_multimap_table(MEMORY_GEMATRIA)?;
    wx.delete_multimap_table(MEMORY_GEMATRIA_REVERSED)?;
    wx.delete_multimap_table(MEMORY_BYTE_SUM)?;
    for (k, v) in &all {
        index(wx, k, v)?;
    }

    // lists: keep live members, rebuild the reverse
    let pairs = multimap_pairs(wx, LISTS)?;
    wx.delete_multimap_table(LISTS)?;
    wx.delete_multimap_table(LISTED_IN)?;
    {
        let mut lists = wx.open_multimap_table(LISTS)?;
        let mut listed = wx.open_multimap_table(LISTED_IN)?;
        for (ln, v) in pairs.into_iter().filter(|(_, v)| live.contains(v)) {
            lists.insert(ln.as_slice(), v.as_slice())?;
            listed.insert(v.as_slice(), ln.as_slice())?;
        }
    }

    // notes: every link known from either side, if both ends are live
    let mut links: HashSet<(Vec<u8>, Vec<u8>)> = multimap_pairs(wx, NOTES)?.into_iter().collect();
    links.extend(multimap_pairs(wx, NOTED_ON)?.into_iter().map(|(note, on)| (on, note)));
    wx.delete_multimap_table(NOTES)?;
    wx.delete_multimap_table(NOTED_ON)?;
    {
        let mut notes = wx.open_multimap_table(NOTES)?;
        let mut noted = wx.open_multimap_table(NOTED_ON)?;
        for (on, note) in links.into_iter().filter(|(on, note)| on != note && live.contains(on) && live.contains(note)) {
            notes.insert(on.as_slice(), note.as_slice())?;
            noted.insert(note.as_slice(), on.as_slice())?;
        }
    }

    // ids to reuse: only the ones not in use
    let reusable: Vec<Vec<u8>> = multimap_pairs(wx, TO_REUSE)?.into_iter().map(|(_, id)| id).collect();
    wx.delete_multimap_table(TO_REUSE)?;
    {
        let mut reuse = wx.open_multimap_table(TO_REUSE)?;
        for id in reusable.into_iter().filter(|id| !live.contains(id)) {
            reuse.insert(ALLOCATED, id.as_slice())?;
        }
    }

    // the counter at least as far as the largest id in use
    let mut state = wx.open_table(STATE)?;
    let last = state.get(ALLOCATED)?.map(|g| g.value().to_vec());
    let top = live.iter().max_by(|a, b| id_order(a).cmp(&id_order(b))).cloned();
    if let Some(top) = top
        && last.as_deref().is_none_or(|l| id_order(&top) > id_order(l))
    {
        state.insert(ALLOCATED, top.as_slice())?;
    }
    drop(state);
    // tables emptied above come back even when nothing was put in them
    ensure_tables(wx)
}

fn multimap_pairs(wx: &WriteTransaction, t: MultimapTableDefinition<&[u8], &[u8]>) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let table = wx.open_multimap_table(t)?;
    let mut out = vec![];
    for r in table.iter()? {
        let (k, vs) = r?;
        for v in vs {
            out.push((k.value().to_vec(), v?.value().to_vec()));
        }
    }
    Ok(out)
}

// ---- backup encoding ----

/// bytes as JSON: {"text": ..} when they are UTF-8, else {"hex": ..}
pub fn enc(b: &[u8]) -> Value {
    match std::str::from_utf8(b) {
        Ok(s) => json!({ "text": s }),
        Err(_) => json!({ "hex": hex(b) }),
    }
}

pub fn dec(v: &Value) -> anyhow::Result<Vec<u8>> {
    if let Some(s) = v.get("text").and_then(Value::as_str) {
        return Ok(s.as_bytes().to_vec());
    }
    let h = v.get("hex").and_then(Value::as_str).ok_or_else(|| anyhow!("neither text nor hex: {v}"))?;
    unhex(h)
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(h: &str) -> anyhow::Result<Vec<u8>> {
    if !h.len().is_multiple_of(2) {
        return Err(anyhow!("odd hex length"));
    }
    if !h.is_ascii() {
        return Err(anyhow!("hex is ascii"));
    }
    (0..h.len()).step_by(2).map(|i| Ok(u8::from_str_radix(&h[i..i + 2], 16)?)).collect()
}

// ---- the open memory ----

static CURRENT: RwLock<Option<Arc<Memory>>> = RwLock::new(None);

/// the memory that's open, or why there isn't one
pub fn current() -> anyhow::Result<Arc<Memory>> {
    CURRENT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| anyhow!("no memory open (esc to open one)"))
}

pub fn is_open() -> bool {
    CURRENT.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// where the open memory lives
pub fn current_path() -> Option<PathBuf> {
    CURRENT.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|m| m.path.clone())
}

/// open the memory at `path` in place of whichever was open. The old one stays open if the
/// new one can't be.
pub fn open(path: impl AsRef<Path>, compact: bool) -> anyhow::Result<Arc<Memory>> {
    let path = path.as_ref();
    let mut cur = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = cur.as_ref()
        && same_file(m.path(), path)
    {
        return Ok(m.clone());
    }
    let m = Arc::new(Memory::open(path, compact)?);
    *cur = Some(m.clone());
    Ok(m)
}

/// put the open memory away. Returns where it was.
pub fn close() -> Option<PathBuf> {
    let old = CURRENT.write().unwrap_or_else(|e| e.into_inner()).take();
    old.map(|m| m.path.clone())
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

// the same calls on whichever memory is open

pub fn saturate(quidity: &[u8], t: Option<Vec<u8>>) -> anyhow::Result<Vec<u8>> { current()?.saturate(quidity, t) }
pub fn remember(this: &[u8]) -> anyhow::Result<Vec<u8>> { current()?.remember(this) }
pub fn forget(this: &[u8]) -> anyhow::Result<Vec<u8>> { current()?.forget(this) }
pub fn add_to_list(ln: &[u8], v: &[u8]) -> anyhow::Result<()> { current()?.add_to_list(ln, v) }
pub fn rm_from_list(ln: &[u8], v: &[u8]) -> anyhow::Result<()> { current()?.rm_from_list(ln, v) }
pub fn get_list(ln: &[u8]) -> anyhow::Result<Found> { current()?.get_list(ln) }
pub fn lists_of(v: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> { current()?.lists_of(v) }
pub fn note_on(note: &[u8], on: &[u8]) -> anyhow::Result<()> { current()?.note_on(note, on) }
pub fn rm_note(note: &[u8], on: &[u8]) -> anyhow::Result<()> { current()?.rm_note(note, on) }
pub fn get_notes_on(this: &[u8]) -> anyhow::Result<Found> { current()?.get_notes_on(this) }
pub fn noted_on(note: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> { current()?.noted_on(note) }
pub fn search(query: &[u8], also_saturate: bool) -> anyhow::Result<(Found, Option<Vec<u8>>)> { current()?.search(query, also_saturate) }
pub fn search_with(query: &[u8], also_saturate: bool, threshold: f32) -> anyhow::Result<(Found, Option<Vec<u8>>)> { current()?.search_with(query, also_saturate, threshold) }
pub fn gsearch(sum: u64, mode: Mode) -> anyhow::Result<Found> { current()?.gsearch(sum, mode) }
pub fn find(value: &[u8]) -> anyhow::Result<Option<Vec<u8>>> { current()?.find(value) }
pub fn find_or_saturate(value: &[u8]) -> anyhow::Result<Vec<u8>> { current()?.find_or_saturate(value) }
pub fn repair() -> anyhow::Result<()> { current()?.repair() }
pub fn stats() -> anyhow::Result<Stats> { current()?.stats() }
pub fn backup_memories_to_file(path: impl AsRef<Path>) -> anyhow::Result<u64> { current()?.backup_to_file(path) }
pub fn restore_from_file(path: impl AsRef<Path>) -> anyhow::Result<u64> { current()?.restore_from_file(path) }
