use super::{JsonValue, OrderedJsonObject};
use crate::{Error, ErrorKind, Result};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct EntryName(String);
impl EntryName {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(Error::new(ErrorKind::InvalidArgument, "F10 Entry must be nonempty without control characters"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

/// The enum prevents simultaneous body and Params inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestPayload { Body(JsonValue), Params(Vec<JsonValue>) }
impl RequestPayload {
    pub(crate) fn retained(&self, boundary: &mut dyn FnMut() -> Result<()>) -> Result<Self> {
        match self {
            Self::Body(value) => Ok(Self::Body(super::json::clone_value(value, boundary)?)),
            Self::Params(values) => {
                let mut output = Vec::new();
                for value in values {
                    boundary()?;
                    output.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot retain F10 parameters").with_source(error))?;
                    output.push(super::json::clone_value(value, boundary)?);
                }
                Ok(Self::Params(output))
            }
        }
    }
    pub(crate) fn encoded(&self, maximum: usize, boundary: &mut dyn FnMut() -> Result<()>) -> Result<Vec<u8>> {
        match self {
            Self::Body(value) => super::json::encode(value, maximum, boundary),
            Self::Params(values) => super::json::encode_params(values, maximum, boundary),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallQuery { entry: EntryName, payload: RequestPayload }
impl CallQuery {
    pub fn new(entry: EntryName, payload: RequestPayload) -> Self { Self { entry, payload } }
    pub fn entry(&self) -> &EntryName { &self.entry }
    pub fn payload(&self) -> &RequestPayload { &self.payload }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamsQuery { call: CallQuery }
impl ParamsQuery {
    pub fn new(entry: EntryName, params: Vec<JsonValue>) -> Self { Self { call: CallQuery::new(entry, RequestPayload::Params(params)) } }
    pub fn entry(&self) -> &EntryName { self.call.entry() }
    pub fn params(&self) -> &[JsonValue] {
        match self.call.payload() { RequestPayload::Params(values) => values, RequestPayload::Body(_) => unreachable!("ParamsQuery stores Params only") }
    }
    pub fn into_call(self) -> CallQuery { self.call }
    pub(crate) fn call(&self) -> &CallQuery { &self.call }
}

/// Typed query extras preserve order, reject duplicate names, and cannot replace
/// endpoint identity/pagination fields. Generic `CallQuery` remains unrestricted.
pub(crate) fn merge_extra(target: &mut OrderedJsonObject, extra: &[(String, JsonValue)], protected: &[&str]) -> Result<()> {
    for (index, (name, _)) in extra.iter().enumerate() {
        if protected.contains(&name.as_str()) || target.iter().any(|(key, _)| key == name)
            || extra[..index].iter().any(|(key, _)| key == name) {
            return Err(Error::new(ErrorKind::InvalidArgument, "duplicate or reserved F10 extra field").with_context("field", name));
        }
    }
    target.try_reserve(extra.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 extra fields").with_source(error))?;
    target.extend(extra.iter().cloned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_have_one_wrapper_and_compact_unicode_body() -> Result<()> {
        let payload = RequestPayload::Params(vec![JsonValue::from("中文"), JsonValue::from(7_i64)]);
        assert_eq!(payload.encoded(128, &mut || Ok(()))?, "{\"Params\":[\"中文\",7]}".as_bytes());
        assert!(RequestPayload::Params(Vec::new()).encoded(2, &mut || Ok(())).is_err());
        Ok(())
    }

    #[test]
    fn typed_extra_rejection_leaves_original_payload_untouched() {
        let original = vec![("Code".to_owned(), JsonValue::from("sample"))];
        let mut values = original.clone();
        let extra = vec![("filter".to_owned(), JsonValue::Null), ("filter".to_owned(), JsonValue::Null)];
        assert!(merge_extra(&mut values, &extra, &["Code"]).is_err());
        assert_eq!(values, original);
        assert!(merge_extra(&mut values, &[("Code".to_owned(), JsonValue::Null)], &["Code"]).is_err());
        assert_eq!(values, original);
    }
}
