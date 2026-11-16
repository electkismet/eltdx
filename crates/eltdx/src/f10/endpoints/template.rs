//! Fixed request definitions shared by checked query types and legacy bindings.
use crate::f10::{AshareCode, BondCode, CallQuery, FuturesCode, HongKongCode,
    IndustryCode, JsonValue, ThirdBoardCode, UsTicker};

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum SimpleArgument { Literal(&'static str), Code, Section, Market }

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
        SimpleArgument::Code => JsonValue::from(code.number()),
        SimpleArgument::Section => JsonValue::from(section),
        SimpleArgument::Market => JsonValue::from(format!("{}", code.market().ok_or_else(||
            crate::Error::new(crate::ErrorKind::Internal, "F10 template requires market"))?)),
    }))?;
    super::params(definition.entry, values)
}
