//! Shared F10 table traversal and projection policy. Python adapters retain
//! dynamic object operations; native callers keep lossless typed cells.

pub fn route_with<E>(
    ladder: impl FnOnce() -> Result<bool, E>,
    international: impl FnOnce() -> Result<bool, E>,
    futures: impl FnOnce() -> Result<bool, E>,
) -> Result<super::Route, E> {
    if ladder()? {
        Ok(super::Route::Ladder)
    } else if international()? || futures()? {
        Ok(super::Route::International)
    } else {
        Ok(super::Route::Static)
    }
}

pub fn allowed_symbols<T, E>(
    symbols: impl IntoIterator<Item = Result<T, E>>,
    mut alphanumeric: impl FnMut(&T) -> Result<bool, E>,
    mut allowed: impl FnMut(&T) -> Result<bool, E>,
) -> Result<bool, E> {
    for symbol in symbols {
        let symbol = symbol?;
        if !alphanumeric(&symbol)? && !allowed(&symbol)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn column_name(columns: &[String], index: usize) -> String {
    let value = column_name_with(
        || Ok::<_, std::convert::Infallible>(index < columns.len()),
        || Ok(columns[index].clone()),
        |value| Ok(!value.is_empty()),
        || Ok(format!("col_{index}")),
    );
    match value {
        Ok(value) => value,
        Err(impossible) => match impossible {},
    }
}

pub fn column_name_with<T, E>(
    present: impl FnOnce() -> Result<bool, E>,
    mut get: impl FnMut() -> Result<T, E>,
    truthy: impl FnOnce(&T) -> Result<bool, E>,
    fallback: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    if present()? && truthy(&get()?)? {
        get()
    } else {
        fallback()
    }
}

pub fn visit_cells<V, C, E>(
    values: impl IntoIterator<Item = Result<V, E>>,
    make: impl FnMut(usize, V) -> Result<C, E>,
    boundary: &mut (impl FnMut() -> Result<(), E> + ?Sized),
) -> Result<Vec<C>, E> {
    let mut cells = Vec::new();
    visit_cells_into(values, &mut cells, make, boundary)?;
    Ok(cells)
}

pub fn visit_cells_into<V, C, E>(
    values: impl IntoIterator<Item = Result<V, E>>,
    cells: &mut Vec<C>,
    mut make: impl FnMut(usize, V) -> Result<C, E>,
    boundary: &mut (impl FnMut() -> Result<(), E> + ?Sized),
) -> Result<(), E> {
    for (index, value) in values.into_iter().enumerate() {
        boundary()?;
        cells.push(make(index, value?)?);
    }
    Ok(())
}

/// Name is read at each source expression, preserving custom Python cell
/// descriptors and hash/equality hooks. Literal suffix collisions overwrite
/// the existing key while retaining its first insertion position.
pub trait Projection<C> {
    type Name;
    type Value;
    type Error;
    fn name(&mut self, cell: &C) -> Result<Self::Name, Self::Error>;
    fn count(&mut self, name: Self::Name) -> Result<usize, Self::Error>;
    fn store_count(&mut self, name: Self::Name, count: usize) -> Result<(), Self::Error>;
    fn suffix(&mut self, name: Self::Name, count: usize) -> Result<Self::Name, Self::Error>;
    fn value(&mut self, cell: C) -> Result<Self::Value, Self::Error>;
    fn insert(&mut self, name: Self::Name, value: Self::Value) -> Result<(), Self::Error>;
}

pub fn project_cells<C, P: Projection<C>>(
    cells: impl IntoIterator<Item = Result<C, P::Error>>,
    projection: &mut P,
    boundary: &mut impl FnMut() -> Result<(), P::Error>,
) -> Result<(), P::Error> {
    for cell in cells {
        boundary()?;
        let cell = cell?;
        let name = projection.name(&cell)?;
        let count = projection.count(name)?;
        let name = projection.name(&cell)?;
        projection.store_count(name, count + 1)?;
        let name = projection.name(&cell)?;
        let key = if count == 0 {
            name
        } else {
            projection.suffix(name, count + 1)?
        };
        let value = projection.value(cell)?;
        projection.insert(key, value)?;
    }
    Ok(())
}

pub fn visit_tables<T, R, E>(
    tables: impl IntoIterator<Item = Result<T, E>>,
    parse: impl FnMut(T, usize) -> Result<R, E>,
    boundary: &mut impl FnMut() -> Result<(), E>,
) -> Result<Vec<R>, E> {
    let mut results = Vec::new();
    visit_tables_into(tables, &mut results, parse, boundary)?;
    Ok(results)
}

pub fn visit_tables_into<T, R, E>(
    tables: impl IntoIterator<Item = Result<T, E>>,
    results: &mut Vec<R>,
    mut parse: impl FnMut(T, usize) -> Result<R, E>,
    boundary: &mut impl FnMut() -> Result<(), E>,
) -> Result<(), E> {
    for (index, table) in tables.into_iter().enumerate() {
        boundary()?;
        results.push(parse(table?, index)?);
    }
    Ok(())
}
