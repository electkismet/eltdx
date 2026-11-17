//! Fixed request definitions shared by checked query types and legacy bindings.
use crate::f10::{
    AshareCode, BondCode, CallQuery, FuturesCode, HongKongCode, IndustryCode, JsonValue,
    ThirdBoardCode, UsTicker,
};

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum SimpleArgument {
    Literal(&'static str),
    Integer(i64),
    Code,
    Section,
    Market,
    Input(&'static str),
    MappedSection(SectionMap),
    DateInput(&'static str),
    SectionChoice(&'static str, &'static str, &'static str),
    TextInput(&'static str),
    IntTextInput(&'static str),
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum SectionMap {
    UsIndicator,
    HkIndicator,
    HkStatement,
}

impl SectionMap {
    pub fn python_attribute(self) -> &'static str {
        match self {
            Self::UsIndicator => "_MG_CWZB_SECTIONS",
            Self::HkIndicator => "_HK_CWZB_SECTIONS",
            Self::HkStatement => "_HK_CWFX_SECTIONS",
        }
    }
    pub fn value(self, section: &str) -> Option<&'static str> {
        let values: &[(&str, &str)] = match self {
            Self::UsIndicator => &[
                ("jbzb", "001"),
                ("ylnl", "002"),
                ("cznl", "003"),
                ("yynl", "004"),
                ("ccznl", "005"),
            ],
            Self::HkIndicator => &[
                ("jbzb", "1"),
                ("ylnl", "2"),
                ("cznl", "3"),
                ("yynl", "4"),
                ("ccznl", "5"),
            ],
            Self::HkStatement => &[("zhsyb", "1"), ("zcfzb", "2"), ("xjllb", "3")],
        };
        values
            .iter()
            .find_map(|&(name, value)| (name == section).then_some(value))
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct SimpleEndpoint {
    pub method: &'static str,
    pub entry: &'static str,
    /// Concatenate the caller's section after `entry`, without normalization.
    pub append_section: bool,
    pub arguments: &'static [SimpleArgument],
    /// Python code helper, or None for unconverted industry/futures symbols.
    pub code_helper: Option<&'static str>,
    pub valid_sections: Option<&'static [&'static str]>,
}

pub(super) trait TemplateIdentity {
    fn number(&self) -> &str;
    fn market(&self) -> Option<u16> {
        None
    }
    fn python_helper(method: &str) -> Option<&'static str>;
}
macro_rules! identity {
    ($ty:ty, $helper:expr) => {
        impl TemplateIdentity for $ty {
            fn number(&self) -> &str {
                self.as_str()
            }
            fn python_helper(_: &str) -> Option<&'static str> {
                $helper
            }
        }
    };
}
identity!(AshareCode, Some("_code6"));
identity!(ThirdBoardCode, Some("_code6"));
identity!(UsTicker, Some("_us_code"));
identity!(HongKongCode, Some("_hk_code"));
identity!(IndustryCode, None);
identity!(FuturesCode, None);
impl TemplateIdentity for () {
    fn number(&self) -> &str {
        ""
    }
    fn python_helper(_: &str) -> Option<&'static str> {
        None
    }
}
impl TemplateIdentity for BondCode {
    fn number(&self) -> &str {
        self.as_str()
    }
    fn market(&self) -> Option<u16> {
        Some(self.market().get())
    }
    fn python_helper(method: &str) -> Option<&'static str> {
        Some(
            if method.starts_with("bond_") && method != "bond_bulletin"
                || method.starts_with("gzhg_")
            {
                "_code6"
            } else {
                "_bond_code"
            },
        )
    }
}

pub(super) fn definition<I: TemplateIdentity>(
    method: &'static str,
    entry: &'static str,
    arguments: &'static [SimpleArgument],
) -> SimpleEndpoint {
    SimpleEndpoint {
        method,
        entry,
        append_section: false,
        arguments,
        code_helper: I::python_helper(method),
        valid_sections: crate::f10::values::closed_sections(method),
    }
}

impl SimpleEndpoint {
    pub fn compose<T, E>(
        &self,
        mut value: impl FnMut(SimpleArgument) -> Result<T, E>,
    ) -> Result<Vec<T>, E> {
        self.arguments
            .iter()
            .map(|&argument| value(argument))
            .collect()
    }
}

pub(super) fn native<I: TemplateIdentity>(
    definition: SimpleEndpoint,
    code: &I,
    section: &str,
) -> crate::Result<CallQuery> {
    native_inputs(definition, code, section, |_| {
        Err(crate::Error::new(
            crate::ErrorKind::Internal,
            "fixed F10 query requires a named input",
        ))
    })
}

pub(super) fn native_inputs<I: TemplateIdentity>(
    definition: SimpleEndpoint,
    code: &I,
    section: &str,
    mut input: impl FnMut(&str) -> crate::Result<JsonValue>,
) -> crate::Result<CallQuery> {
    let values = definition.compose(|argument| {
        Ok::<JsonValue, crate::Error>(match argument {
            SimpleArgument::Literal(value) => JsonValue::from(value),
            SimpleArgument::Integer(value) => JsonValue::from(value),
            SimpleArgument::Code => JsonValue::from(code.number()),
            SimpleArgument::Section => JsonValue::from(section),
            SimpleArgument::Market => JsonValue::from(format!(
                "{}",
                code.market().ok_or_else(|| crate::Error::new(
                    crate::ErrorKind::Internal,
                    "F10 template requires market"
                ))?
            )),
            SimpleArgument::MappedSection(map) => {
                JsonValue::from(map.value(section).ok_or_else(|| {
                    crate::Error::new(
                        crate::ErrorKind::InvalidArgument,
                        "unknown F10 mapped section",
                    )
                })?)
            }
            SimpleArgument::Input(name)
            | SimpleArgument::DateInput(name)
            | SimpleArgument::TextInput(name)
            | SimpleArgument::IntTextInput(name) => input(name)?,
            SimpleArgument::SectionChoice(value, yes, no) => {
                JsonValue::from(if section == value { yes } else { no })
            }
        })
    })?;
    let entry = if definition.append_section {
        format!("{}{section}", definition.entry)
    } else {
        definition.entry.to_owned()
    };
    super::params(&entry, values)
}

/// The predicate preserves language-specific equality (including its errors).
/// Python keeps arbitrary paging objects; Rust supplies checked numeric values.
#[doc(hidden)]
pub fn page_template<E>(
    method: &str,
    mut equals: impl FnMut(&str) -> Result<bool, E>,
) -> Result<Option<SimpleEndpoint>, E> {
    use SimpleArgument::{Code, Input, Integer, Literal, Section, TextInput};
    let (method, entry, arguments): (&'static str, &'static str, &'static [SimpleArgument]) =
        match method {
            "mg_capital" => (
                "mg_capital",
                "CWServ.skef10_mg_gbjg",
                if equals("001")? {
                    &[Literal("001"), Code, Integer(1)]
                } else {
                    &[Literal("002"), Code, Input("page")]
                },
            ),
            "mg_shareholders" => (
                "mg_shareholders",
                "CWServ.skef10_mg_gbgd",
                if equals("006")? {
                    &[Literal("006"), Code, Integer(1), Integer(10)]
                } else {
                    &[Section, Code, Input("page"), Input("page_size")]
                },
            ),
            "mg_market" => (
                "mg_market",
                "CWServ.skef10_mg_schq",
                if equals("001")? {
                    &[Literal("001"), Code, Input("page"), Input("page_size")]
                } else {
                    &[Literal("003"), Code, Integer(0), Integer(0)]
                },
            ),
            "company_news" => (
                "company_news",
                "CWServ.tdxf10_gg_gszx",
                if equals("finance_review")? {
                    return Ok(Some(definition::<AshareCode>(
                        "company_news",
                        "CWServ.tdxf10_gg_cwfx_cbdp",
                        &[Code, TextInput("page")],
                    )));
                } else {
                    &[
                        Code,
                        Section,
                        Input("keyword"),
                        TextInput("rating"),
                        TextInput("page"),
                        TextInput("page_size"),
                    ]
                },
            ),
            _ => return Ok(None),
        };
    Ok(Some(if method == "company_news" {
        definition::<AshareCode>(method, entry, arguments)
    } else {
        definition::<UsTicker>(method, entry, arguments)
    }))
}

pub(super) fn native_page(
    method: &str,
    code: &UsTicker,
    section: &str,
    page: u32,
    page_size: u32,
) -> crate::Result<CallQuery> {
    let definition = page_template(method, |value| Ok::<_, crate::Error>(section == value))?
        .ok_or_else(|| {
            crate::Error::new(crate::ErrorKind::Internal, "unknown F10 page template")
        })?;
    native_inputs(definition, code, section, |name| {
        Ok(match name {
            "page" => JsonValue::from(u64::from(page)),
            "page_size" => JsonValue::from(u64::from(page_size)),
            _ => {
                return Err(crate::Error::new(
                    crate::ErrorKind::Internal,
                    "unsupported F10 page argument",
                ))
            }
        })
    })
}

#[doc(hidden)]
pub fn input_template(method: &str) -> Option<SimpleEndpoint> {
    use SimpleArgument::{
        Code, DateInput, Input, IntTextInput, Literal, Section, SectionChoice, TextInput,
    };
    Some(match method {
        "hk_pepb_band" => definition::<HongKongCode>(
            "hk_pepb_band",
            "CWServ.skef10_hk_sdfx_pepb_band",
            &[
                Literal("001"),
                Code,
                Input("sjfw"),
                Input("tab_value"),
                Input("hqzq"),
            ],
        ),
        "hk_valuation" => definition::<HongKongCode>(
            "hk_valuation",
            "CWServ.skef10_hk_sdfx_gzfx",
            &[Code, Input("item_id")],
        ),
        "hk_analyst_forecast" => definition::<HongKongCode>(
            "hk_analyst_forecast",
            "CWServ.skef10_hk_sdfx_fxysyfx",
            &[Code, DateInput("start"), DateInput("end")],
        ),
        "allotment_details" => definition::<AshareCode>(
            "allotment_details",
            "CWServ.tdxf10_gg_fhrz_zfhpmx",
            &[Literal("zfpg"), Code, Input("date")],
        ),
        "finance_report" => definition::<AshareCode>(
            "finance_report",
            "CWServ.tdxf10_gg_cwfx",
            &[Code, Input("report_type"), Literal("")],
        ),
        "finance_diagnosis" => definition::<AshareCode>(
            "finance_diagnosis",
            "CWServ.tdxf10_gg_cwzd",
            &[Section, Code, Input("scope")],
        ),
        "stock_score" => definition::<AshareCode>(
            "stock_score",
            "CWServ.tdxf10_gg_ggzp",
            &[Section, Code, Input("arg"), Literal("")],
        ),
        "governance" => definition::<AshareCode>(
            "governance",
            "CWServ.tdxf10_gg_zbyz",
            &[Section, Code, Input("arg")],
        ),
        "hy_stage_rise" => definition::<IndustryCode>(
            "hy_stage_rise",
            "CWServ.skef10_hy_jdzf",
            &[Code, Input("flag")],
        ),
        "hy_capital_stats" => definition::<IndustryCode>(
            "hy_capital_stats",
            "CWServ.skef10_hy_hysj_bxzjtj",
            &[Section, Code, SectionChoice("001", "n", "")],
        ),
        "qh_basis_spread" => definition::<FuturesCode>(
            "qh_basis_spread",
            "CWServ.qhf9_sp_jcjcfx",
            &[Section, Code, Input("period"), Literal(""), Literal("")],
        ),
        "qh_warehouse_receipt" => definition::<FuturesCode>(
            "qh_warehouse_receipt",
            "CWServ.qhf9_sp_ckrb_ckrb",
            &[Literal("1"), Code, DateInput("date")],
        ),
        "topic_compare" => definition::<AshareCode>(
            "topic_compare",
            "CWServ.tdxf10_gg_rdtc_gndb",
            &[Section, Code, TextInput("topic_id"), Input("sort_by")],
        ),
        "northbound_holding" => definition::<AshareCode>(
            "northbound_holding",
            "CWServ.tdxf10_gg_zlcc",
            &[
                Code,
                Section,
                Input("filter_value"),
                TextInput("page"),
                TextInput("page"),
                TextInput("page_size"),
            ],
        ),
        "detail" => definition::<()>(
            "detail",
            "CWServ.tdxf10_gg_idreq",
            &[Input("detail_type"), TextInput("record_id")],
        ),
        "market_news" => definition::<()>(
            "market_news",
            "CWServ.tdxi_zxxwsy",
            &[
                IntTextInput("tab"),
                TextInput("page"),
                TextInput("page_size"),
            ],
        ),
        "channel_news" => definition::<()>(
            "channel_news",
            "CWServ.tdxi_zxxwnews",
            &[
                TextInput("query1"),
                IntTextInput("channel"),
                TextInput("page"),
                TextInput("page_size"),
                Input("keyword"),
            ],
        ),
        _ => return None,
    })
}

pub(super) fn native_named<I: TemplateIdentity>(
    method: &str,
    code: &I,
    section: &str,
    input: impl FnMut(&str) -> crate::Result<JsonValue>,
) -> crate::Result<CallQuery> {
    let definition = input_template(method).ok_or_else(|| {
        crate::Error::new(crate::ErrorKind::Internal, "unknown F10 input template")
    })?;
    native_inputs(definition, code, section, input)
}
