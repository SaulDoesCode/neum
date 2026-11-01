use anyhow::anyhow;

use zgui::{prelude::*};
use zgui_ui::prelude::*;

use std::{
    sync::{LazyLock}, //time::{SystemTime, UNIX_EPOCH}, //simd::{cmp::SimdPartialEq, prelude::*},
    collections::{HashMap, HashSet}
};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use rayon::prelude::*;
use redb::{
    Database, MultimapTableDefinition, ReadableDatabase, ReadableMultimapTable, ReadableTable,
    ReadableTableMetadata, TableDefinition, WriteTransaction,
};

use serde_json::{Value, json};

#[component]
fn Gematria() -> impl IntoView {
    let txt = RwSignal::new_local(String::new());
    let alt_name = RwSignal::new_local(String::new());
    let osum = RwSignal::new(0_u64);
    let rsum = RwSignal::new(0_u64);
    let sch = RwSignal::new(false);
    let o_or_r = RwSignal::new(true);
    let sat = RwSignal::new(true);
    let display_output: RwSignal<HashMap<Vec<u8>, Vec<u8>>> = RwSignal::new(HashMap::new());
    let esc_mode = RwSignal::new(false);
    let term_ctx_menu = RwSignal::new(false);

    view! {
        row(class = "spread",
            on:click = {move |_ev| {
                esc_mode.update(|n| *n = false);
            }}
        ) {{move || { //  'ordinary cunt' sums to 162/162
            let mut els = vec![];
            for (k, v) in display_output.get() {
                if let Ok(v) = String::from_utf8(v) {
                    let ov = v.to_string();
                    let key = format!("{:?}", k).replace("\"", "").replace("[", "").replace("]", "");
                    let (o, r) = sums(v.as_bytes());
                    let culr = color_for_sum(if o_or_r.get() { r } else { o });
                    let unrlnclr = format!("{:?},{:?},{:?}", culr[0], culr[1], culr[2]);

                    els.push(view!{
                        box(
                            class = "otxt",
                            style = format!("border-bottom: 3px solid rgb({});", unrlnclr),
                            on:double_click = move |_| {
                                let key = key.clone();
                                txt.update(move |t| *t = key);
                            },
                            on:pointer_down = move |ev| {
                                if let Some(btn) = ev.button {
                                    if btn == PointerButton::Secondary {
                                        if ev.modifiers.control() {
                                            let n = (osum.get() as f64 * rsum.get() as f64).sqrt().round() as u64;
                                            txt.update(move |t| *t = n.to_string());
                                            if let Ok(out) = gsearch(n, !o_or_r.get()) {
                                                display_output.update(move |disout| *disout = out);
                                            }
                                        }
                                    }
                                }
                            },
                            on:click = move |_ev| {
                                let ov = ov.to_string();
                                txt.update(move |t| *t = ov.to_string());
                                let (o, r) = sums(txt.get().as_bytes());
                                if osum.get() != o { osum.update(move |n| *n = o); }
                                if rsum.get() != r { rsum.update(move |n| *n = r); }
                            }
                        ) { text() {{v}} }
                    });
                }
            }
            els
        }}}
        column(class = "col") {
            if move || term_ctx_menu.get() {
                row(class = "term-menu") {
                    Input(
                        value = alt_name,
                    )
                }
            }
            row(
                class = "inp",
                on:pointer_down = move |ev| {
                    if let Some(btn) = ev.button {
                        if btn == PointerButton::Secondary {
                            term_ctx_menu.update(|n| *n = !*n);
                        } else if btn == PointerButton::Middle {
                            let n = txt.get();
                            alt_name.update(move |ln| *ln = n);
                            txt.update(move |t| *t = String::new());
                        }
                    }
                },
            ) {
                Input(
                    value = txt,
                    label = "type text to sum with gematria",
                    on:input = move |_| {
                        let (o, r) = sums(txt.get().as_bytes());
                        osum.update(move |n| *n = o);
                        rsum.update(move |n| *n = r);
                    },
                    on:click = move |_ev| {
                        esc_mode.update(|n| *n = false);
                    },
                    on:key_down = move |ev| match &ev.key {
                        Key::Named(NamedKey::Enter) => {
                            if sch.get() {
                                let t = txt.get();
                                if t.len() == 0 {
                                    let alt = alt_name.get();
                                    if alt.len() != 0 {
                                        let sr = search(alt.as_bytes(), false);
                                        if let Ok((_found, that_one)) = sr {
                                            if let Some(l) = that_one {
                                                match get_list(&l) {
                                                    Ok(lst) => display_output.update(move |disout| *disout = lst),
                                                    Err(e) => println!("trouble adding thing to list: {e}")
                                                }
                                            } else {
                                                display_output.update(move |disout| disout.clear());
                                                println!("look no find.. that one");
                                            }
                                        } else if let Err(e) = sr {
                                            println!("search error: {e}");
                                        } 
                                    }
                                } else {
                                    let r = search(t.as_bytes(), sat.get());
                                    let mut k = vec![];
                                    if let Ok(out) = r {
                                        display_output.update(move |disout| *disout = out.0);
                                        if let Some(key) = out.1 { k.extend_from_slice(&key); }
                                    }
                                    let an = alt_name.get();
                                    if an.len() != 0 {
                                        let sr = search(an.as_bytes(), true);
                                        if let Ok((_found, that_one)) = sr {
                                            if let Some(l) = that_one {
                                                if let Err(e) = add_to_list(&l, &k) {
                                                    println!("trouble adding thing to list: {e}");
                                                }
                                            } else {
                                                println!("look no find.. that one");
                                            }
                                        } else if let Err(e) = sr {
                                            display_output.update(move |disout| disout.clear());
                                            println!("search error: {e}");
                                        }
                                    }
                                }
                            } else {
                                let t = txt.get();
                                let an = alt_name.get();
                                if an.len() != 0 && sat.get() && t.parse::<u64>().is_err() {
                                    match gsearch(osum.get(), false) {
                                        Ok(mut fnd) => {
                                            let tb = t.as_bytes().to_vec();
                                            if fnd.par_iter().find_any(|(_k, v)| tb.eq(*v)).is_none() { 
                                                if let Ok(k) = saturate(&tb, None) {
                                                    let sr = search(an.as_bytes(), true);
                                                    if let Ok((_found, that_one)) = sr {
                                                        if let Some(l) = that_one {
                                                            if let Err(e) = note_on(&l, &k) {
                                                                println!("trouble noting on thing: {e}");
                                                            }
                                                        } else {
                                                            println!("look no find.. that one");
                                                        }
                                                        fnd.insert(k, tb);
                                                    } else if let Err(e) = sr {
                                                        display_output.update(move |disout| disout.clear());
                                                        println!("search error: {e}");
                                                    }
                                                }
                                            }
                                        },
                                        Err(e) => println!("search error: {e}")
                                    }
                                } else if let Ok(mut out) = gsearch(if let Ok(sum) = t.parse::<u64>() && !ev.modifiers == Modifiers::CONTROL { sum } else if o_or_r.get() { osum.get() } else { rsum.get() }, !o_or_r.get()) {
                                    if sat.get() {
                                        if t.parse::<u64>().is_err() {
                                            let tb = txt.get().as_bytes().to_vec();
                                            if out.par_iter().find_any(|(_k, v)| tb.eq(*v)).is_none() { 
                                                if let Ok(key) = saturate(&tb, None) { out.insert(key, tb); }
                                            }
                                        }
                                    }
                                    display_output.update(move |disout| *disout = out);
                                }
                            }
                        }
                        Key::Named(NamedKey::Delete) => {
                            if let Ok(key) = parse_u8_list(&txt.get()) {
                                let alt = alt_name.get();
                                if alt.len() != 0 {
                                     let sr = search(alt.as_bytes(), false);
                                        if let Ok((_found, that_one)) = sr {
                                            if let Some(l) = that_one {
                                                match rm_from_list(&l, &key) {
                                                    Ok(()) => display_output.update(move |disout| {
                                                        disout.remove_entry(&key);
                                                    }),
                                                    Err(e) => println!("trouble removing thing from list: {e}")
                                                }
                                            } else {
                                                println!("rm from list, illegal 'nothing' removal attempt");
                                            }
                                        } else if let Err(e) = sr {
                                            println!("rm from list, search error: {e}");
                                        }
                                } else if let Ok(was) = forget(&key) {
                                    display_output.update(move |disout| {
                                        disout.remove_entry(&key);
                                    });
                                    if let Ok(stwing) = String::from_utf8(was) {
                                        txt.update(move |t| *t = stwing);
                                    }
                                }
                            }
                        }
                        Key::Named(NamedKey::Escape) => {
                            esc_mode.update(|n| *n = !*n);
                        }
                        _k => {}
                    }
                )
            }
            row(
                class = "sums",
                on:click = {move |_ev| {
                    esc_mode.update(|n| *n = false);
                }},
            ) {
                column() {
                    column() {
                        text(class = "number") {{move || format!("{}",  osum.get())}}
                        text(class = "value") {{move || format!("ordinal ")}}
                    }
                }
                row(class = "toggles") {
                    text(class="sch", class:ing = move || sch.get(), on:click = move |_| {
                        if sch.get() {
                            if let Ok(out) = gsearch(if o_or_r.get() { osum.get() } else { rsum.get() }, !o_or_r.get()) {
                                display_output.update(move |disout| *disout = out);
                            }
                        }
                        sch.update(move |b| *b = !*b)
                    }) { "s" }
                    text(class = "o-or-r", class:hidden = move || sch.get(), on:click = move |_| o_or_r.update(move |b| *b = !*b)) { 
                        {move || {
                            if let Ok(out) = gsearch(if o_or_r.get() { osum.get() } else { rsum.get() }, !o_or_r.get()) {
                                display_output.update(move |disout| *disout = out);
                            }
                            if o_or_r.get() { "o" } else { "r" }
                        }}
                    }
                    text(class = "saturate", class:sat = move || sat.get(), on:click = move |_| sat.update(move |b| *b = !*b)) { 
                        "s"
                    }
                }
                column() {
                    column() {
                        text(class = "number") {{move || format!("{}", rsum.get())}}
                        text(class = "value") {{move || format!("reversed ")}}
                    }
                }
            }

            if move || esc_mode.get() {
                box(
                    class = "esc",
                    on:key_down = move |ev| match &ev.key {
                        Key::Named(NamedKey::Escape) => {
                            esc_mode.update(|n| *n = !*n);
                        }
                        _k => {}
                    }
                ) {
                    text() { "escape" }
                }
            }
        }
    }
}

const STYLE: &str = css!("
    .col { 
        position: fixed;
        bottom: 0;
        width: calc(99% - 2mm);
        gap: 12px; 
        align-items: center;
        justify-content: center;
        flex-flow: row wrap;
        color: #fff;
    }
    .sums { margin: 1mm; padding: 1mm; width: 80%; justify-content: space-evenly; }
    .value { text-align: center; font-size: 13px; margin-top: 1mm; margin-left: 1.5mm; }
    .number { text-align: center; font-size: 15px; margin: 0 1mm; }
    control { padding: 8px 16px; border-radius: 8px; background: #d9b176; }
    control:hover { background: #e30e55; }
    .inp {
        width: 95%;
        max-width: 16cm;
        padding: 1mm;
        border-bottom: 1px solid #fff;
        font-size: 20px;
    }
    .toggles > * {
        cursor: pointer;
        transition: all 97ms ease;
    }
    .o-or-r:hover {
        font-weight: 500;
        margin: 0 .25em;
    }
    .saturate, .sch {
        color: #888;
        margin: 0 .25em;
    }
    .sat, .ing {
        color: #fff;
    }
    .hidden {
        opacity: 0;
        pointer-events: none;
    }
    .spread {
        flex-flow: row wrap;
        width: calc(100% - 2mm);
        height: auto;
        padding: 1mm;
        text-align: center;
        justify-content: center;
        align-content: center;
        align-items: center;
    }
    .otxt {
      display: inline-block;
      text-align: left;
      margin: 2px 4px;
      padding: 2px 3px;
      background: #f9f3e2;
      box-shadow: 0 1px 3px rgba(0,0,0,0.5);
    }
    .esc {
        position: absolute;
        background: #ff278f;
        color: #fff;
        width: 90%;
        height: 90vh;
        bottom: 5vh;
        left: 5%;
        justify-content: center;
        text-align: center;
        align-items: center;
        align-content: center;
        box-shadow: 0 1px 3px rgba(0,0,0,0.5);
    }
    .term-menu {
        display: inline-flex;
        background: #ff278f;
        color: #fff;
        width: 100%;
        max-width: 256px;
        height: 2cm;
        justify-content: center;
        text-align: center;
        align-items: center;
        align-content: center;
        box-shadow: 0 1px 3px rgba(0,0,0,0.5);
    }
");

fn main() -> anyhow::Result<()> { 
    //println!("permsq(256) = {}", permsq(256, 256));

    let neum = app()
        .with_title("Neum") // .with_decorations(Decorations::NoTitleBar)
        .with_stylesheet(STYLE)
        .with_size(666.0, 444.0);

    neum.run(move || { view! { Gematria() } })?;
    Ok(())
}

fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

fn color_for_sum(sum: u64) -> [u8; 3] {
    hsl((sum as f64 * 137.507_764) % 360.0, 0.45, 0.45)
}

const ALLOCATED: &[u8] = b"allocated".as_slice();

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

/// the database: `./memory` as before, or the path in NEUM_MEMORY (tests use that)
static MEMORY: LazyLock<Database> = LazyLock::new(|| {
    let path = std::env::var("NEUM_MEMORY").unwrap_or_else(|_| "./memory".into());
    let mut db = Database::create(path).expect("opening/creating db had issues");
    init_state(&mut db).expect("state init failed");
    db
});

fn init_state(db: &mut Database) -> anyhow::Result<()> {
    let wx = db.begin_write()?;
    ensure_tables(&wx)?;
    wx.commit()?;
    if db.compact()? {
        println!("memory db compacted");
    }
    Ok(())
}

fn ensure_tables(wx: &WriteTransaction) -> anyhow::Result<()> {
    wx.open_table(STATE)?;
    wx.open_table(MEMORIES)?;
    for t in [MEMORY_INDEX, TO_REUSE, LISTS, LISTED_IN, NOTES, NOTED_ON] {
        wx.open_multimap_table(t)?;
    }
    wx.open_multimap_table(MEMORY_GEMATRIA)?;
    wx.open_multimap_table(MEMORY_GEMATRIA_REVERSED)?;
    Ok(())
}

fn parse_u8_list(s: &str) -> Result<Vec<u8>, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    trimmed
        .split(',')
        .map(|part| {
            let part_trimmed = part.trim();
            part_trimmed
                .parse::<u8>()
                .map_err(|e| format!("Invalid integer '{}': {}", part_trimmed, e))
        })
        .collect()
}

// ---- ids ----

/// ids are big-endian counters that grow a byte when they run out: [0]..[255], [0,0]..,
/// up to 4 bytes (the same sequence your `saturate` made)
fn next_id(prev: &[u8]) -> anyhow::Result<Vec<u8>> {
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
fn id_order(id: &[u8]) -> (usize, &[u8]) {
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

// ---- memories ----

/// with `None`, store a new memory under a fresh id; with `Some(id)`, put this
/// value at that id, moving its index entries from the old value (if there was one) to
/// the new. Returns the id.
pub fn saturate(quidity: &[u8], t: Option<Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    let wx = MEMORY.begin_write()?;
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

pub fn remember(this: &[u8]) -> anyhow::Result<Vec<u8>> {
    match MEMORY.begin_read()?.open_table(MEMORIES)?.get(this)? {
        Some(ag) => Ok(ag.value().to_vec()),
        None => Err(anyhow!("didn't find that")),
    }
}

/// FIX 2: forget a memory and every link to it. Notes are memories of their own, so the
/// notes on it stay; they are only no longer linked to it.
pub fn forget(this: &[u8]) -> anyhow::Result<Vec<u8>> {
    let wx = MEMORY.begin_write()?;
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
    wx.commit()?;
    Ok(old)
}

// ---- lists ----

/// put a memory into a list (it must exist, so lists never point at nothing)
pub fn add_to_list(ln: &[u8], v: &[u8]) -> anyhow::Result<()> {
    let wx = MEMORY.begin_write()?;
    if !exists(&wx, v)? {
        wx.abort()?;
        return Err(anyhow!("no memory with that id to list"));
    }
    wx.open_multimap_table(LISTS)?.insert(ln, v)?;
    wx.open_multimap_table(LISTED_IN)?.insert(v, ln)?;
    wx.commit()?;
    Ok(())
}

pub fn rm_from_list(ln: &[u8], v: &[u8]) -> anyhow::Result<()> {
    let wx = MEMORY.begin_write()?;
    wx.open_multimap_table(LISTS)?.remove(ln, v)?;
    wx.open_multimap_table(LISTED_IN)?.remove(v, ln)?;
    wx.commit()?;
    Ok(())
}

/// a list's memories by id (empty if the list is empty or unknown)
pub fn get_list(ln: &[u8]) -> anyhow::Result<HashMap<Vec<u8>, Vec<u8>>> {
    let rx = MEMORY.begin_read()?;
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
pub fn lists_of(v: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
    let rx = MEMORY.begin_read()?;
    let listed = rx.open_multimap_table(LISTED_IN)?;
    let mut out = vec![];
    for r in listed.get(v)? {
        out.push(r?.value().to_vec());
    }
    Ok(out)
}

// ---- notes ----

fn link_note(wx: &WriteTransaction, note: &[u8], on: &[u8]) -> anyhow::Result<()> {
    if !exists(wx, note)? || !exists(wx, on)? {
        return Err(anyhow!("a note and what it is on must both exist"));
    }
    wx.open_multimap_table(NOTES)?.insert(on, note)?;
    wx.open_multimap_table(NOTED_ON)?.insert(note, on)?;
    Ok(())
}

pub fn note_on(note: &[u8], on: &[u8]) -> anyhow::Result<()> {
    let wx = MEMORY.begin_write()?;
    link_note(&wx, note, on)?;
    wx.commit()?;
    Ok(())
}

/// many (on, note) pairs in one transaction (one commit, so one fsync: use this for imports)
pub fn notes_on<'a>(pairs: impl IntoIterator<Item = (&'a [u8], &'a [u8])>) -> anyhow::Result<()> {
    let wx = MEMORY.begin_write()?;
    for (on, note) in pairs {
        link_note(&wx, note, on)?;
    }
    wx.commit()?;
    Ok(())
}

pub fn rm_note(note: &[u8], on: &[u8]) -> anyhow::Result<()> {
    let wx = MEMORY.begin_write()?;
    wx.open_multimap_table(NOTES)?.remove(on, note)?;
    wx.open_multimap_table(NOTED_ON)?.remove(note, on)?;
    wx.commit()?;
    Ok(())
}

/// get notes on a memory, by key (empty if none)
pub fn get_notes_on(this: &[u8]) -> anyhow::Result<HashMap<Vec<u8>, Vec<u8>>> {
    let rx = MEMORY.begin_read()?;
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
pub fn noted_on(note: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
    let rx = MEMORY.begin_read()?;
    let noted = rx.open_multimap_table(NOTED_ON)?;
    let mut out = vec![];
    for r in noted.get(note)? {
        out.push(r?.value().to_vec());
    }
    Ok(out)
}

// ---- search ----

/// memories holding at least 3/4 of the query's distinct tokens. With
/// `also_saturate`, the query is stored as a memory too unless one already equals it;
/// its new id comes back second (and it is in the results).
pub fn search(query: &[u8], also_saturate: bool) -> anyhow::Result<(HashMap<Vec<u8>, Vec<u8>>, Option<Vec<u8>>)> {
    if query.len() == 0 { return Err(anyhow!("can't look for nothing")); }
    let tkns = unique_tokens(query);
    let mut out = HashMap::new();
    let mut that_one: Option<Vec<u8>> = None;
    if !tkns.is_empty() {
        let threshold = ((tkns.len() as f32 * 0.75).ceil() as usize).max(1);
        let rx = MEMORY.begin_read()?;
        let mi = rx.open_multimap_table(MEMORY_INDEX)?;
        let mem = rx.open_table(MEMORIES)?;
        let mut scores: HashMap<Vec<u8>, usize> = HashMap::new();
        for tkn in &tkns {
            for r in mi.get(*tkn)? {
                *scores.entry(r?.value().to_vec()).or_insert(0) += 1;
            }
        }
        for (k, hits) in scores {
            if hits >= threshold {
                if let Some(v) = mem.get(k.as_slice())? {
                    let val = v.value();
                    if val == query { that_one = Some(k.clone()); }
                    out.insert(k, val.to_vec());
                }
            }
        }
    }
    if that_one.is_none() {
        that_one = if also_saturate && out.par_iter().find_any(|v| v.1 == query).is_none() {
            let id = saturate(query, None)?;
            out.insert(id.clone(), query.to_vec());
            Some(id)
        } else { None };
    }
    Ok((out, that_one))
}

/// memories whose ordinal (or, with `rev`, reversed) sum is `sum`
pub fn gsearch(sum: u64, rev: bool) -> anyhow::Result<HashMap<Vec<u8>, Vec<u8>>> {
    let rx = MEMORY.begin_read()?;
    let mem = rx.open_table(MEMORIES)?;
    let memg = rx.open_multimap_table(if rev { MEMORY_GEMATRIA_REVERSED } else { MEMORY_GEMATRIA })?;
    let mut out = HashMap::new();
    for r in memg.get(sum)? {
        let k = r?;
        if let Some(v) = mem.get(k.value())? {
            out.insert(k.value().to_vec(), v.value().to_vec());
        }
    }
    Ok(out)
}

// ---- repair ----

/// rebuild what can be rebuilt and drop what points at nothing (see the module notes)
pub fn repair() -> anyhow::Result<()> {
    let wx = MEMORY.begin_write()?;
    repair_in(&wx)?;
    wx.commit()?;
    Ok(())
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
        for (on, note) in links.into_iter().filter(|(on, note)| live.contains(on) && live.contains(note)) {
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
    if let Some(top) = top {
        if last.as_deref().is_none_or(|l| id_order(&top) > id_order(l)) {
            state.insert(ALLOCATED, top.as_slice())?;
        }
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

// ---- backup ----

/// bytes as JSON: {"text": ..} when they are UTF-8, else {"hex": ..}
fn enc(b: &[u8]) -> Value {
    match std::str::from_utf8(b) {
        Ok(s) => json!({ "text": s }),
        Err(_) => json!({ "hex": b.iter().map(|x| format!("{x:02x}")).collect::<String>() }),
    }
}

fn dec(v: &Value) -> anyhow::Result<Vec<u8>> {
    if let Some(s) = v.get("text").and_then(Value::as_str) {
        return Ok(s.as_bytes().to_vec());
    }
    let h = v.get("hex").and_then(Value::as_str).ok_or_else(|| anyhow!("neither text nor hex: {v}"))?;
    if h.len() % 2 != 0 {
        return Err(anyhow!("odd hex length"));
    }
    (0..h.len()).step_by(2).map(|i| Ok(u8::from_str_radix(&h[i..i + 2], 16)?)).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> anyhow::Result<Vec<u8>> {
    dec(&json!({ "hex": s }))
}

/// everything needed to restore, as JSON at `path` (your old one was
/// "./it-all.json"). Returns how many memories were written.
pub fn backup_memories_to_file(path: &str) -> anyhow::Result<u64> {
    let rx = MEMORY.begin_read()?;
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
        "allocated": allocated,
        "reuse": reuse,
        "memories": memories,
        "lists": lists,
        "notes": notes,
    });
    std::fs::write(path, serde_json::to_string(&all)?)?;
    Ok(mem.len()?)
}

/// read a backup into an empty database (refuses to mix one into memories already there)
pub fn restore_from_file(path: &str) -> anyhow::Result<u64> {
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let wx = MEMORY.begin_write()?;
    if !wx.open_table(MEMORIES)?.is_empty()? {
        wx.abort()?;
        return Err(anyhow!("restore wants an empty database"));
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

/// AVX2 fast path from weum: 32 bytes per block, 4 blocks unrolled, PSADBW accumulation.
///
/// Unlike weum's kernel, the letter test is `(b | 0x20) - 'a' <= 25` rather than
/// `(b | 0x20) - ('a' - 1) <= 26`: with the latter, '@' and '`' pass as letters of value
/// 0 and add 27 to the reversed sum.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dual_sum_avx2(data: &[u8]) -> (u64, u64) {
    let lower = _mm256_set1_epi8(0x20);
    let a = _mm256_set1_epi8(b'a' as i8);
    let max = _mm256_set1_epi8(25);
    let one = _mm256_set1_epi8(1);
    let twentyseven = _mm256_set1_epi8(27);
    let zero = _mm256_setzero_si256();

    let mut s = [zero; 4];
    let mut r = [zero; 4];

    macro_rules! block {
        ($i:expr, $p:expr) => {{
            let v = unsafe { _mm256_loadu_si256($p as *const __m256i) };
            // 0 for 'a' .. 25 for 'z', anything else lands above 25
            let off = _mm256_sub_epi8(_mm256_or_si256(v, lower), a);
            let mask = _mm256_cmpeq_epi8(_mm256_min_epu8(off, max), off);
            let gval = _mm256_and_si256(_mm256_add_epi8(off, one), mask);
            let rval = _mm256_and_si256(_mm256_sub_epi8(twentyseven, gval), mask);
            s[$i] = _mm256_add_epi64(s[$i], _mm256_sad_epu8(gval, zero));
            r[$i] = _mm256_add_epi64(r[$i], _mm256_sad_epu8(rval, zero));
        }};
    }

    let len = data.len();
    let ptr = data.as_ptr();
    let mut i = 0;
    while i + 128 <= len {
        block!(0, ptr.add(i));
        block!(1, ptr.add(i + 32));
        block!(2, ptr.add(i + 64));
        block!(3, ptr.add(i + 96));
        i += 128;
    }
    while i + 32 <= len {
        block!(0, ptr.add(i));
        i += 32;
    }

    let reduce = |acc: [__m256i; 4]| -> u64 {
        let t = _mm256_add_epi64(_mm256_add_epi64(acc[0], acc[1]), _mm256_add_epi64(acc[2], acc[3]));
        let lanes = _mm_add_epi64(_mm256_castsi256_si128(t), _mm256_extracti128_si256(t, 1));
        (_mm_cvtsi128_si64(lanes) as u64).wrapping_add(_mm_extract_epi64(lanes, 1) as u64)
    };
    let (ordinal, reversed) = sums_scalar(&data[i..]);
    (reduce(s) + ordinal, reduce(r) + reversed)
}

pub fn sums_scalar(data: &[u8]) -> (u64, u64) {
    let (mut o, mut r) = (0u64, 0u64);
    for &b in data {
        let lc = b | 0x20;
        if lc.is_ascii_lowercase() {
            let v = (lc - b'a' + 1) as u64;
            o += v;
            r += 27 - v;
        }
    }
    (o, r)
}

pub fn sums(data: &[u8]) -> (u64, u64) {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        return unsafe { dual_sum_avx2(data) };
    }
    sums_scalar(data)
}

fn is_delimiter(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\t' | b'\r')
}

/// splits on spaces, newlines, tabs and carriage returns like weum's `tokenate`; tokens
/// are the raw bytes between delimiters, in order
pub fn tokenate(data: &[u8]) -> Vec<&[u8]> {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        return unsafe { tokenize_avx2(data) };
    }
    tokenize_scalar(data)
}

pub fn tokenize_scalar(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| is_delimiter(b)).filter(|t| !t.is_empty()).collect()
}

/// 32 bytes at a time: one delimiter bitmask per block, from which word starts and ends
/// fall out as bit transitions (carried across blocks)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn tokenize_avx2(data: &[u8]) -> Vec<&[u8]> {
    let space = _mm256_set1_epi8(b' ' as i8);
    let newline = _mm256_set1_epi8(b'\n' as i8);
    let tab = _mm256_set1_epi8(b'\t' as i8);
    let cr = _mm256_set1_epi8(b'\r' as i8);

    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut in_word = false;
    let len = data.len();
    let mut i = 0;
    while i + 32 <= len {
        let v = unsafe { _mm256_loadu_si256(data.as_ptr().add(i) as *const __m256i) };
        let delim = _mm256_or_si256(
            _mm256_or_si256(_mm256_cmpeq_epi8(v, space), _mm256_cmpeq_epi8(v, newline)),
            _mm256_or_si256(_mm256_cmpeq_epi8(v, tab), _mm256_cmpeq_epi8(v, cr)),
        );
        let word = !(_mm256_movemask_epi8(delim) as u32);
        // a bit per byte saying whether the byte before it was part of a word
        let before = (word << 1) | in_word as u32;
        let mut edges = (word & !before) | (!word & before);
        while edges != 0 {
            let bit = edges.trailing_zeros() as usize;
            if word >> bit & 1 == 1 {
                start = i + bit;
            } else {
                tokens.push(&data[start..i + bit]);
            }
            edges &= edges - 1;
        }
        in_word = word >> 31 == 1;
        i += 32;
    }
    for (j, &b) in data.iter().enumerate().skip(i) {
        match (is_delimiter(b), in_word) {
            (true, true) => {
                tokens.push(&data[start..j]);
                in_word = false;
            }
            (false, false) => {
                start = j;
                in_word = true;
            }
            _ => {}
        }
    }
    if in_word {
        tokens.push(&data[start..]);
    }
    tokens
}

/// each distinct token once (sorted by bytes). FIX: `dedup` alone only removes repeats
/// that sit next to each other, so "a b a" kept both a's
pub fn unique_tokens(data: &[u8]) -> Vec<&[u8]> {
    let mut tkns = tokenate(data);
    tkns.sort_unstable();
    tkns.dedup();
    tkns
}

#[allow(dead_code)]
fn permsq(a: u64, b: u64) -> String {
    let mut digits = vec![1u8];
    for _ in 0..b {
        let mut carry = 0u64;
        for d in digits.iter_mut().rev() {
            let prod = (*d as u64) * a + carry;
            *d = (prod % 10) as u8;
            carry = prod / 10;
        }
        while carry > 0 {
            digits.insert(0, (carry % 10) as u8);
            carry /= 10;
        }
    }
    digits.into_iter().map(|d| (d + b'0') as char).collect()
}

/// the byte sums of some data
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteSums {
    /// Σ (b + 1)
    pub bytes: u64,
    /// Σ (256 − b)
    pub reversed: u64,
    /// Σ digit_sum(b + 1)
    pub digits: u64,
    /// how many bytes were summed
    pub len: u64,
}

impl ByteSums {
    /// the digit roots of `bytes`, `reversed` and `digits` (0 for 0, else 1 + (n − 1) mod 9)
    pub fn roots(&self) -> [u64; 3] {
        [digit_root(self.bytes), digit_root(self.reversed), digit_root(self.digits)]
    }

    /// the mean byte value, 1 ..= 256 (0 for no bytes)
    pub fn mean(&self) -> f64 {
        if self.len == 0 { 0.0 } else { self.bytes as f64 / self.len as f64 }
    }
}

/// the value of one byte: 1 ..= 256
pub fn value(b: u8) -> u64 {
    b as u64 + 1
}

/// the sum of n's decimal digits
pub fn digit_sum(mut n: u64) -> u64 {
    let mut s = 0;
    while n > 0 {
        s += n % 10;
        n /= 10;
    }
    s
}

/// 0 for 0, else 1 + (n − 1) mod 9
pub fn digit_root(n: u64) -> u64 {
    if n == 0 { 0 } else { 1 + (n - 1) % 9 }
}

const DIGITS: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut n = i + 1;
        let mut s = 0;
        while n > 0 {
            s += n % 10;
            n /= 10;
        }
        t[i] = s as u8;
        i += 1;
    }
    t
};

/// every byte sum of `data`
pub fn byte_sums(data: &[u8]) -> ByteSums {
    let (mut bytes, mut digits) = (0u64, 0u64);
    for &b in data {
        bytes += b as u64;
        digits += DIGITS[b as usize] as u64;
    }
    let len = data.len() as u64;
    bytes += len;
    ByteSums {bytes, reversed: 257 * len - bytes, digits, len}
}

/// Σ (b + 1) alone
pub fn byte_sum(data: &[u8]) -> u64 {
    data.iter().map(|&b| b as u64).sum::<u64>() + data.len() as u64
}

pub fn prime_factors(mut n: u64) -> Vec<u64> {
    let mut found = Vec::new();
    let mut d = 2;
    while n > 1 {
        while n.is_multiple_of(d) {
            found.push(d);
            n /= d;
        }
        d += 1;
        if d * d > n && n > 1 {
            found.push(n);
            break;
        }
    }
    found
}

fn pf_sum(n: u64) -> u64 {
    prime_factors(n).iter().sum()
}

pub fn pf_reduction(s: u64) -> (Vec<u64>, u64, u64) {
    const LIMIT: usize = 32;
    let pf = prime_factors(s);
    let pfr: u64 = pf.iter().sum();
    let ds = digit_sum(s);
    let dp = ds * pfr;
    let mut chain: Vec<u64> = Vec::new();
    for _ in 0..=LIMIT {
        let xn = match chain.last() {
            Some(&last) => pf_sum(last) * digit_sum(last),
            None => pf_sum(dp) * ds,
        };
        if chain.contains(&xn) { break; }
        chain.push(xn);
    }
    let total = chain.iter().sum::<u64>() + pfr;
    (chain, total, digit_sum(total))
}

#[allow(dead_code)]
fn vowel(c: u8) -> u64 {
    match c {
        b'a' => 1, b'e' => 2,
        b'o' => 3, b'u' => 4,
        b'i' => 5, b'y' => 6,
        _ => 0,
    }
}

#[allow(dead_code)]
fn consonant(c: u8) -> u64 {
    match c {
        b'b' => 1, b'c' => 2, b'd' => 3, b'f' => 4,
        b'g' => 5, b'h' => 6, b'j' => 7, b'k' => 8,
        b'l' => 9, b'm' => 10, b'n' => 11, b'p' => 12,
        b'q' => 13, b'r' => 14, b's' => 15, b't' => 16,
        b'v' => 17, b'w' => 18, b'x' => 19, b'z' => 20,
        _ => 0,
    }
}
