//! Common capped/EOF paging without an implicit legacy maximum.
pub struct DownloadCursor {
    chunk_size: usize,
    maximum: Option<usize>,
    offset: usize,
    eof: bool,
}

impl DownloadCursor {
    pub(crate) fn new(chunk_size: usize, maximum: Option<usize>) -> Self {
        Self {
            chunk_size,
            maximum,
            offset: 0,
            eof: false,
        }
    }
    pub(crate) fn offset(&self) -> usize {
        self.offset
    }
    pub(crate) fn request(&self) -> Option<(usize, usize)> {
        if self.eof
            || self.chunk_size == 0
            || self.maximum.is_some_and(|maximum| self.offset >= maximum)
        {
            return None;
        }
        Some((
            self.offset,
            self.maximum.map_or(self.chunk_size, |maximum| {
                self.chunk_size.min(maximum - self.offset)
            }),
        ))
    }
    pub(crate) fn advance(&mut self, count: usize) -> Option<()> {
        let (_, requested) = self.request()?;
        self.offset = self.offset.checked_add(count)?;
        self.eof = count < requested;
        Some(())
    }
}

pub fn legacy_resource_download<T, E>(
    chunk_size: usize,
    maximum: Option<usize>,
    mut fetch: impl FnMut(usize, usize) -> Result<(T, usize), E>,
    overflow: impl Fn() -> E,
) -> Result<Vec<T>, E> {
    let mut cursor = DownloadCursor::new(chunk_size, maximum);
    let mut chunks = Vec::new();
    while let Some((offset, size)) = cursor.request() {
        let (chunk, count) = fetch(offset, size)?;
        chunks.push(chunk);
        cursor.advance(count).ok_or_else(&overflow)?;
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_size_fetches_eof_and_caps_truncate_without_extra_read() {
        let mut calls = Vec::new();
        legacy_resource_download(
            4,
            None,
            |offset, size| {
                calls.push((offset, size));
                Ok::<_, &str>(((), if offset < 8 { 4 } else { 0 }))
            },
            || "overflow",
        )
        .unwrap();
        assert_eq!(calls, [(0, 4), (4, 4), (8, 4)]);
        calls.clear();
        legacy_resource_download(
            4,
            Some(6),
            |offset, size| {
                calls.push((offset, size));
                Ok::<_, &str>(((), size))
            },
            || "overflow",
        )
        .unwrap();
        assert_eq!(calls, [(0, 4), (4, 2)]);
    }
}
