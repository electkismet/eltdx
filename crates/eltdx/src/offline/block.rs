use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use crate::error::Result;
use super::{ascii_replace, bars, cancelled, decode_gbk, diagnostic, io_error, malformed,
    source_lines, source_trim, BlockInfo, CustomerBlock, ReadOptions, ReadResult};

pub fn read(path: &Path, options: &ReadOptions) -> Result<ReadResult<BlockInfo>> {
    let bytes = bars::load(path, options)?;
    if bytes.len() < 386 { return Err(malformed("block header needs 384 bytes and a u16 count")); }
    let count = usize::from(bars::u16_at(&bytes, 384));
    let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().to_lowercase();
    let category = if name.contains("gn") { 2 } else if name.contains("fg") { 3 }
        else if name.contains("dy") { 1 } else { 0 };
    let mut rows = Vec::new();
    for index in 0..count {
        cancelled(options.cancel())?;
        let start = 386 + index * 2813;
        let record = bytes.get(start..start + 2813).ok_or_else(|| malformed("declared block record is truncated")
            .with_context("record_index", index.to_string()).with_context("declared_count", count.to_string()))?;
        let name = decode_gbk(&record[..9]).trim_matches('\0').to_owned();
        let declared_count = bars::u16_at(record, 9);
        let reserved = bars::u16_at(record, 11);
        let mut codes = Vec::new();
        for slot in 0..usize::from(declared_count).min(400) {
            cancelled(options.cancel())?;
            let start = 13 + slot * 7;
            let code = ascii_replace(&record[start..start + 7]).trim_matches('\0').to_owned();
            if !code.is_empty() { codes.push(code); }
        }
        rows.push(BlockInfo::new(name, category, declared_count, codes, reserved));
    }
    cancelled(options.cancel())?;
    let extra = bytes.len() - (386 + count * 2813);
    let diagnostics = if extra == 0 { Vec::new() } else {
        vec![diagnostic("block", format!("ignored {extra} bytes after declared block records"))]
    };
    Ok(ReadResult::new(rows, extra, diagnostics))
}

fn safe_leaf(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && !value.contains(['/', '\\', ':'])
}

enum Part { Prefix(OsString), Root, Current, Parent, Normal(OsString) }
fn parts(path: &Path) -> VecDeque<Part> {
    path.components().map(|part| match part {
        Component::Prefix(prefix) => Part::Prefix(prefix.as_os_str().to_owned()),
        Component::RootDir => Part::Root,
        Component::CurDir => Part::Current,
        Component::ParentDir => Part::Parent,
        Component::Normal(name) => Part::Normal(name.to_owned()),
    }).collect()
}

/// Resolves existing symlinks even when their final target is missing, like
/// Python Path.resolve(strict=False), before deciding whether a missing blk is skipped.
fn resolve_allow_missing(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() { path.to_path_buf() } else {
        std::env::current_dir().map_err(|error| io_error("current_dir", path, error))?.join(path)
    };
    let mut queue = parts(&absolute);
    let mut resolved = PathBuf::new();
    let mut symlinks = 0;
    while let Some(part) = queue.pop_front() {
        match part {
            Part::Prefix(prefix) => resolved.push(prefix),
            Part::Root => resolved.push(std::path::MAIN_SEPARATOR_STR),
            Part::Current => (),
            Part::Parent => { resolved.pop(); }
            Part::Normal(name) => {
                let candidate = resolved.join(&name);
                match std::fs::symlink_metadata(&candidate) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        symlinks += 1;
                        if symlinks > 64 { return Err(malformed("customer block symlink resolution exceeded 64 links")); }
                        let target = std::fs::read_link(&candidate).map_err(|error| io_error("read_link", &candidate, error))?;
                        if target.is_absolute() { resolved.clear(); }
                        let mut target_parts = parts(&target);
                        target_parts.append(&mut queue);
                        queue = target_parts;
                    }
                    Ok(_) => resolved.push(name),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => resolved.push(name),
                    Err(error) => return Err(io_error("symlink_metadata", &candidate, error)),
                }
            }
        }
    }
    Ok(resolved)
}

fn contained(root: &Path, candidate: &Path) -> Result<PathBuf> {
    let resolved = resolve_allow_missing(candidate)?;
    if !resolved.starts_with(root) {
        return Err(malformed("customer block resolved path escapes the requested directory")
            .with_context("path", candidate.display().to_string()));
    }
    Ok(resolved)
}

pub fn read_custom(dir: &Path, options: &ReadOptions) -> Result<ReadResult<CustomerBlock>> {
    cancelled(options.cancel())?;
    let root = std::fs::canonicalize(dir).map_err(|error| io_error("canonicalize", dir, error))?;
    if !root.is_dir() {
        return Err(io_error("read_custom", dir,
            std::io::Error::new(std::io::ErrorKind::NotFound, "custom block directory is missing")));
    }
    let cfg = contained(&root, &root.join("blocknew.cfg"))?;
    let mut budget = bars::ByteBudget::new(options);
    let bytes = bars::load_with_budget(&cfg, options, &mut budget)?;
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, record) in bytes.chunks_exact(120).enumerate() {
        cancelled(options.cancel())?;
        let name = decode_gbk(&record[..50]).split('\0').next().unwrap_or("").to_owned();
        let leaf = decode_gbk(&record[50..]).split('\0').next().unwrap_or("").to_owned();
        if leaf.is_empty() {
            diagnostics.push(diagnostic("customer_block", format!("row {index}: skipped empty block filename")));
            continue;
        }
        if !safe_leaf(&leaf) {
            return Err(malformed("customer block filename must be a safe leaf name")
                .with_context("record_index", index.to_string()));
        }
        let path = contained(&root, &root.join(format!("{leaf}.blk")))?;
        let present = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.is_file(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error("metadata", &path, error)),
        };
        if !present || name.is_empty() {
            diagnostics.push(diagnostic("customer_block", format!("row {index}: skipped missing blk or empty block name")));
            continue;
        }
        let content = bars::load_with_budget(&path, options, &mut budget)?;
        let text = String::from_utf8_lossy(&content);
        let mut codes = Vec::new();
        for line in source_lines(&text) {
            cancelled(options.cancel())?;
            let mut characters = source_trim(line).chars();
            characters.next(); // Remove exactly one Unicode market character.
            let code = characters.as_str();
            if !code.is_empty() { codes.push(code.to_owned()); }
        }
        rows.push(CustomerBlock::new(name, leaf, codes));
    }
    cancelled(options.cancel())?;
    let tail = bytes.len() % 120;
    if tail != 0 { diagnostics.push(diagnostic("customer_block", format!("ignored {tail} trailing cfg bytes"))); }
    Ok(ReadResult::new(rows, tail, diagnostics))
}
