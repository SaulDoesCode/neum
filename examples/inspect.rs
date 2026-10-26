//! what a memory holds, without the window:
//!
//! ```text
//! cargo run --example inspect -- path/to/memory          # its details
//! cargo run --example inspect -- path/to/memory 162      # memories at 162, by every sum
//! ```

use neum::{Memory, Mode, gematria::byte_list, human_bytes};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "./memory".into());
    let m = Memory::open(&path, false)?;
    let s = m.stats()?;
    println!("{}", s.path.display());
    println!("  {} memories ({}), {} on disk", s.memories, human_bytes(s.stored_bytes), human_bytes(s.file_bytes));
    println!("  {} lists holding {}, {} notes on {} memories", s.lists, s.listed, s.notes, s.noted);
    println!("  {} ids to reuse, schema v{}", s.reusable, s.schema);
    if let Some(sum) = args.next() {
        let sum: u64 = sum.parse()?;
        for mode in Mode::ALL {
            let mut found: Vec<_> = m.gsearch(sum, mode)?.into_iter().collect();
            found.sort();
            println!("{} {sum}: {}", mode.name(), found.len());
            for (k, v) in found {
                let shown = String::from_utf8(v.clone()).unwrap_or_else(|_| format!("⟨{}⟩", byte_list(&v)));
                println!("  [{}] {shown}", byte_list(&k));
            }
        }
    }
    Ok(())
}
