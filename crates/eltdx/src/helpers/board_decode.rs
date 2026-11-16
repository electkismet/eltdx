//! Default-HQ internal readers. No optional offline feature, path discovery,
//! implicit local files, disk writes or second encoding table.
use std::collections::{HashMap, HashSet};

use crate::encoding::{scan_gbk, InvalidBytePolicy};
use crate::services::allocation_error;
use crate::types::{BoardCategory, BoardDefinition, BoardFile, BoardId, Diagnostic, EnrichmentStatus,
    HqMarket, ResultMeta, SecurityId};
use crate::{Error, ErrorKind, Result};

const MAX_BOARDS: usize = 65_535;
const MAX_MEMBERS: usize = 1_000_000;
const MAX_LINE: usize = 32 * 1024;

#[derive(Debug)]
pub(super) struct BoardData {
    pub definitions: Vec<BoardDefinition>, pub files: Vec<BoardFile>,
    pub concepts: HashMap<String, Vec<SecurityId>>, pub meta: ResultMeta,
}
impl BoardData {
    pub fn file(&self, name: &str) -> Option<&BoardFile> { self.files.iter().find(|file| file.name() == name) }
    pub fn weight(&self) -> usize {
        let raw_bytes = self.meta.raw().map_or(0, |raw| raw.payloads().iter()
            .chain(raw.wire_frames().unwrap_or(&[])).fold(0_usize, |sum, bytes| sum.saturating_add(bytes.len())));
        self.definitions.len().saturating_add(self.concepts.values().map(Vec::len).sum::<usize>())
            .saturating_add(self.files.iter().map(|file| file.data().len()).sum::<usize>().div_ceil(4096))
            .saturating_add(raw_bytes.div_ceil(4096))
    }
    pub fn members(&self, board: &BoardDefinition, boundary: &mut impl FnMut() -> Result<()>) -> Result<Vec<SecurityId>> {
        boundary()?;
        match board.category() {
            BoardCategory::Concept | BoardCategory::Style => {
                let source = self.concepts.get(board.id().number()).ok_or_else(|| malformed("board has no complete infoharbor membership section"))?;
                let mut members = Vec::new(); members.try_reserve(source.len()).map_err(allocation_error)?;
                for id in source { boundary()?; members.push(id.clone()); } Ok(members)
            }
            BoardCategory::Industry1 | BoardCategory::Industry2 | BoardCategory::Industry3 | BoardCategory::Industry880 => {
                let file = self.file("tdxhy.cfg").ok_or_else(|| malformed("industry membership requires tdxhy.cfg"))?;
                industry_members(file.data(), board, boundary)
            }
            BoardCategory::Region => {
                let file = self.file("base.dbf").ok_or_else(|| malformed("region membership requires actual base.dbf; the HQ download list does not supply it"))?;
                dbf_members(file.data(), board.membership_key(), boundary)
            }
            BoardCategory::All => Err(malformed("All is a query selector, not a board category")),
        }
    }
}

pub(super) fn parse(files: Vec<BoardFile>, mut meta: ResultMeta, boundary: &mut impl FnMut() -> Result<()>) -> Result<BoardData> {
    boundary()?;
    let mut definitions = Vec::new(); let mut positions = HashMap::new(); let mut concepts = HashMap::new();
    for name in ["tdxzs.cfg", "tdxzs3.cfg"] {
        let Some(file) = files.iter().find(|file| file.name() == name) else { continue; };
        let text = text(file.data(), boundary)?;
        for line in text.lines() {
            boundary()?; bounded_line(line)?; let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let fields = line.split('|').take(7).collect::<Vec<_>>();
            if fields.len() < 6 || !six_digits(fields[1].trim()) { continue; }
            let number = fields[1].trim();
            let raw = fields[2].trim().parse::<u16>().map_err(|_| malformed("board definition category is invalid"))?;
            let key = fields[5].trim();
            let category = match category(raw, key) {
                Some(category) => category,
                None => { if meta.diagnostics().len() < 128 { meta = meta.with_diagnostic(diagnostic("boards.category", format!("unsupported category/key retained as exclusion: {number}/{raw}/{key}"))); } continue; }
            };
            let (market, inferred) = board_market(number, fields[3].trim());
            if inferred && meta.diagnostics().len() < 128 { meta = meta.with_diagnostic(diagnostic("boards.market", format!("{number}: market inferred from board prefix/default"))); }
            let definition = BoardDefinition { id: BoardId::new(number, Some(market))?, name: if fields[0].trim().is_empty() {
                number.to_owned() } else { fields[0].trim().to_owned() }, market, category, category_raw: raw, membership_key: key.to_owned() };
            insert_definition(&mut definitions, &mut positions, definition)?;
        }
    }
    if let Some(file) = files.iter().find(|file| file.name() == "infoharbor_block.dat") {
        let text = text(file.data(), boundary)?;
        let mut current: Option<Section> = None; let mut total = 0;
        for line in text.lines() {
            boundary()?; bounded_line(line)?; let line = line.trim();
            if line.is_empty() { continue; }
            if line.starts_with('#') {
                if let Some(section) = current.take() { finish_section(section, &mut concepts, &mut total)?; }
                let fields = line.split(',').take(4).collect::<Vec<_>>();
                if fields.len() < 3 || !six_digits(fields[2].trim()) { return Err(malformed("infoharbor header must contain a six-digit code and member count")); }
                let raw_name = fields[0].strip_prefix('#').unwrap_or("").trim();
                let (name, category, raw) = if let Some(name) = raw_name.strip_prefix("GN_") { (name, BoardCategory::Concept, 4) }
                    else if let Some(name) = raw_name.strip_prefix("FG_") { (name, BoardCategory::Style, 5) }
                    else { continue; };
                let count = fields[1].trim().parse::<usize>().map_err(|_| malformed("infoharbor member count is invalid"))?;
                if count > MAX_MEMBERS { return Err(limit("infoharbor member count exceeds its bound")); }
                let number = fields[2].trim(); let (market, _) = board_market(number, "1");
                insert_definition(&mut definitions, &mut positions, BoardDefinition { id: BoardId::new(number, Some(market))?,
                    name: name.to_owned(), market, category, category_raw: raw, membership_key: String::new() })?;
                current = Some(Section { number: number.to_owned(), expected: count, members: Vec::new(), seen: HashSet::new() });
            } else if let Some(section) = &mut current {
                let bytes = line.as_bytes(); let mut offset = 0;
                while offset + 8 <= bytes.len() {
                    if offset % 1024 == 0 { boundary()?; }
                    if matches!(bytes[offset], b'0' | b'1' | b'2') && bytes[offset + 1] == b'#'
                        && bytes[offset + 2..offset + 8].iter().all(u8::is_ascii_digit)
                        && (offset == 0 || !bytes[offset - 1].is_ascii_digit())
                        && bytes.get(offset + 8).is_none_or(|byte| !byte.is_ascii_digit()) {
                        let number = std::str::from_utf8(&bytes[offset + 2..offset + 8]).map_err(|_| malformed("infoharbor member code is not ASCII"))?;
                        let id = SecurityId::new(market(bytes[offset] - b'0')?, number)?;
                        push_unique(&mut section.members, &mut section.seen, id)?; offset += 8;
                    } else { offset += 1; }
                }
            }
        }
        if let Some(section) = current { finish_section(section, &mut concepts, &mut total)?; }
    }
    if definitions.is_empty() { return Err(malformed("no supported board definitions were found in the selected sources")); }
    boundary()?; Ok(BoardData { definitions, files, concepts, meta })
}

struct Section { number: String, expected: usize, members: Vec<SecurityId>, seen: HashSet<SecurityId> }
fn finish_section(section: Section, concepts: &mut HashMap<String, Vec<SecurityId>>, total: &mut usize) -> Result<()> {
    if section.members.len() != section.expected { return Err(malformed("infoharbor unique membership count disagrees with its header")
        .with_context("board", section.number).with_context("expected", format!("{}", section.expected))
        .with_context("actual", format!("{}", section.members.len()))); }
    *total = total.checked_add(section.members.len()).ok_or_else(|| limit("aggregate membership overflows"))?;
    if *total > MAX_MEMBERS { return Err(limit("aggregate infoharbor memberships exceed one million")); }
    concepts.try_reserve(1).map_err(allocation_error)?;
    if concepts.insert(section.number, section.members).is_some() { return Err(malformed("duplicate infoharbor board section")); }
    Ok(())
}

fn insert_definition(definitions: &mut Vec<BoardDefinition>, positions: &mut HashMap<String, usize>, definition: BoardDefinition) -> Result<()> {
    if positions.contains_key(definition.id.number()) { return Ok(()); } // First source occurrence wins, as documented.
    if definitions.len() == MAX_BOARDS { return Err(limit("board definitions exceed 65535")); }
    definitions.try_reserve(1).map_err(allocation_error)?; positions.try_reserve(1).map_err(allocation_error)?;
    positions.insert(definition.id.number().to_owned(), definitions.len()); definitions.push(definition); Ok(())
}

fn category(raw: u16, key: &str) -> Option<BoardCategory> {
    match raw {
        4 => Some(BoardCategory::Concept), 5 => Some(BoardCategory::Style),
        3 if !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit()) => Some(BoardCategory::Region),
        2 if !key.is_empty() => Some(BoardCategory::Industry880),
        12 if key.starts_with('X') && key.as_bytes()[1..].iter().all(u8::is_ascii_digit) => match key.len() {
            3 => Some(BoardCategory::Industry1), 5 => Some(BoardCategory::Industry2), 7 => Some(BoardCategory::Industry3), _ => None },
        _ => None,
    }
}
fn board_market(number: &str, declared: &str) -> (HqMarket, bool) {
    if number.starts_with("399") { return (HqMarket::Shenzhen, declared != "0"); }
    if number.starts_with("880") || number.starts_with("881") { return (HqMarket::Shanghai, declared != "1"); }
    match declared { "0" => (HqMarket::Shenzhen, false), "1" => (HqMarket::Shanghai, false),
        "2" => (HqMarket::Beijing, false), _ => (HqMarket::Shanghai, true) }
}
fn market(value: u8) -> Result<HqMarket> { match value { 0 => Ok(HqMarket::Shenzhen), 1 => Ok(HqMarket::Shanghai),
    2 => Ok(HqMarket::Beijing), _ => Err(malformed("board member has an unsupported HQ market")) } }

fn industry_members(bytes: &[u8], board: &BoardDefinition, boundary: &mut impl FnMut() -> Result<()>) -> Result<Vec<SecurityId>> {
    let text = text(bytes, boundary)?; let column = if board.category() == BoardCategory::Industry880 { 2 } else { 5 };
    let mut members = Vec::new(); let mut seen = HashSet::new();
    for line in text.lines() {
        boundary()?; bounded_line(line)?; let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let fields = line.split('|').take(7).collect::<Vec<_>>();
        if fields.len() <= column { return Err(malformed("industry member row lacks required mapping columns")); }
        if !fields[column].trim().starts_with(board.membership_key()) { continue; }
        let market_id = fields[0].trim().parse::<u8>().map_err(|_| malformed("industry member market is invalid"))?;
        let id = SecurityId::new(market(market_id)?, fields[1].trim()).map_err(|error| malformed("industry member code is invalid").with_source(error))?;
        push_unique(&mut members, &mut seen, id)?;
    }
    boundary()?; Ok(members)
}

fn dbf_members(bytes: &[u8], region: &str, boundary: &mut impl FnMut() -> Result<()>) -> Result<Vec<SecurityId>> {
    boundary()?;
    if bytes.len() < 33 { return Err(malformed("DBF requires a complete fixed header and field terminator")); }
    let count = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| malformed("DBF record count is truncated"))?) as usize;
    let header = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
    let record = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
    if count > MAX_MEMBERS || header < 33 || header > bytes.len() || record < 2 { return Err(malformed("DBF declared dimensions exceed available bounds")); }
    let end = count.checked_mul(record).and_then(|size| header.checked_add(size)).ok_or_else(|| malformed("DBF record span overflows"))?;
    if end > bytes.len() { return Err(malformed("DBF record array is truncated")); }
    if bytes[end..].iter().any(|byte| !matches!(*byte, 0 | 0x1a)) { return Err(malformed("DBF has unrecognized bytes after its record array")); }
    let mut fields = HashMap::new(); let mut offset = 1_usize; let mut pos = 32; let mut terminated = false;
    while pos < header {
        boundary()?;
        if bytes[pos] == 0x0d { terminated = true;
            if bytes[pos + 1..header].iter().any(|byte| *byte != 0) { return Err(malformed("DBF field header has nonzero bytes after its terminator")); }
            break;
        }
        let descriptor = bytes.get(pos..pos + 32).filter(|_| pos + 32 <= header).ok_or_else(|| malformed("DBF field descriptor is truncated"))?;
        let name_end = descriptor[..11].iter().position(|byte| *byte == 0).unwrap_or(11);
        let name = std::str::from_utf8(&descriptor[..name_end]).ok().filter(|name| name.is_ascii() && !name.is_empty())
            .ok_or_else(|| malformed("DBF field name must be nonempty ASCII"))?;
        let width = usize::from(descriptor[16]);
        let next = offset.checked_add(width).ok_or_else(|| malformed("DBF field offset overflows"))?;
        if width == 0 || next > record { return Err(malformed("DBF field width exceeds its record")); }
        if fields.insert(name.to_owned(), (offset, width)).is_some() { return Err(malformed("DBF contains duplicate field names")); }
        offset = next; pos += 32;
    }
    if !terminated || offset > record { return Err(malformed("DBF has no bounded field terminator")); }
    for name in ["SC", "GPDM", "DY"] { if !fields.contains_key(name) { return Err(malformed("DBF region lookup requires SC, GPDM and DY fields")); } }
    let region = region.parse::<u32>().map_err(|_| malformed("region membership key must be a bounded nonnegative integer"))?;
    let mut members = Vec::new(); let mut seen = HashSet::new();
    for index in 0..count {
        boundary()?; let row = &bytes[header + index * record..header + (index + 1) * record];
        if row[0] == b'*' { continue; }
        if row[0] != b' ' { return Err(malformed("DBF record deletion marker is invalid")); }
        let value = |name: &str| -> Result<&str> {
            let (offset, width) = fields.get(name).copied().ok_or_else(|| malformed("DBF required field disappeared"))?;
            let value = std::str::from_utf8(&row[offset..offset + width]).ok().filter(|value| value.is_ascii())
                .ok_or_else(|| malformed("DBF selected field is not ASCII"))?;
            Ok(value.trim_matches(|value: char| value.is_ascii_whitespace() || value == '\0'))
        };
        let actual_region = value("DY")?.parse::<u32>().ok();
        if actual_region != Some(region) { continue; }
        let market_id = value("SC")?.parse::<u8>().map_err(|_| malformed("DBF member market is invalid"))?;
        let id = SecurityId::new(market(market_id)?, value("GPDM")?).map_err(|error| malformed("DBF member code is invalid").with_source(error))?;
        push_unique(&mut members, &mut seen, id)?;
    }
    boundary()?; Ok(members)
}

fn push_unique(members: &mut Vec<SecurityId>, seen: &mut HashSet<SecurityId>, id: SecurityId) -> Result<()> {
    if seen.contains(&id) { return Ok(()); }
    if members.len() == MAX_MEMBERS { return Err(limit("board memberships exceed one million")); }
    members.try_reserve(1).map_err(allocation_error)?; seen.try_reserve(1).map_err(allocation_error)?;
    seen.insert(id.clone()); members.push(id); Ok(())
}
fn six_digits(value: &str) -> bool { value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit()) }
fn bounded_line(value: &str) -> Result<()> { if value.len() > MAX_LINE { Err(limit("board text line exceeds 32 KiB")) } else { Ok(()) } }
fn text(bytes: &[u8], boundary: &mut impl FnMut() -> Result<()>) -> Result<String> {
    if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return std::str::from_utf8(bytes).map(str::to_owned).map_err(|error| malformed("board UTF-8 BOM source is invalid").with_source(error));
    }
    let mut text = String::new(); text.try_reserve_exact(bytes.len().checked_mul(3).ok_or_else(|| limit("board text size overflows"))?).map_err(allocation_error)?;
    let invalid = scan_gbk(bytes, InvalidBytePolicy::Replace, |scalar| text.push(scalar), boundary)?;
    if invalid { match std::str::from_utf8(bytes) { Ok(value) => Ok(value.to_owned()),
        Err(error) => Err(malformed("board source is neither complete GBK nor UTF-8").with_source(error)) } }
    else { Ok(text) }
}
fn diagnostic(stage: &str, message: String) -> Diagnostic { Diagnostic::new("board_files", stage, EnrichmentStatus::Unavailable, None, Some(message)) }
fn malformed(message: &str) -> Error { Error::new(ErrorKind::ResourceFormat, message).with_context("domain", "HQ boards") }
fn limit(message: &str) -> Error { Error::new(ErrorKind::LimitExceeded, message).with_context("domain", "HQ boards") }

#[cfg(test)]
mod source_cases {
    use super::*;
    use bytes::Bytes;
    fn file(name: &str, content: &[u8]) -> BoardFile { BoardFile::new(name, Bytes::copy_from_slice(content)).unwrap() }
    fn decode(files: Vec<BoardFile>) -> Result<BoardData> { parse(files, ResultMeta::default(), &mut || Ok(())) }

    #[test]
    fn infoharbor_verifies_unique_count_and_keeps_first_appearance_member_order() {
        let data = decode(vec![file("infoharbor_block.dat", b"#GN_Concept,3,880001\n1#600001,0#000001,1#600001,2#920001\n#FG_Style,0,880002\n")]).unwrap();
        assert_eq!(data.definitions.len(), 2); assert_eq!(data.definitions[0].category(), BoardCategory::Concept);
        assert_eq!(data.definitions[1].category(), BoardCategory::Style);
        let members = data.members(&data.definitions[0], &mut || Ok(())).unwrap();
        assert_eq!(members, vec![SecurityId::new(HqMarket::Shanghai, "600001").unwrap(),
            SecurityId::new(HqMarket::Shenzhen, "000001").unwrap(), SecurityId::new(HqMarket::Beijing, "920001").unwrap()]);
        assert!(data.members(&data.definitions[1], &mut || Ok(())).unwrap().is_empty());
        for contents in [b"#GN_Bad,2,880001\n1#600000,1#600000\n".as_slice(),
            b"#GN_Bad,1,880001\n1#6000000\n", b"#GN_Bad,0,880001\n#GN_Again,0,880001\n"] {
            assert_eq!(decode(vec![file("infoharbor_block.dat", contents)]).unwrap_err().kind(), ErrorKind::ResourceFormat);
        }
    }

    #[test]
    fn definitions_map_known_categories_industry_depth_and_declared_markets_without_default_category_guess() {
        let definitions = b"One|880001|12|1|0|X01\nTwo|880002|12|1|0|X0102\nThree|880003|12|1|0|X010203\nLegacy|880004|2|1|0|801\nRegion|880005|3|1|0|13\nConcept|399001|4|1|0|\nUnknown|880006|99|1|0|\n";
        let data = decode(vec![file("tdxzs.cfg", definitions)]).unwrap();
        assert_eq!(data.definitions.iter().map(BoardDefinition::category).collect::<Vec<_>>(), vec![BoardCategory::Industry1,
            BoardCategory::Industry2, BoardCategory::Industry3, BoardCategory::Industry880, BoardCategory::Region, BoardCategory::Concept]);
        assert_eq!(data.definitions[5].market(), HqMarket::Shenzhen);
        assert!(data.meta.diagnostics().iter().any(|value| value.stage() == "boards.category"));
        assert!(data.meta.diagnostics().iter().any(|value| value.stage() == "boards.market"));
        let duplicate = decode(vec![file("tdxzs.cfg", b"First|880001|4|1|0|\n"), file("tdxzs3.cfg", b"Second|880001|4|1|0|\n")]).unwrap();
        assert_eq!(duplicate.definitions.len(), 1); assert_eq!(duplicate.definitions[0].name(), "First");
    }

    #[test]
    fn industry_members_use_correct_column_and_prefix_with_stable_dedup() {
        let data = decode(vec![file("tdxzs.cfg", b"One|880001|12|1|0|X01\nLegacy|880002|2|1|0|801\n"),
            file("tdxhy.cfg", b"1|600001|801010|x|x|X010203\n0|000001|802|x|x|X010204\n1|600001|801020|x|x|X010203\n2|920001|801030|x|x|X020304\n")]).unwrap();
        let first = data.members(&data.definitions[0], &mut || Ok(())).unwrap();
        let legacy = data.members(&data.definitions[1], &mut || Ok(())).unwrap();
        assert_eq!(first, vec![SecurityId::new(HqMarket::Shanghai, "600001").unwrap(),
            SecurityId::new(HqMarket::Shenzhen, "000001").unwrap()]);
        assert_eq!(legacy, vec![SecurityId::new(HqMarket::Shanghai, "600001").unwrap(),
            SecurityId::new(HqMarket::Beijing, "920001").unwrap()]);
        assert_eq!(data.members(&data.definitions[0], &mut || Err(Error::new(ErrorKind::Cancelled, "stop"))).unwrap_err().kind(), ErrorKind::Cancelled);
    }

    fn dbf(rows: &[(u8, &str, &str, &str)]) -> Vec<u8> {
        let header = 32 + 3 * 32 + 1; let width = 1 + 1 + 6 + 3;
        let mut bytes = vec![0; header]; bytes[0] = 3;
        bytes[4..8].copy_from_slice(&(rows.len() as u32).to_le_bytes());
        bytes[8..10].copy_from_slice(&(header as u16).to_le_bytes()); bytes[10..12].copy_from_slice(&(width as u16).to_le_bytes());
        for (index, (name, size)) in [("SC", 1), ("GPDM", 6), ("DY", 3)].into_iter().enumerate() {
            let offset = 32 + index * 32; bytes[offset..offset + name.len()].copy_from_slice(name.as_bytes());
            bytes[offset + 11] = b'C'; bytes[offset + 16] = size;
        }
        bytes[header - 1] = 0x0d;
        for &(marker, market, code, region) in rows {
            let mut record = vec![b' '; width]; record[0] = marker;
            record[1..1 + market.len()].copy_from_slice(market.as_bytes()); record[2..2 + code.len()].copy_from_slice(code.as_bytes());
            record[8..8 + region.len()].copy_from_slice(region.as_bytes()); bytes.extend_from_slice(&record);
        }
        bytes.push(0x1a); bytes
    }

    #[test]
    fn region_dbf_requires_real_data_filters_deleted_rows_and_retains_first_occurrence() {
        let bytes = dbf(&[(b' ', "1", "600001", "13"), (b'*', "1", "600002", "13"), (b' ', "0", "000001", "14"),
            (b' ', "1", "600001", "13"), (b' ', "2", "920001", "013")]);
        let members = dbf_members(&bytes, "13", &mut || Ok(())).unwrap();
        assert_eq!(members, vec![SecurityId::new(HqMarket::Shanghai, "600001").unwrap(),
            SecurityId::new(HqMarket::Beijing, "920001").unwrap()]);
        let missing = decode(vec![file("tdxzs.cfg", b"Region|880001|3|1|0|13\n")]).unwrap();
        assert_eq!(missing.members(&missing.definitions[0], &mut || Ok(())).unwrap_err().kind(), ErrorKind::ResourceFormat);
    }

    #[test]
    fn dbf_rejects_truncated_arrays_bad_field_widths_names_and_record_markers() {
        let good = dbf(&[(b' ', "1", "600001", "13")]);
        let mut truncated = good.clone(); truncated.truncate(130);
        let mut width = good.clone(); width[32 + 16] = 250;
        let mut duplicate = good.clone(); duplicate[64..75].fill(0); duplicate[64..66].copy_from_slice(b"SC");
        let mut marker = good.clone(); marker[129] = b'?';
        let mut no_terminator = good.clone(); no_terminator[128] = 0;
        for bytes in [truncated, width, duplicate, marker, no_terminator] {
            assert_eq!(dbf_members(&bytes, "13", &mut || Ok(())).unwrap_err().kind(), ErrorKind::ResourceFormat);
        }
    }

    #[test]
    fn bounded_text_uses_shared_gbk_and_explicit_utf8_bom_and_checks_cancellation() {
        assert_eq!(text(b"\xd6\xd0\xce\xc4", &mut || Ok(())).unwrap(), "中文");
        assert_eq!(text("\u{feff}中文".as_bytes(), &mut || Ok(())).unwrap(), "中文");
        assert_eq!(text(b"\xff", &mut || Ok(())).unwrap_err().kind(), ErrorKind::ResourceFormat);
        assert_eq!(bounded_line(&"x".repeat(MAX_LINE + 1)).unwrap_err().kind(), ErrorKind::LimitExceeded);
        assert_eq!(parse(vec![], ResultMeta::default(), &mut || Err(Error::new(ErrorKind::Cancelled, "stop"))).unwrap_err().kind(), ErrorKind::Cancelled);
    }
}
