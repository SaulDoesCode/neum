use zgui::{prelude::*, reactive::LocalStorage};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::{ColorScheme, Theme, ThemeProviderProps};

use std::{collections::HashMap, path::Path};

use neum::{
    Found, Mode, Settings, Stats,
    gematria::{ByteSums, byte_list, byte_story, byte_sums, color_for_sum, input_bytes, is_byte_list, parse_u8_list, sums},
    human_bytes,
    settings::startup_memory,
    store::{self, hex, id_order},
};

type Local<T> = RwSignal<T, LocalStorage>;

/// every signal the window runs on, in one Copy bundle so handlers can share the actions below
#[derive(Clone, Copy)]
struct Ui {
    txt: Local<String>,
    alt_name: Local<String>,
    osum: RwSignal<u64>,
    rsum: RwSignal<u64>,
    bsum: RwSignal<ByteSums>,
    sch: RwSignal<bool>,
    mode: RwSignal<Mode>,
    sat: RwSignal<bool>,
    display_output: RwSignal<Found>,
    esc_mode: RwSignal<bool>,
    term_ctx_menu: RwSignal<bool>,
    /// what the last thing done had to say
    status: Local<String>,
    settings: Local<Settings>,
    stats: Local<Option<Stats>>,
    /// the path typed into the escape menu
    path_in: Local<String>,
}

impl Ui {
    fn say(self, msg: impl Into<String>) {
        let msg = msg.into();
        eprintln!("{msg}");
        self.status.set(msg);
    }

    fn show(self, found: Found) {
        self.display_output.set(found);
    }

    /// change the settings and keep them
    fn set(self, f: impl FnOnce(&mut Settings)) {
        self.settings.update(f);
        if let Err(e) = self.settings.get_untracked().save() {
            self.say(format!("couldn't save settings: {e}"));
        }
    }

    /// the sums of what's in the bar
    fn compute(self) {
        let t = self.txt.get_untracked();
        let (o, r) = sums(t.as_bytes());
        if self.osum.get_untracked() != o { self.osum.set(o); }
        if self.rsum.get_untracked() != r { self.rsum.set(r); }
        let b = byte_sums(&input_bytes(&t));
        if self.bsum.get_untracked() != b { self.bsum.set(b); }
    }

    /// the two numbers either side of the toggles
    fn shown_sums(self) -> (u64, u64) {
        match self.mode.get() {
            Mode::Bytes => {
                let b = self.bsum.get();
                (b.bytes, b.reversed)
            }
            _ => (self.osum.get(), self.rsum.get()),
        }
    }

    fn shown_sums_untracked(self) -> (u64, u64) {
        match self.mode.get_untracked() {
            Mode::Bytes => {
                let b = self.bsum.get_untracked();
                (b.bytes, b.reversed)
            }
            _ => (self.osum.get_untracked(), self.rsum.get_untracked()),
        }
    }

    /// the sum the middle letter is on
    fn active_sum(self) -> u64 {
        match self.mode.get_untracked() {
            Mode::Ordinal => self.osum.get_untracked(),
            Mode::Reversed => self.rsum.get_untracked(),
            Mode::Bytes => self.bsum.get_untracked().bytes,
        }
    }

    fn regsearch(self) {
        if !store::is_open() { return; }
        match store::gsearch(self.active_sum(), self.mode.get_untracked()) {
            Ok(out) => self.show(out),
            Err(e) => self.say(format!("search error: {e}")),
        }
    }

    /// the line under the bar: what was last said, else where Enter will put things
    fn hint(self) -> String {
        let s = self.status.get();
        if !s.is_empty() { return s; }
        let alt = self.alt_name.get();
        if alt.is_empty() {
            String::new()
        } else if self.sch.get() {
            format!("→ list «{alt}»")
        } else {
            format!("→ notes on «{alt}»")
        }
    }

    fn threshold(self) -> f32 {
        self.settings.get_untracked().threshold_fraction()
    }

    /// the memory the alt name is, if there is one (made, with `make`)
    fn alt_memory(self, make: bool) -> Option<Vec<u8>> {
        let alt = self.alt_name.get_untracked();
        if alt.is_empty() { return None; }
        match store::search(alt.as_bytes(), make) {
            Ok((_, Some(l))) => Some(l),
            Ok((_, None)) => {
                self.say(format!("look no find.. no memory is «{alt}»"));
                None
            }
            Err(e) => {
                self.say(format!("search error: {e}"));
                None
            }
        }
    }

    fn enter(self, ctrl: bool) {
        let t = self.txt.get_untracked();
        let alt = self.alt_name.get_untracked();
        if t.is_empty() {
            // an empty bar shows what's under the alt name: its list on the search channel,
            // its notes on the gematria channel
            if alt.is_empty() { return; }
            let Some(l) = self.alt_memory(false) else {
                self.show(HashMap::new());
                return;
            };
            if self.sch.get_untracked() {
                match store::get_list(&l) {
                    Ok(lst) => {
                        self.say(format!("list «{alt}»: {} in it", lst.len()));
                        self.show(lst);
                    }
                    Err(e) => self.say(format!("trouble getting the list: {e}")),
                }
            } else {
                match store::get_notes_on(&l) {
                    Ok(notes) => {
                        self.say(format!("notes on «{alt}»: {}", notes.len()));
                        self.show(notes);
                    }
                    Err(e) => self.say(format!("trouble getting the notes: {e}")),
                }
            }
            return;
        }

        if self.sch.get_untracked() {
            // search channel: words, and lists
            let (out, key) = match store::search_with(t.as_bytes(), self.sat.get_untracked(), self.threshold()) {
                Ok(r) => r,
                Err(e) => return self.say(format!("search error: {e}")),
            };
            self.say(format!("{} found", out.len()));
            self.show(out);
            if alt.is_empty() { return; }
            let Some(k) = key else {
                return self.say(format!("«{t}» isn't a memory, so it can't be listed (turn saturate on)"));
            };
            if let Some(l) = self.alt_memory(true) {
                match store::add_to_list(&l, &k) {
                    Ok(()) => self.say(format!("listed in «{alt}»")),
                    Err(e) => self.say(format!("trouble adding thing to list: {e}")),
                }
            }
        } else {
            // gematria channel: sums, and notes
            let mode = self.mode.get_untracked();
            let number = t.trim().parse::<u64>().ok().filter(|_| !ctrl);
            let written_bytes = mode == Mode::Bytes && is_byte_list(&t);
            let sum = number.unwrap_or_else(|| self.active_sum());
            let mut out = match store::gsearch(sum, mode) {
                Ok(out) => out,
                Err(e) => return self.say(format!("search error: {e}")),
            };
            // a number is a sum to look up and written bytes are only looked up; anything
            // else is text, which can be saturated and noted
            if number.is_none() && !written_bytes {
                let tb = t.as_bytes().to_vec();
                let mut key = out.iter().find(|(_, v)| **v == tb).map(|(k, _)| k.clone());
                if key.is_none() && self.sat.get_untracked() {
                    match store::saturate(&tb, None) {
                        Ok(k) => {
                            out.insert(k.clone(), tb);
                            key = Some(k);
                        }
                        Err(e) => self.say(format!("trouble saturating: {e}")),
                    }
                }
                if !alt.is_empty() {
                    match (key, self.alt_memory(true)) {
                        (Some(k), Some(l)) => match store::note_on(&k, &l) {
                            Ok(()) => self.say(format!("noted on «{alt}»")),
                            Err(e) => self.say(format!("trouble noting on thing: {e}")),
                        },
                        (None, _) => self.say(format!("«{t}» isn't a memory, so it can't be a note (turn saturate on)")),
                        _ => {}
                    }
                } else {
                    self.status.set(format!("{} at {} {sum}", out.len(), mode.name()));
                }
            } else {
                self.status.set(format!("{} at {} {sum}", out.len(), mode.name()));
            }
            self.show(out);
        }
    }

    fn delete(self) {
        let t = self.txt.get_untracked();
        let Ok(key) = parse_u8_list(&t) else { return };
        if key.is_empty() { return; }
        if !self.alt_name.get_untracked().is_empty() {
            // out of the alt name's list (search channel) or off its notes (gematria channel)
            let Some(l) = self.alt_memory(false) else { return };
            let r = if self.sch.get_untracked() { store::rm_from_list(&l, &key) } else { store::rm_note(&key, &l) };
            match r {
                Ok(()) => {
                    self.display_output.update(|d| { d.remove(&key); });
                    self.say(if self.sch.get_untracked() { "taken off the list" } else { "no longer a note on it" });
                }
                Err(e) => self.say(format!("trouble unlinking: {e}")),
            }
        } else {
            match store::forget(&key) {
                Ok(was) => {
                    self.display_output.update(|d| { d.remove(&key); });
                    self.say(format!("forgot [{}]", byte_list(&key)));
                    if let Ok(stwing) = String::from_utf8(was) {
                        self.txt.set(stwing);
                        self.compute();
                    }
                }
                Err(e) => self.say(format!("{e}")),
            }
        }
    }

    // ---- the escape menu ----

    fn refresh(self) {
        self.stats.set(store::stats().ok());
    }

    fn toggle_esc(self) {
        let open = !self.esc_mode.get_untracked();
        if open {
            self.refresh();
            if self.path_in.get_untracked().is_empty()
                && let Some(p) = store::current_path()
            {
                self.path_in.set(p.display().to_string());
            }
        }
        self.esc_mode.set(open);
    }

    fn open_memory(self, path: String) {
        let path = path.trim().to_string();
        if path.is_empty() {
            return self.say("type the path of a memory to open (a new path makes a new one)");
        }
        match store::open(&path, self.settings.get_untracked().compact_on_open) {
            Ok(m) => {
                let p = m.path().display().to_string();
                let p = canonical(&p);
                self.set(|s| s.opened(&p));
                self.path_in.set(p.clone());
                self.show(HashMap::new());
                self.refresh();
                self.say(format!("opened {p}"));
            }
            Err(e) => self.say(format!("{e}")),
        }
    }

    fn close_memory(self) {
        match store::close() {
            Some(p) => {
                self.show(HashMap::new());
                self.refresh();
                self.say(format!("closed {}", p.display()));
            }
            None => self.say("no memory is open"),
        }
    }

    /// to the path typed if it is a .json, else next to the memory with the time in its name
    fn export(self) {
        let typed = self.path_in.get_untracked().trim().to_string();
        let to = if typed.ends_with(".json") {
            typed
        } else if let Some(p) = store::current_path() {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{}.{secs}.json", p.display())
        } else {
            return self.say("no memory open to export");
        };
        match store::backup_memories_to_file(&to) {
            Ok(n) => self.say(format!("exported {n} memories to {to}")),
            Err(e) => self.say(format!("export failed: {e}")),
        }
    }

    fn import(self) {
        let from = self.path_in.get_untracked().trim().to_string();
        if !from.ends_with(".json") {
            return self.say("type the path of a .json export to import it");
        }
        match store::restore_from_file(&from) {
            Ok(n) => {
                self.refresh();
                self.say(format!("imported {n} memories from {from}"));
            }
            Err(e) => self.say(format!("import failed: {e}")),
        }
    }

    fn repair(self) {
        match store::repair() {
            Ok(()) => {
                self.refresh();
                self.say("repaired: indexes rebuilt, dangling links dropped");
            }
            Err(e) => self.say(format!("repair failed: {e}")),
        }
    }
}

fn canonical(p: &str) -> String {
    std::fs::canonicalize(p).map(|c| c.display().to_string()).unwrap_or_else(|_| p.to_string())
}

/// the file name of a save, for the list
fn save_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

fn stat_rows(s: &Stats) -> Vec<(&'static str, String)> {
    vec![
        ("file", s.path.display().to_string()),
        ("on disk", human_bytes(s.file_bytes)),
        ("memories", format!("{} · {}", s.memories, human_bytes(s.stored_bytes))),
        ("lists", format!("{} holding {}", s.lists, s.listed)),
        ("notes", format!("{} on {} memories", s.notes, s.noted)),
        ("last id", s.allocated.as_deref().map(|a| format!("[{}] 0x{}", byte_list(a), hex(a))).unwrap_or_else(|| "none yet".into())),
        ("ids to reuse", s.reusable.to_string()),
        ("schema", format!("v{}", s.schema)),
    ]
}

const KEYS: [(&str, &str); 11] = [
    ("enter", "sum, look up, saturate; numbers look up that sum (ctrl+enter sums them as text)"),
    ("enter, alt name set", "s on: add to the alt name's list · s off: note it on the alt name"),
    ("enter, empty bar", "s on: show the alt name's list · s off: show its notes"),
    ("delete on a key", "forget it, or unlink it from the alt name's list or notes"),
    ("middle click bar", "move the text to the alt name"),
    ("right click bar", "show the alt name"),
    ("click / double click", "an entry's text / its key into the bar"),
    ("ctrl + right click", "an entry: look up the mean of the two sums"),
    ("s · o/r/b · s", "search channel · which sum (click to cycle) · saturate"),
    ("b", "the byte sum bucket: Σ(byte+1); 104,105 is read as bytes"),
    ("esc", "this menu"),
];

#[component]
fn Gematria(settings: Settings, greeting: String) -> impl IntoView {
    let ui = Ui {
        txt: RwSignal::new_local(String::new()),
        alt_name: RwSignal::new_local(String::new()),
        osum: RwSignal::new(0_u64),
        rsum: RwSignal::new(0_u64),
        bsum: RwSignal::new(ByteSums::default()),
        sch: RwSignal::new(false),
        mode: RwSignal::new(settings.mode),
        sat: RwSignal::new(settings.saturate),
        display_output: RwSignal::new(HashMap::new()),
        esc_mode: RwSignal::new(false),
        term_ctx_menu: RwSignal::new(false),
        status: RwSignal::new_local(greeting),
        settings: RwSignal::new_local(settings),
        stats: RwSignal::new_local(None),
        path_in: RwSignal::new_local(String::new()),
    };
    let Ui { txt, alt_name, sch, mode, sat, display_output, esc_mode, term_ctx_menu, status, settings, stats, path_in, .. } = ui;

    let light = move || settings.get().light;
    let scheme = Signal::derive_local(move || if light() { ColorScheme::Light } else { ColorScheme::Dark });
    let sat_at_start = Binding::controlled(
        Signal::derive_local(move || settings.get().saturate),
        move |on: bool| ui.set(|s| s.saturate = on),
    );
    let compact_on_open = Binding::controlled(
        Signal::derive_local(move || settings.get().compact_on_open),
        move |on: bool| ui.set(|s| s.compact_on_open = on),
    );

    // the window's own background and text, so the page behind everything follows the theme
    let light_theme = Theme::light().with_css("--zui-color-background: #f4efe3; --zui-color-foreground: #1d1a16;");
    let dark_theme = Theme::dark().with_css("--zui-color-background: #000; --zui-color-foreground: #fff;");

    view! {
        ThemeProvider(scheme = scheme, light = light_theme, dark = dark_theme) {
            column(class = "app", class:light = light) {
                row(class = "spread",
                    on:click = {move |_ev| {
                        esc_mode.update(|n| *n = false);
                    }}
                ) {{move || { //  'ordinary cunt' sums to 162/162
                    let mut found: Vec<(Vec<u8>, Vec<u8>)> = display_output.get().into_iter().collect();
                    found.sort_by(|a, b| id_order(&a.0).cmp(&id_order(&b.0)));
                    let mode_now = mode.get();
                    let mut els = vec![];
                    for (k, v) in found {
                        let key = byte_list(&k);
                        let (o, r) = sums(&v);
                        // everything shown shares the searched sum, so colour by another one
                        let culr = color_for_sum(if mode_now == Mode::Ordinal { r } else { o });
                        let unrlnclr = format!("{:?},{:?},{:?}", culr[0], culr[1], culr[2]);
                        let (shown, is_text) = match String::from_utf8(v) {
                            Ok(s) => (s, true),
                            Err(e) => (format!("⟨{}⟩", byte_list(e.as_bytes())), false),
                        };
                        let ov = if is_text { shown.clone() } else { key.clone() };

                        els.push(view!{
                            box(
                                class = "otxt",
                                class:bytes = !is_text,
                                style = format!("border-bottom: 3px solid rgb({});", unrlnclr),
                                on:double_click = move |_| {
                                    let key = key.clone();
                                    txt.update(move |t| *t = key);
                                    ui.compute();
                                },
                                on:pointer_down = move |ev| {
                                    if let Some(btn) = ev.button
                                        && btn == PointerButton::Secondary
                                        && ev.modifiers.control()
                                    {
                                        let (a, b) = ui.shown_sums_untracked();
                                        let n = (a as f64 * b as f64).sqrt().round() as u64;
                                        txt.update(move |t| *t = n.to_string());
                                        ui.compute();
                                        match store::gsearch(n, mode.get_untracked()) {
                                            Ok(out) => display_output.update(move |disout| *disout = out),
                                            Err(e) => ui.say(format!("search error: {e}")),
                                        }
                                    }
                                },
                                on:click = move |_ev| {
                                    let ov = ov.to_string();
                                    txt.update(move |t| *t = ov.to_string());
                                    ui.compute();
                                }
                            ) { text() {{shown}} }
                        });
                    }
                    els
                }}}
                column(class = "col") {
                    if move || term_ctx_menu.get() {
                        row(class = "term-menu") {
                            Input(
                                value = alt_name,
                                label = "alt name: a list (s on) or what notes are on (s off)",
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
                                    ui.compute();
                                }
                            }
                        },
                    ) {
                        Input(
                            value = txt,
                            label = "type text to sum with gematria",
                            on:input = move |_| {
                                ui.compute();
                                if !status.get_untracked().is_empty() { status.set(String::new()); }
                            },
                            on:click = move |_ev| {
                                esc_mode.update(|n| *n = false);
                            },
                            on:key_down = move |ev| match &ev.key {
                                Key::Named(NamedKey::Enter) => ui.enter(ev.modifiers.control()),
                                Key::Named(NamedKey::Delete) => ui.delete(),
                                Key::Named(NamedKey::Escape) => ui.toggle_esc(),
                                _k => {}
                            }
                        )
                    }
                    if move || !ui.hint().is_empty() {
                        row(class = "hint") {
                            text() {{move || ui.hint()}}
                        }
                    }
                    if move || mode.get() == Mode::Bytes {
                        column(class = "story") {
                            {move || {
                                let t = txt.get();
                                byte_story(&input_bytes(&t))
                                    .lines()
                                    .into_iter()
                                    .map(|l| view! { text(class = "story-line") {{l}} })
                                    .collect::<Vec<_>>()
                            }}
                        }
                    }
                    row(
                        class = "sums",
                        on:click = {move |_ev| {
                            esc_mode.update(|n| *n = false);
                        }},
                    ) {
                        column() {
                            column() {
                                text(class = "number") {{move || format!("{}", ui.shown_sums().0)}}
                                text(class = "value") {{move || if mode.get() == Mode::Bytes { "bytes " } else { "ordinal " }}}
                            }
                        }
                        row(class = "toggles") {
                            text(class="sch", class:ing = move || sch.get(), on:click = move |_| {
                                if sch.get() {
                                    ui.regsearch();
                                }
                                sch.update(move |b| *b = !*b)
                            }) { "s" }
                            text(class = "o-or-r", class:hidden = move || sch.get(), on:click = move |_| {
                                let next = mode.get_untracked().next();
                                mode.set(next);
                                ui.set(|s| s.mode = next);
                                ui.regsearch();
                            }) {
                                {move || mode.get().letter()}
                            }
                            text(class = "saturate", class:sat = move || sat.get(), on:click = move |_| sat.update(move |b| *b = !*b)) {
                                "s"
                            }
                        }
                        column() {
                            column() {
                                text(class = "number") {{move || format!("{}", ui.shown_sums().1)}}
                                text(class = "value") {{move || if mode.get() == Mode::Bytes { "reversed bytes " } else { "reversed " }}}
                            }
                        }
                    }

                    if move || esc_mode.get() {
                        box(
                            class = "esc",
                            on:key_down = move |ev| match &ev.key {
                                Key::Named(NamedKey::Escape) => ui.toggle_esc(),
                                _k => {}
                            }
                        ) {
                            scroll(class = "esc-scroll") {
                                column(class = "esc-inner") {
                                    row(class = "esc-head") {
                                        column() {
                                            text(class = "esc-title") { "neum" }
                                            text(class = "esc-sub") {{format!("v{} · esc to go back", env!("CARGO_PKG_VERSION"))}}
                                        }
                                        spacer()
                                        Button(
                                            variant = ButtonVariant::Outline,
                                            on:click = move |_| ui.set(|s| s.light = !s.light)
                                        ) {
                                            {move || if light() { "☾ dark mode" } else { "☀ light mode" }}
                                        }
                                    }
                                    row(class = "esc-cards") {
                                        column(class = "esc-card") {
                                            text(class = "esc-h") { "memory" }
                                            {move || match stats.get() {
                                                Some(s) => stat_rows(&s)
                                                    .into_iter()
                                                    .map(|(k, v)| view! {
                                                        row(class = "kv") {
                                                            text(class = "k") {{k}}
                                                            text(class = "v") {{v}}
                                                        }
                                                    })
                                                    .collect::<Vec<_>>(),
                                                None => vec![view! {
                                                    row(class = "kv") {
                                                        text(class = "k") {"closed"}
                                                        text(class = "v") {"no memory open: open one under saves"}
                                                    }
                                                }],
                                            }}
                                            row(class = "esc-buttons") {
                                                Button(variant = ButtonVariant::Secondary, size = ButtonSize::Sm, on:click = move |_| ui.refresh()) { "refresh" }
                                                Button(variant = ButtonVariant::Secondary, size = ButtonSize::Sm, on:click = move |_| ui.repair()) { "repair" }
                                            }
                                        }
                                        column(class = "esc-card") {
                                            text(class = "esc-h") { "saves" }
                                            Input(value = path_in, label = "path: a memory to open or make, or a .json")
                                            row(class = "esc-buttons") {
                                                Button(size = ButtonSize::Sm, on:click = move |_| ui.open_memory(path_in.get_untracked())) { "open" }
                                                Button(variant = ButtonVariant::Outline, size = ButtonSize::Sm, on:click = move |_| ui.close_memory()) { "close" }
                                                Button(variant = ButtonVariant::Secondary, size = ButtonSize::Sm, on:click = move |_| ui.export()) { "export" }
                                                Button(variant = ButtonVariant::Secondary, size = ButtonSize::Sm, on:click = move |_| ui.import()) { "import" }
                                            }
                                            {move || {
                                                let open_now = stats.get().map(|s| canonical(&s.path.display().to_string()));
                                                settings.get().recent.into_iter().map(|p| {
                                                    let on = open_now.as_deref() == Some(p.as_str());
                                                    let (to_open, to_drop) = (p.clone(), p.clone());
                                                    view! {
                                                        row(class = "save", class:on = on) {
                                                            column(
                                                                class = "save-name",
                                                                on:click = move |_| ui.open_memory(to_open.clone())
                                                            ) {
                                                                text(class = "save-title") {{save_name(&p)}}
                                                                text(class = "save-path") {{p.clone()}}
                                                            }
                                                            spacer()
                                                            text(
                                                                class = "save-drop",
                                                                on:click = move |_| {
                                                                    let p = to_drop.clone();
                                                                    ui.set(move |s| s.forget_save(&p));
                                                                }
                                                            ) {"×"}
                                                        }
                                                    }
                                                }).collect::<Vec<_>>()
                                            }}
                                        }
                                        column(class = "esc-card") {
                                            text(class = "esc-h") { "settings" }
                                            row(class = "setting") {
                                                text() {"saturate on at start"}
                                                spacer()
                                                Switch(checked = sat_at_start)
                                            }
                                            row(class = "setting") {
                                                text() {"compact a memory when opening it"}
                                                spacer()
                                                Switch(checked = compact_on_open)
                                            }
                                            row(class = "setting") {
                                                text() {"words a search match must share"}
                                                spacer()
                                                Button(
                                                    variant = ButtonVariant::Outline,
                                                    size = ButtonSize::Sm,
                                                    on:click = move |_| ui.set(|s| s.threshold = s.next_threshold())
                                                ) {
                                                    {move || format!("{}%", settings.get().threshold)}
                                                }
                                            }
                                            text(class = "esc-note") {{format!("kept in {}", Settings::path().display())}}
                                        }
                                        column(class = "esc-card") {
                                            text(class = "esc-h") { "keys" }
                                            {KEYS.iter().map(|(k, v)| view! {
                                                row(class = "kv") {
                                                    text(class = "k") {{*k}}
                                                    text(class = "v") {{*v}}
                                                }
                                            }).collect::<Vec<_>>()}
                                        }
                                    }
                                    text(class = "esc-status") {{move || status.get()}}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

const STYLE: &str = css!("
    :root {
        background-color: var(--zui-color-background, #000);
        color: var(--zui-color-foreground, #fff);
    }
    .app {
        --fg: #fff;
        --muted: #888;
        --card: #f9f3e2;
        --card-fg: #111;
        --line: #fff;
        --panel: #16121a;
        --panel-fg: #f4eef8;
        --panel-muted: #a89cb2;
        --accent: #ff278f;
        --shadow: rgba(0,0,0,0.5);
        display: contents;
        color: var(--fg);
    }
    .app.light {
        --fg: #1d1a16;
        --muted: #9a907f;
        --card: #fffdf8;
        --card-fg: #1d1a16;
        --line: #1d1a16;
        --panel: #fffaf2;
        --panel-fg: #1d1a16;
        --panel-muted: #7d7365;
        --accent: #d4136f;
        --shadow: rgba(60,40,20,0.18);
    }
    .col {
        position: fixed;
        bottom: 0;
        width: calc(99% - 2mm);
        gap: 12px;
        align-items: center;
        justify-content: center;
        flex-flow: row wrap;
        color: var(--fg);
    }
    .sums { margin: 1mm; padding: 1mm; width: 80%; justify-content: space-evenly; }
    .value { text-align: center; font-size: 13px; margin-top: 1mm; margin-left: 1.5mm; color: var(--fg); }
    .number { text-align: center; font-size: 15px; margin: 0 1mm; color: var(--fg); }
    .inp {
        width: 95%;
        max-width: 16cm;
        padding: 1mm;
        border-bottom: 1px solid var(--line);
        font-size: 20px;
    }
    .inp .zui-input, .term-menu .zui-input,
    .inp .zui-input:focus-visible, .term-menu .zui-input:focus-visible {
        border: none;
        border-radius: 0;
        box-shadow: none;
        background-color: transparent;
        color: inherit;
        font-family: inherit;
        font-size: inherit;
        line-height: inherit;
    }
    .hint {
        width: 95%;
        max-width: 16cm;
        justify-content: center;
        font-size: 12px;
        color: var(--muted);
    }
    .story {
        width: 95%;
        max-width: 16cm;
        align-items: center;
        gap: 2px;
        font-size: 12px;
        color: var(--muted);
    }
    .story-line { text-align: center; }
    .toggles > * {
        cursor: pointer;
        transition: all 97ms ease;
    }
    .o-or-r { color: var(--fg); }
    .o-or-r:hover {
        font-weight: 500;
        margin: 0 .25em;
    }
    .saturate, .sch {
        color: var(--muted);
        margin: 0 .25em;
    }
    .sat, .ing {
        color: var(--fg);
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
      background: var(--card);
      color: var(--card-fg);
      box-shadow: 0 1px 3px var(--shadow);
      cursor: pointer;
    }
    .otxt.bytes { font-family: monospace; font-size: 12px; }
    .esc {
        position: absolute;
        background: var(--panel);
        color: var(--panel-fg);
        width: 90%;
        height: 90vh;
        bottom: 5vh;
        left: 5%;
        border-top: 3px solid var(--accent);
        border-radius: 6px;
        box-shadow: 0 4px 18px var(--shadow);
    }
    .esc-scroll { width: 100%; height: 100%; overflow-y: auto; }
    .esc-inner { width: 100%; padding: 10px 14px; gap: 10px; }
    .esc-head { align-items: center; gap: 8px; }
    .esc-title { font-size: 20px; font-weight: 700; color: var(--accent); }
    .esc-sub { font-size: 11px; color: var(--panel-muted); }
    .esc-cards { flex-flow: row wrap; gap: 10px; align-items: flex-start; }
    .esc-card {
        flex: 1 1 240px;
        min-width: 220px;
        gap: 6px;
        padding: 8px 10px;
        border: 1px solid var(--panel-muted);
        border-radius: 6px;
    }
    .esc-h {
        font-size: 12px;
        font-weight: 700;
        letter-spacing: 1px;
        text-transform: uppercase;
        color: var(--accent);
    }
    .kv { gap: 8px; font-size: 12px; align-items: flex-start; }
    .k { flex: 0 0 auto; min-width: 84px; color: var(--panel-muted); }
    .v { flex: 1 1 auto; color: var(--panel-fg); overflow-wrap: anywhere; }
    .esc-buttons { flex-flow: row wrap; gap: 6px; margin-top: 4px; }
    .setting { align-items: center; gap: 8px; font-size: 13px; }
    .esc-note { font-size: 11px; color: var(--panel-muted); overflow-wrap: anywhere; }
    .esc-status { font-size: 12px; color: var(--accent); min-height: 14px; }
    .save {
        align-items: center;
        gap: 6px;
        padding: 3px 6px;
        border-left: 3px solid transparent;
        cursor: pointer;
    }
    .save:hover { background: rgba(127,127,127,0.12); }
    .save.on { border-left-color: var(--accent); }
    .save-name { gap: 1px; }
    .save-title { font-size: 13px; }
    .save-path { font-size: 10px; color: var(--panel-muted); overflow-wrap: anywhere; }
    .save-drop { color: var(--panel-muted); padding: 0 4px; }
    .save-drop:hover { color: var(--accent); }
    .term-menu {
        display: inline-flex;
        background: var(--accent);
        color: #fff;
        width: 100%;
        max-width: 256px;
        height: 2cm;
        justify-content: center;
        text-align: center;
        align-items: center;
        align-content: center;
        box-shadow: 0 1px 3px var(--shadow);
    }
");

const HELP: &str = "neum: ordinal, reversed and byte-sum gematria over a redb memory

usage: neum [MEMORY]

  MEMORY   the memory file to open (made if it isn't there). Without one: $NEUM_MEMORY,
           else the last memory open, else ./memory

env:
  NEUM_MEMORY   the memory to open when none is given
  NEUM_CONFIG   where settings are kept (default: <config dir>/neum/settings.json)

in the window, esc opens the menu: the memory's details, saves, settings and keys.";

fn main() -> anyhow::Result<()> {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("-h" | "--help") => {
            println!("{HELP}");
            return Ok(());
        }
        Some("-V" | "--version") => {
            println!("neum {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let mut settings = Settings::load();
    let path = startup_memory(arg, &settings);
    // a memory that won't open still leaves the window, where esc can open another
    let greeting = match store::open(&path, settings.compact_on_open) {
        Ok(_) => {
            settings.opened(&canonical(&path));
            if let Err(e) = settings.save() {
                eprintln!("couldn't save settings: {e}");
            }
            String::new()
        }
        Err(e) => format!("{e} · esc to open another"),
    };
    if !greeting.is_empty() { eprintln!("{greeting}"); }

    let neum = app()
        .with_title("Neum") // .with_decorations(Decorations::NoTitleBar)
        .with_stylesheet(STYLE)
        .with_size(666.0, 444.0);

    neum.run(move || {
        let (settings, greeting) = (settings.clone(), greeting.clone());
        view! { Gematria(settings = settings, greeting = greeting) }
    })?;
    store::close();
    Ok(())
}
