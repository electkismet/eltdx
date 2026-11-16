//! Private, feature-independent Python GBK source decoding.
//!
//! The mapping table is the original T21 source data. It is shared by default
//! HQ statistics and optional offline readers; invalid-byte policies belong to
//! each consumer, not to the table. No WHATWG extensions (notably 0x80/euro).

mod gbk_pairs;

const SCAN_CHUNK_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InvalidBytePolicy {
    Replace,
    Ignore,
}

/// Strict Python GBK mapping for exactly two bytes. Undefined pairs, invalid
/// lead/trail bytes and standalone ASCII return None. Zero is the source table's
/// undefined sentinel; ASCII NUL is handled separately by the scanner.
pub(crate) fn gbk_scalar(lead: u8, trail: u8) -> Option<char> {
    if !(0x81..=0xfe).contains(&lead) || !(0x40..=0xfe).contains(&trail) || trail == 0x7f {
        return None;
    }
    let index = usize::from(lead - 0x81) * 190 + usize::from(trail - 0x40)
        - usize::from(trail > 0x7f);
    let scalar = *gbk_pairs::GBK_PAIRS.get(index)?;
    if scalar == 0 { None } else { char::from_u32(scalar) }
}

/// Emit decoded scalars without allocating a second buffer. On an invalid lead
/// or undefined pair consume one byte, then retry the next byte, matching the
/// original offline replacement decoder and Python GBK ignore behavior.
///
/// `boundary` runs before work, after each <=16 KiB scan chunk (a two-byte scalar
/// may straddle the boundary by one byte), and before successful return. Its
/// original error stops emission immediately. The output already emitted on an
/// error belongs to the caller and must not be published as a complete decode.
/// `Ok(true)` means at least one invalid input byte was replaced/ignored.
pub(crate) fn scan_gbk<E>(
    bytes: &[u8],
    policy: InvalidBytePolicy,
    mut emit: impl FnMut(char),
    boundary: &mut impl FnMut() -> Result<(), E>,
) -> Result<bool, E> {
    boundary()?;
    let mut cursor = 0;
    let mut next_boundary = SCAN_CHUNK_BYTES;
    let mut had_errors = false;
    while cursor < bytes.len() {
        if cursor >= next_boundary {
            boundary()?;
            next_boundary = cursor.saturating_add(SCAN_CHUNK_BYTES);
        }
        let lead = bytes[cursor];
        if lead < 0x80 {
            emit(char::from(lead));
            cursor += 1;
            continue;
        }
        if let Some(scalar) = bytes.get(cursor + 1).and_then(|trail| gbk_scalar(lead, *trail)) {
            emit(scalar);
            cursor += 2;
            continue;
        }
        had_errors = true;
        if policy == InvalidBytePolicy::Replace { emit('\u{fffd}'); }
        cursor += 1;
    }
    boundary()?;
    Ok(had_errors)
}
