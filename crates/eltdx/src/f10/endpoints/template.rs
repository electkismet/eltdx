//! Fixed request definitions shared by checked query types and legacy bindings.
use crate::f10::{AshareCode, BondCode, CallQuery, FuturesCode, HongKongCode,
    IndustryCode, JsonValue, ThirdBoardCode, UsTicker};

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum SimpleArgument {
    Literal(&'static str), Integer(i64), Code, Section, Market,
    Input(&'static str), MappedSection(SectionMap),
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum SectionMap { UsIndicator, HkIndicator, HkStatement }

impl SectionMap {
    pub fn python_attribute(self) -> &'static str {
        match self { Self::UsIndicator => "_MG_CWZB_SECTIONS",
            Self::HkIndicator => "_HK_CWZB_SECTIONS", Self::HkStatement => "_HK_CWFX_SECTIONS" }
    }
    pub fn value(self, section: &str) -> Option<&'static str> {
        let values: &[(&str, &str)] = match self {
            Self::UsIndicator => &[("jbzb", "001"), ("ylnl", "002"), ("cznl", "003"), ("yynl", "004"), ("ccznl", "005")],
            Self::HkIndicator => &[("jbzb", "1"), ("ylnl", "2"), ("cznl", "3"), ("yynl", "4"), ("ccznl", "5")],
            Self::HkStatement => &[("zhsyb", "1"), ("zcfzb", "2"), ("xjllb", "3")],
        };
        values.iter().find_map(|&(name, value)| (name == section).then_some(value))
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct SimpleEndpoint {
    pub method: &'static str,
    pub entry: &'static str,
    pub arguments: &'static [SimpleArgument],
    /// Python code helper, or None for unconverted industry/futures symbols.
    pub code_helper: Option<&'static str>,
    pub valid_sections: Option<&'static [&'static str]>,
}

pub(super) trait TemplateIdentity {
    fn number(&self) -> &str;
    fn market(&self) -> Option<u16> { None }
    fn python_helper(method: &str) -> Option<&'static str>;
}
macro_rules! identity {
    ($ty:ty, $helper:expr) => {
        impl TemplateIdentity for $ty {
            fn number(&self) -> &str { self.as_str() }
            fn python_helper(_: &str) -> Option<&'static str> { $helper }
        }
    };
}
identity!(AshareCode, Some("_code6"));
identity!(ThirdBoardCode, Some("_code6"));
identity!(UsTicker, Some("_us_code"));
identity!(HongKongCode, Some("_hk_code"));
identity!(IndustryCode, None);
identity!(FuturesCode, None);
impl TemplateIdentity for BondCode {
    fn number(&self) -> &str { self.as_str() }
    fn market(&self) -> Option<u16> { Some(self.market().get()) }
    fn python_helper(method: &str) -> Option<&'static str> {
        Some(if method.starts_with("bond_") && method != "bond_bulletin" || method.starts_with("gzhg_") {
            "_code6"
        } else { "_bond_code" })
    }
}

pub(super) fn definition<I: TemplateIdentity>(method: &'static str, entry: &'static str,
    arguments: &'static [SimpleArgument]) -> SimpleEndpoint
{
    SimpleEndpoint { method, entry, arguments, code_helper: I::python_helper(method),
        valid_sections: crate::f10::values::closed_sections(method) }
}

impl SimpleEndpoint {
    pub fn compose<T, E>(&self, mut value: impl FnMut(SimpleArgument) -> Result<T, E>) -> Result<Vec<T>, E> {
        self.arguments.iter().map(|&argument| value(argument)).collect()
    }
}

pub(super) fn native<I: TemplateIdentity>(definition: SimpleEndpoint, code: &I,
    section: &str) -> crate::Result<CallQuery>
{
    let values = definition.compose(|argument| Ok::<JsonValue, crate::Error>(match argument {
        SimpleArgument::Literal(value) => JsonValue::from(value),
        SimpleArgument::Integer(value) => JsonValue::from(value),
        SimpleArgument::Code => JsonValue::from(code.number()),
        SimpleArgument::Section => JsonValue::from(section),
        SimpleArgument::Market => JsonValue::from(format!("{}", code.market().ok_or_else(||
            crate::Error::new(crate::ErrorKind::Internal, "F10 template requires market"))?)),
        SimpleArgument::MappedSection(map) => JsonValue::from(map.value(section).ok_or_else(||
            crate::Error::new(crate::ErrorKind::InvalidArgument, "unknown F10 mapped section"))?),
        SimpleArgument::Input(_) => return Err(crate::Error::new(crate::ErrorKind::Internal,
            "fixed F10 query requires a named input")),
    }))?;
    super::params(definition.entry, values)
}

/// The predicate preserves language-specific equality (including its errors).
/// Python keeps arbitrary paging objects; Rust supplies checked numeric values.
#[doc(hidden)]
pub fn page_template<E>(method: &str, mut equals: impl FnMut(&str) -> Result<bool, E>) -> Result<Option<SimpleEndpoint>, E> {
    use SimpleArgument::{Code, Input, Integer, Literal, Section};
    let (method, entry, arguments): (&'static str, &'static str, &'static [SimpleArgument]) = match method {
        "mg_capital" => ("mg_capital", "CWServ.skef10_mg_gbjg", if equals("001")? {
            &[Literal("001"), Code, Integer(1)]
        } else { &[Literal("002"), Code, Input("page")] }),
        "mg_shareholders" => ("mg_shareholders", "CWServ.skef10_mg_gbgd", if equals("006")? {
            &[Literal("006"), Code, Integer(1), Integer(10)]
        } else { &[Section, Code, Input("page"), Input("page_size")] }),
        "mg_market" => ("mg_market", "CWServ.skef10_mg_schq", if equals("001")? {
            &[Literal("001"), Code, Input("page"), Input("page_size")]
        } else { &[Literal("003"), Code, Integer(0), Integer(0)] }),
        _ => return Ok(None),
    };
    Ok(Some(definition::<UsTicker>(method, entry, arguments)))
}

pub(super) fn native_page(method: &str, code: &UsTicker, section: &str, page: u32, page_size: u32) -> crate::Result<CallQuery> {
    let definition = page_template(method, |value| Ok::<_, crate::Error>(section == value))?
        .ok_or_else(|| crate::Error::new(crate::ErrorKind::Internal, "unknown F10 page template"))?;
    let values = definition.compose(|argument| Ok::<_, crate::Error>(match argument {
        SimpleArgument::Literal(value) => JsonValue::from(value),
        SimpleArgument::Integer(value) => JsonValue::from(value),
        SimpleArgument::Code => JsonValue::from(code.as_str()),
        SimpleArgument::Section => JsonValue::from(section),
        SimpleArgument::Input("page") => JsonValue::from(u64::from(page)),
        SimpleArgument::Input("page_size") => JsonValue::from(u64::from(page_size)),
        _ => return Err(crate::Error::new(crate::ErrorKind::Internal, "unsupported F10 page argument")),
    }))?;
    super::params(definition.entry, values)
}
