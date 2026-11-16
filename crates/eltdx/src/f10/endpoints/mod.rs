//! Concrete endpoint queries share request execution, never public timeout
//! restarts. Macros emit distinct public query types and named client methods.
use std::time::Instant;

use crate::operation::OperationContext;
use crate::types::{Diagnostic, EnrichmentStatus};
use crate::{OperationOptions, Result, TimeoutPhase};
use super::{CallQuery, EntryName, F10Client, JsonValue, RequestPayload, Response};

pub(crate) fn params(entry: &str, values: Vec<JsonValue>) -> Result<CallQuery> {
    Ok(CallQuery::new(EntryName::new(entry)?, RequestPayload::Params(values)))
}
pub(crate) fn inferred_bond(response: Response, inferred: bool) -> Response {
    if inferred { response.with_diagnostic(Diagnostic::new("bond_identity", "query", EnrichmentStatus::NotRequested, None,
        Some("Unprefixed bond code uses the historical SH market default; market is inferred.".to_owned()))) } else { response }
}
impl F10Client {
    pub(crate) fn endpoint_call(&self, query: &CallQuery, context: &OperationContext) -> Result<Response> {
        context.check_boundary(TimeoutPhase::Send)?;
        self.call_in_context(query, context)
    }
    fn endpoint(&self, name: &'static str, entry: Instant, query: impl FnOnce() -> Result<CallQuery>, options: &OperationOptions) -> Result<Response> {
        let context = self.context(entry, name, options)?;
        let result = self.endpoint_call(&query()?, &context)?;
        context.finish()?;
        Ok(result)
    }
}

macro_rules! code_endpoint {
    ($query:ident, $method:ident, $code:ty, $input:ident, $entry:literal, wire [$($value:expr),* $(,)?]) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code } }
            pub fn code(&self) -> &$code { &self.code }
            #[doc(hidden)]
            pub fn simple_template() -> super::SimpleEndpoint {
                super::template::definition::<$code>(stringify!($method), $entry, &[$($value),*])
            }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                super::template::native(Self::simple_template(), &self.code, "")
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                let result = self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)?;
                Ok(super::inferred_bond(result, crate::f10::values::IdentityDiagnostic::inferred_bond_market(query.code())))
            }
        }
    };
    ($query:ident, $method:ident, $code:ty, $input:ident, $entry:expr, [$($value:expr),* $(,)?]) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code } }
            pub fn code(&self) -> &$code { &self.code }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                let $input = &self.code;
                super::params($entry, vec![$($value),*])
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                let result = self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)?;
                Ok(super::inferred_bond(result, crate::f10::values::IdentityDiagnostic::inferred_bond_market(query.code())))
            }
        }
    };
}
macro_rules! section_endpoint {
    ($query:ident, $method:ident, $code:ty, $default:literal, sector) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code, section: crate::f10::SectionCode }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code, section: crate::f10::SectionCode::default_for(stringify!($method), $default) } }
            pub fn code(&self) -> &$code { &self.code }
            pub fn section(&self) -> &crate::f10::SectionCode { &self.section }
            pub fn with_section(mut self, section: crate::f10::SectionCode) -> crate::Result<Self> {
                section.checked_for(stringify!($method))?; self.section = section; Ok(self)
            }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                let definition = super::sector_templates::selected(stringify!($method), self.section.as_str())?;
                super::template::native(definition, &self.code, self.section.as_str())
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)
            }
        }
    };
    ($query:ident, $method:ident, $code:ty, $default:literal, dynamic_sector) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code, section: crate::f10::SectionCode }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code, section: crate::f10::SectionCode::default_for(stringify!($method), $default) } }
            pub fn code(&self) -> &$code { &self.code }
            pub fn section(&self) -> &crate::f10::SectionCode { &self.section }
            pub fn with_section(mut self, section: crate::f10::SectionCode) -> crate::Result<Self> {
                section.checked_for(stringify!($method))?; self.section = section; Ok(self)
            }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                let definition = super::sector_templates::sector_dynamic(stringify!($method)).expect("built-in F10 sector");
                super::template::native(definition, &self.code, self.section.as_str())
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)
            }
        }
    };
    ($query:ident, $method:ident, $code:ty, $default:literal, shared) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code, section: crate::f10::SectionCode }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code, section: crate::f10::SectionCode::default_for(stringify!($method), $default) } }
            pub fn code(&self) -> &$code { &self.code }
            pub fn section(&self) -> &crate::f10::SectionCode { &self.section }
            pub fn with_section(mut self, section: crate::f10::SectionCode) -> crate::Result<Self> {
                section.checked_for(stringify!($method))?; self.section = section; Ok(self)
            }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                super::template::native_named(stringify!($method), &self.code, self.section.as_str(), |_| unreachable!("no named input"))
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)
            }
        }
    };
    ($query:ident, $method:ident, $code:ty, $default:literal, $input:ident, $section:ident, $entry:literal, wire [$($value:expr),* $(,)?]) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code, section: crate::f10::SectionCode }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code, section: crate::f10::SectionCode::default_for(stringify!($method), $default) } }
            pub fn code(&self) -> &$code { &self.code }
            pub fn section(&self) -> &crate::f10::SectionCode { &self.section }
            pub fn with_section(mut self, section: crate::f10::SectionCode) -> crate::Result<Self> {
                section.checked_for(stringify!($method))?; self.section = section; Ok(self)
            }
            #[doc(hidden)]
            pub fn simple_template() -> super::SimpleEndpoint {
                super::template::definition::<$code>(stringify!($method), $entry, &[$($value),*])
            }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                super::template::native(Self::simple_template(), &self.code, self.section.as_str())
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                let result = self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)?;
                Ok(super::inferred_bond(result, crate::f10::values::IdentityDiagnostic::inferred_bond_market(query.code())))
            }
        }
    };
    ($query:ident, $method:ident, $code:ty, $default:literal, $input:ident, $section:ident, $entry:expr, [$($value:expr),* $(,)?]) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $query { code: $code, section: crate::f10::SectionCode }
        impl $query {
            pub fn new(code: $code) -> Self { Self { code, section: crate::f10::SectionCode::default_for(stringify!($method), $default) } }
            pub fn code(&self) -> &$code { &self.code }
            pub fn section(&self) -> &crate::f10::SectionCode { &self.section }
            pub fn with_section(mut self, section: crate::f10::SectionCode) -> crate::Result<Self> {
                section.checked_for(stringify!($method))?; self.section = section; Ok(self)
            }
            pub fn to_call(&self) -> crate::Result<super::CallQuery> {
                let $input = &self.code;
                let $section = self.section.as_str();
                super::params($entry, vec![$($value),*])
            }
        }
        impl super::F10Client {
            pub fn $method(&self, query: &$query, options: &crate::OperationOptions) -> crate::Result<super::Response> {
                let result = self.endpoint(concat!("f10.", stringify!($method)), std::time::Instant::now(), || query.to_call(), options)?;
                Ok(super::inferred_bond(result, crate::f10::values::IdentityDiagnostic::inferred_bond_market(query.code())))
            }
        }
    };
}

mod template;
pub use template::{input_template, page_template, SectionMap, SimpleArgument, SimpleEndpoint};
mod sector_templates;
pub use sector_templates::{position_arguments, position_prefix, sector_choices, sector_dynamic, sector_selection, sector_weights_without_date};
mod simple_registry;
pub use simple_registry::simple_endpoint;
mod international;
pub use international::*;
mod bonds;
pub use bonds::*;
mod sectors;
pub use sectors::*;
mod reports;
pub use reports::*;
mod news_queries;
pub use news_queries::*;
mod valuation;
pub use valuation::*;
mod market;
pub use market::*;
mod ladder;
pub use ladder::*;
