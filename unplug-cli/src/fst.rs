use crate::opt::list::Options;
use anyhow::{bail, Result};
use log::info;
use std::fs::{self, File};
use std::path::Path;
use unicase::UniCase;
use unplug::common::io::copy_buffered;
use unplug::dvd::{Entry, EntryId, FileTree, Glob, OpenFile};

/// Lists files in an FST matched by `glob`.
pub fn list_files(tree: &FileTree, opt: &Options, glob: &Glob) -> Result<()> {
    let get_file = |(p, e)| tree[e].file().map(|f| (p, f));
    let mut files = glob.find(tree).filter_map(get_file).collect::<Vec<_>>();
    if files.is_empty() {
        bail!("No files found");
    }
    if opt.by_offset {
        files.sort_unstable_by_key(|(_, f)| f.offset);
    } else if opt.by_size {
        files.sort_unstable_by_key(|(_, f)| f.size);
    } else {
        files.sort_unstable_by(|(p1, _), (p2, _)| UniCase::new(p1).cmp(&UniCase::new(p2)));
    }
    if opt.reverse {
        files.reverse();
    }
    for (path, file) in files {
        if opt.long {
            println!("{:<8x} {:<8x} {}", file.offset, file.size, path);
        } else {
            println!("{}", path);
        }
    }
    Ok(())
}

/// Extracts a file from a disc or archive.
pub fn extract_file(
    source: &mut dyn OpenFile,
    entry: EntryId,
    entry_path: &str,
    out_dir: &Path,
    out_name: Option<&str>,
    io_buf: &mut [u8],
) -> Result<()> {
    let file = source.query_file(entry);
    let name = out_name.unwrap_or_else(|| file.name());
    let out_path = if name.is_empty() { out_dir.to_owned() } else { out_dir.join(name) };
    match file {
        Entry::File(_) => {
            info!("Extracting {}", entry_path);
            let mut writer = File::create(&out_path)?;
            let mut reader = source.open_file(entry)?;
            copy_buffered(&mut reader, &mut writer, io_buf)?;
        }
        Entry::Directory(dir) => {
            fs::create_dir_all(&out_path)?;
            for child in dir.children.clone() {
                let child_file = source.query_file(child);
                let child_path =
                    format!("{}/{}", entry_path.trim_end_matches('/'), child_file.name());
                extract_file(source, child, &child_path, &out_path, None, io_buf)?;
            }
        }
    }
    Ok(())
}
