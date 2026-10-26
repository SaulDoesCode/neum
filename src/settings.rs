//! what neum remembers between runs, as JSON in the config folder

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::gematria::Mode;

/// how many saves the escape menu keeps at hand
pub const RECENT: usize = 8;

/// the thresholds the escape menu cycles through, in percent
pub const THRESHOLDS: [u8; 4] = [50, 75, 90, 100];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// light mode instead of dark
    pub light: bool,
    /// the memory to open at start (the last one open)
    pub memory: String,
    /// memories opened before, newest first: the saves
    pub recent: Vec<String>,
    /// compact a memory's file when opening it
    pub compact_on_open: bool,
    /// whether the saturate toggle starts on
    pub saturate: bool,
    /// which sum the middle letter starts on
    pub mode: Mode,
    /// percent of a query's distinct words a memory must hold for search to find it
    pub threshold: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            light: false,
            memory: String::new(),
            recent: vec![],
            compact_on_open: true,
            saturate: true,
            mode: Mode::Ordinal,
            threshold: 75,
        }
    }
}

impl Settings {
    /// where settings live: $NEUM_CONFIG, else neum/settings.json in the platform's config
    /// folder ($XDG_CONFIG_HOME, ~/.config, %APPDATA%), else next to the working directory
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("NEUM_CONFIG").filter(|p| !p.is_empty()) {
            return PathBuf::from(p);
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").filter(|p| !p.is_empty()).map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").filter(|p| !p.is_empty()).map(|h| PathBuf::from(h).join(".config")));
        match base {
            Some(b) => b.join("neum").join("settings.json"),
            None => PathBuf::from("neum-settings.json"),
        }
    }

    /// settings from `path`; the defaults if there are none there or they don't parse
    pub fn load_from(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str::<Settings>(&s)
                .map(Settings::sane)
                .unwrap_or_else(|e| {
                    eprintln!("settings at {} don't parse ({e}), using defaults", path.display());
                    Settings::default()
                }),
            Err(_) => Settings::default(),
        }
    }

    pub fn load() -> Settings {
        Settings::load_from(&Settings::path())
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)?;
        }
        // written whole then moved, so a crash mid-write leaves the old settings
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Settings::path())
    }

    /// the threshold as a fraction, for search
    pub fn threshold_fraction(&self) -> f32 {
        self.threshold.clamp(1, 100) as f32 / 100.0
    }

    /// the next threshold in [`THRESHOLDS`] after this one
    pub fn next_threshold(&self) -> u8 {
        THRESHOLDS.iter().copied().find(|&t| t > self.threshold).unwrap_or(THRESHOLDS[0])
    }

    /// `path` is the memory open now: first among the saves, and the one to open next time
    pub fn opened(&mut self, path: &str) {
        self.memory = path.to_string();
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(RECENT);
    }

    /// take a save off the list (the file stays)
    pub fn forget_save(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
    }

    fn sane(mut self) -> Settings {
        self.threshold = self.threshold.clamp(1, 100);
        self.recent.retain(|p| !p.is_empty());
        self.recent.dedup();
        self.recent.truncate(RECENT);
        self
    }
}

/// which memory to open at start: the first argument, else $NEUM_MEMORY, else the last one
/// open, else `./memory`
pub fn startup_memory(arg: Option<String>, settings: &Settings) -> String {
    arg.filter(|a| !a.is_empty())
        .or_else(|| std::env::var("NEUM_MEMORY").ok().filter(|p| !p.is_empty()))
        .or_else(|| Some(settings.memory.clone()).filter(|p| !p.is_empty()))
        .unwrap_or_else(|| "./memory".into())
}
