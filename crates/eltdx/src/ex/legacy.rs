//! Shared extension receive progress. Callers supply the numeric and I/O policy:
//! strict wire code uses bounded usize reads, Python preserves its object hooks.
#[doc(hidden)]
pub fn receive_exact_with<B,N,C,E>(mut buffer: B,target: &N,
    mut needs_more: impl FnMut(&B,&N) -> std::result::Result<bool,E>,
    mut receive: impl FnMut(&B,&N) -> std::result::Result<C,E>,
    mut extend: impl FnMut(&mut B,C) -> std::result::Result<(),E>) -> std::result::Result<B,E> {
    while needs_more(&buffer,target)? {
        let chunk = receive(&buffer,target)?;
        extend(&mut buffer,chunk)?;
    }
    Ok(buffer)
}

#[cfg(test)]
mod source_cases {
    use super::*;

    #[test]
    fn exact_receive_retains_partial_chunks_and_does_not_read_past_target() {
        let mut chunks = [vec![1_u8,2],vec![3],vec![4,5]].into_iter();
        let out = receive_exact_with(Vec::new(),&3,
            |buffer,target| Ok::<_,()>(buffer.len()<*target),
            |_,_| Ok(chunks.next().unwrap()),
            |buffer,chunk| {buffer.extend(chunk);Ok(())}).unwrap();
        assert_eq!(out,[1,2,3]);
        assert_eq!(chunks.next(),Some(vec![4,5]));
        let out = receive_exact_with(Vec::<u8>::new(),&0,
            |buffer,target| Ok::<_,()>(buffer.len()<*target),
            |_,_| panic!("zero length cannot read"),
            |buffer,chunk:Vec<u8>| {buffer.extend(chunk);Ok(())}).unwrap();
        assert!(out.is_empty());
    }
}
