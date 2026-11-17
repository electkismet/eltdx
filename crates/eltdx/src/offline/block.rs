use super::{
    ascii_replace, bars, cancelled, context_value, decode_gbk, diagnostic, io_error, malformed,
    BlockInfo, CustomerBlock, ReadOptions, ReadResult,
};
use crate::error::Result;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub fn read(path: &Path, options: &ReadOptions) -> Result<ReadResult<BlockInfo>> {
    let bytes = bars::load(path, options)?;
    if bytes.len() < 386 {
        return Err(malformed("block header needs 384 bytes and a u16 count"));
    }
    let count =
        python_legacy::dat_count(&bytes).ok_or_else(|| malformed("block count is incomplete"))?;
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_str()
        .unwrap_or("")
        .to_lowercase();
    let category = python_legacy::category(&name);
    let mut rows = Vec::new();
    for index in 0..count {
        cancelled(options.cancel())?;
        let start = 386 + index * 2813;
        let record = python_legacy::dat_record(&bytes, start).ok_or_else(|| {
            malformed("declared block record is truncated")
                .with_context("record_index", context_value(index))
                .with_context("declared_count", context_value(count))
        })?;
        let name = decode_gbk(record.name).trim_matches('\0').to_owned();
        let declared_count = record.count;
        let reserved = record.reserved;
        let mut codes = Vec::new();
        for bytes in record.codes() {
            cancelled(options.cancel())?;
            let code = ascii_replace(bytes).trim_matches('\0').to_owned();
            if !code.is_empty() {
                codes.push(code);
            }
        }
        rows.push(BlockInfo::new(
            name,
            category,
            declared_count,
            codes,
            reserved,
        ));
    }
    cancelled(options.cancel())?;
    let extra = bytes.len() - (386 + count * 2813);
    let diagnostics = if extra == 0 {
        Vec::new()
    } else {
        vec![diagnostic(
            "block",
            format!("ignored {extra} bytes after declared block records"),
        )]
    };
    Ok(ReadResult::new(rows, extra, diagnostics))
}

/// Shared binary layouts and source text rules. Python owns codec calls and
/// filesystem hooks; strict readers retain their own budgets and diagnostics.
#[doc(hidden)]
pub mod python_legacy {
    pub fn category(lowercase_name: &str) -> u8 {
        if lowercase_name.contains("gn") {
            2
        } else if lowercase_name.contains("fg") {
            3
        } else if lowercase_name.contains("dy") {
            1
        } else {
            0
        }
    }
    pub fn safe_leaf(value: &str) -> bool {
        !value.is_empty() && value != "." && value != ".." && !value.contains(['/', '\\', ':'])
    }
    pub fn dat_count(bytes: &[u8]) -> Option<usize> {
        bytes
            .get(..386)
            .map(|header| usize::from(super::bars::u16_at(header, 384)))
    }
    pub struct DatRecord<'a> {
        pub name: &'a [u8],
        pub count: u16,
        pub reserved: u16,
        code_bytes: &'a [u8],
    }
    impl DatRecord<'_> {
        pub fn codes(&self) -> impl Iterator<Item = &[u8]> {
            self.code_bytes
                .as_chunks::<7>()
                .0
                .iter()
                .take(usize::from(self.count).min(400))
                .map(|code| code.as_slice())
        }
    }
    pub fn dat_record(bytes: &[u8], offset: usize) -> Option<DatRecord<'_>> {
        let row = bytes.get(offset..offset.checked_add(2813)?)?;
        Some(DatRecord {
            name: &row[..9],
            count: super::bars::u16_at(row, 9),
            reserved: super::bars::u16_at(row, 11),
            code_bytes: &row[13..],
        })
    }
    pub fn config_records(bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
        bytes
            .as_chunks::<120>()
            .0
            .iter()
            .map(|record| (&record[..50], &record[50..]))
    }
    pub fn visit_codes<E>(
        text: &str,
        mut emit: impl FnMut(&str) -> Result<(), E>,
        boundary: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), E> {
        for line in super::super::source_lines(text) {
            boundary()?;
            let mut characters = super::super::source_trim(line).chars();
            characters.next();
            let code = characters.as_str();
            if !code.is_empty() {
                emit(code)?;
            }
        }
        boundary()
    }
}

enum Part {
    Prefix(OsString),
    Root,
    Current,
    Parent,
    Normal(OsString),
}
fn parts(path: &Path) -> VecDeque<Part> {
    path.components()
        .map(|part| match part {
            Component::Prefix(prefix) => Part::Prefix(prefix.as_os_str().to_owned()),
            Component::RootDir => Part::Root,
            Component::CurDir => Part::Current,
            Component::ParentDir => Part::Parent,
            Component::Normal(name) => Part::Normal(name.to_owned()),
        })
        .collect()
}

/// Resolves existing symlinks even when their final target is missing, like
/// Python Path.resolve(strict=False), before deciding whether a missing blk is skipped.
fn resolve_allow_missing(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| io_error("current_dir", path, error))?
            .join(path)
    };
    let mut queue = parts(&absolute);
    let mut resolved = PathBuf::new();
    let mut symlinks = 0;
    while let Some(part) = queue.pop_front() {
        match part {
            Part::Prefix(prefix) => resolved.push(prefix),
            Part::Root => resolved.push(std::path::MAIN_SEPARATOR_STR),
            Part::Current => (),
            Part::Parent => {
                resolved.pop();
            }
            Part::Normal(name) => {
                let candidate = resolved.join(&name);
                match std::fs::symlink_metadata(&candidate) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        symlinks += 1;
                        if symlinks > 64 {
                            return Err(malformed(
                                "customer block symlink resolution exceeded 64 links",
                            ));
                        }
                        let target = std::fs::read_link(&candidate)
                            .map_err(|error| io_error("read_link", &candidate, error))?;
                        if target.is_absolute() {
                            resolved.clear();
                        }
                        let mut target_parts = parts(&target);
                        target_parts.append(&mut queue);
                        queue = target_parts;
                    }
                    Ok(_) => resolved.push(name),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        resolved.push(name)
                    }
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
        return Err(
            malformed("customer block resolved path escapes the requested directory")
                .with_context("path", format!("{}", candidate.display())),
        );
    }
    Ok(resolved)
}

pub fn read_custom(dir: &Path, options: &ReadOptions) -> Result<ReadResult<CustomerBlock>> {
    cancelled(options.cancel())?;
    let root = std::fs::canonicalize(dir).map_err(|error| io_error("canonicalize", dir, error))?;
    if !root.is_dir() {
        return Err(io_error(
            "read_custom",
            dir,
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "custom block directory is missing",
            ),
        ));
    }
    let cfg = contained(&root, &root.join("blocknew.cfg"))?;
    let mut budget = bars::ByteBudget::new(options);
    let bytes = bars::load_with_budget(&cfg, options, &mut budget)?;
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, (name_bytes, leaf_bytes)) in python_legacy::config_records(&bytes).enumerate() {
        cancelled(options.cancel())?;
        let name = decode_gbk(name_bytes)
            .split('\0')
            .next()
            .unwrap_or("")
            .to_owned();
        let leaf = decode_gbk(leaf_bytes)
            .split('\0')
            .next()
            .unwrap_or("")
            .to_owned();
        if leaf.is_empty() {
            return Err(malformed("customer block filename must not be empty")
                .with_context("record_index", context_value(index)));
        }
        if !python_legacy::safe_leaf(&leaf) {
            return Err(
                malformed("customer block filename must be a safe leaf name")
                    .with_context("record_index", context_value(index)),
            );
        }
        let path = contained(&root, &root.join(format!("{leaf}.blk")))?;
        let present = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.is_file(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error("metadata", &path, error)),
        };
        if !present || name.is_empty() {
            diagnostics.push(diagnostic(
                "customer_block",
                format!("row {index}: skipped missing blk or empty block name"),
            ));
            continue;
        }
        let content = bars::load_with_budget(&path, options, &mut budget)?;
        let text = String::from_utf8_lossy(&content);
        let mut codes = Vec::new();
        python_legacy::visit_codes(
            &text,
            |code| {
                codes.push(code.to_owned());
                Ok(())
            },
            &mut || cancelled(options.cancel()),
        )?;
        rows.push(CustomerBlock::new(name, leaf, codes));
    }
    cancelled(options.cancel())?;
    let tail = bytes.len() % 120;
    if tail != 0 {
        diagnostics.push(diagnostic(
            "customer_block",
            format!("ignored {tail} trailing cfg bytes"),
        ));
    }
    Ok(ReadResult::new(rows, tail, diagnostics))
}
