use super::{JsonValue, RequestPayload};
use crate::types::ResultMeta;
use crate::{Error, ErrorKind, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cell { name: String, index: usize, value: JsonValue }
impl Cell {
    pub fn name(&self) -> &str { &self.name }
    pub fn index(&self) -> usize { self.index }
    pub fn value(&self) -> &JsonValue { &self.value }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row { cells: Vec<Cell> }
impl Row {
    pub fn cells(&self) -> &[Cell] { &self.cells }
    pub fn get_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Cell> + 'a {
        self.cells.iter().filter(move |cell| cell.name == name)
    }
    /// Python's suffix convention, including overwrite-on-name-collision.
    /// Lossless callers should inspect `cells()` or `get_all()` instead.
    pub fn python_projection(&self) -> Vec<(String, &JsonValue)> {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        let mut output: Vec<(String, &JsonValue)> = Vec::new();
        for cell in &self.cells {
            let count = if let Some((_, count)) = counts.iter_mut().find(|(name, _)| *name == cell.name) {
                *count += 1;
                *count
            } else {
                counts.push((&cell.name, 1));
                1
            };
            let key = if count == 1 { cell.name.clone() } else { format!("{}__{count}", cell.name) };
            if let Some((_, value)) = output.iter_mut().find(|(name, _)| *name == key) {
                *value = &cell.value;
            } else {
                output.push((key, &cell.value));
            }
        }
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultSet {
    key: Option<String>, columns: Vec<String>, rows: Vec<Row>, raw: Option<JsonValue>,
}
impl ResultSet {
    pub fn key(&self) -> Option<&str> { self.key.as_deref() }
    pub fn columns(&self) -> &[String] { &self.columns }
    pub fn rows(&self) -> &[Row] { &self.rows }
    pub fn raw(&self) -> Option<&JsonValue> { self.raw.as_ref() }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Response {
    entry: String, request: RequestPayload, error_code: Option<i64>,
    result_sets: Vec<ResultSet>, meta: ResultMeta,
}
impl Response {
    pub fn entry(&self) -> &str { &self.entry }
    pub fn request(&self) -> &RequestPayload { &self.request }
    pub fn error_code(&self) -> Option<i64> { self.error_code }
    pub fn result_sets(&self) -> &[ResultSet] { &self.result_sets }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub fn ok(&self) -> bool { self.error_code.is_none_or(|code| code == 0) }
    pub fn first_table(&self) -> Option<&ResultSet> { self.result_sets.first() }

    pub(crate) fn from_json(entry: String, request: RequestPayload, value: JsonValue,
        retain: bool, meta: ResultMeta, boundary: &mut dyn FnMut() -> Result<()>) -> Result<Self> {
        boundary()?;
        let JsonValue::Object(mut object) = value else {
            return Err(protocol("TQLEX returned non-object JSON"));
        };
        let error_code = match last(&object, "ErrorCode") {
            None | Some(JsonValue::Null) => None,
            Some(JsonValue::Number(number)) => Some(number.as_i64().ok_or_else(|| protocol("TQLEX ErrorCode is outside i64"))?),
            Some(_) => return Err(protocol("TQLEX ErrorCode must be an integer")),
        };
        // Move the authoritative final occurrence; duplicate raw fields remain
        // in the retained HTTP payload without building a second complete tree.
        let result_sets = object.iter().rposition(|(key, _)| key == "ResultSets")
            .map(|position| std::mem::replace(&mut object[position].1, JsonValue::Null));
        let values = match result_sets {
            None | Some(JsonValue::Null) => Vec::new(),
            Some(JsonValue::Array(values)) => values,
            Some(_) => return Err(protocol("TQLEX ResultSets must be an array")),
        };
        let mut result_sets = Vec::new();
        result_sets.try_reserve(values.len()).map_err(allocation)?;
        for value in values {
            boundary()?;
            result_sets.push(parse_table(value, retain, boundary)?);
        }
        boundary()?;
        Ok(Self { entry, request, error_code, result_sets, meta })
    }
}

fn protocol(message: &str) -> Error { Error::new(ErrorKind::Protocol, message) }
fn allocation(error: std::collections::TryReserveError) -> Error {
    Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 response storage").with_source(error)
}
fn last<'a>(object: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    object.iter().rev().find(|(name, _)| name == key).map(|(_, value)| value)
}
fn take(object: &mut [(String, JsonValue)], key: &str) -> Option<JsonValue> {
    object.iter().rposition(|(name, _)| name == key)
        .map(|position| std::mem::replace(&mut object[position].1, JsonValue::Null))
}
fn string(value: &JsonValue) -> Result<String> {
    match value {
        JsonValue::String(text) => Ok(text.clone()),
        JsonValue::Number(number) => Ok(number.lexeme().to_owned()),
        JsonValue::Bool(value) => Ok(if *value { "True" } else { "False" }.to_owned()),
        JsonValue::Null => Ok("None".to_owned()),
        _ => Err(protocol("TQLEX column name must be scalar")),
    }
}
fn array(value: Option<JsonValue>, label: &str) -> Result<Vec<JsonValue>> {
    match value {
        None | Some(JsonValue::Null) => Ok(Vec::new()),
        Some(JsonValue::Array(values)) => Ok(values),
        _ => Err(protocol(label)),
    }
}
fn parse_table(value: JsonValue, retain: bool, boundary: &mut dyn FnMut() -> Result<()>) -> Result<ResultSet> {
    let JsonValue::Object(mut object) = value else { return Err(protocol("TQLEX ResultSet must be an object")); };
    let raw = retain.then(|| JsonValue::Object(object.clone()));
    let key = match take(&mut object, "ResultSetKey") {
        None | Some(JsonValue::Null) => None,
        Some(JsonValue::String(value)) => Some(value),
        _ => return Err(protocol("TQLEX ResultSetKey must be a string")),
    };
    let names = if let Some(value) = take(&mut object, "ColName") {
        array(Some(value), "TQLEX ColName must be an array")?
    } else {
        array(take(&mut object, "ColDes"), "TQLEX ColDes must be an array")?
            .into_iter().map(|value| match value {
                JsonValue::Object(fields) => last(&fields, "Name").cloned().unwrap_or_else(|| JsonValue::String(String::new())),
                value => value,
            }).collect()
    };
    let mut columns = Vec::new();
    columns.try_reserve(names.len()).map_err(allocation)?;
    for value in &names { boundary()?; columns.push(string(value)?); }
    let content = array(take(&mut object, "Content"), "TQLEX ResultSet Content must be an array")?;
    let mut rows = Vec::new();
    rows.try_reserve(content.len()).map_err(allocation)?;
    for value in content {
        boundary()?;
        let values = if let JsonValue::Array(values) = value { values } else { vec![value] };
        let mut cells = Vec::new();
        cells.try_reserve(values.len()).map_err(allocation)?;
        for (index, value) in values.into_iter().enumerate() {
            boundary()?;
            let name = columns.get(index).filter(|name| !name.is_empty()).cloned().unwrap_or_else(|| format!("col_{index}"));
            cells.push(Cell { name, index, value });
        }
        rows.push(Row { cells });
    }
    Ok(ResultSet { key, columns, rows, raw })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(json: &[u8], retain: bool) -> Result<Response> {
        Response::from_json("Example.entry".to_owned(), RequestPayload::Params(Vec::new()),
            JsonValue::parse(json)?, retain, ResultMeta::default(), &mut || Ok(()))
    }
    #[test]
    fn duplicate_columns_and_suffix_collisions_keep_authoritative_cells() -> Result<()> {
        let response = response(br#"{"ResultSets":[{"ColName":["x","x","x__2",""],"Content":[[1,2,3,null,5]]}]}"#, false)?;
        let row = &response.first_table().ok_or_else(|| protocol("missing table"))?.rows()[0];
        assert_eq!(row.cells().len(), 5);
        assert_eq!(row.get_all("x").count(), 2);
        let projected = row.python_projection();
        assert_eq!(projected[1].0, "x__2");
        assert_eq!(projected[1].1.as_number().and_then(|n| n.as_i64()), Some(3));
        assert_eq!(row.cells()[3].name(), "col_3");
        assert_eq!(row.cells()[4].name(), "col_4");
        Ok(())
    }
    #[test]
    fn nonzero_business_code_is_response_and_tables_remain_ordered() -> Result<()> {
        let response = response(br#"{"ErrorCode":17,"ResultSets":[{"ResultSetKey":"same","Content":[null]},{"ResultSetKey":"same","Content":[]},{}]}"#, true)?;
        assert!(!response.ok());
        assert_eq!(response.error_code(), Some(17));
        assert_eq!(response.result_sets().len(), 3);
        assert_eq!(response.result_sets()[2].key(), None);
        assert!(response.result_sets()[0].raw().is_some());
        assert_eq!(response.result_sets()[0].rows()[0].cells()[0].value(), &JsonValue::Null);
        Ok(())
    }
}
