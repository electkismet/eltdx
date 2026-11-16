//! Shared industry/futures request definitions; language adapters own lookup
//! policy (dictionary lookup versus ordered comparisons) and date conversion.
use super::{SimpleArgument, SimpleEndpoint};
use SimpleArgument::{Code, DateInput, Input, Literal, SectionChoice};

fn definition(method: &'static str, entry: &'static str, arguments: &'static [SimpleArgument]) -> SimpleEndpoint {
    SimpleEndpoint { method, entry, append_section: false, arguments, code_helper: None, valid_sections: None }
}

#[doc(hidden)]
pub fn sector_choices(method: &str) -> &'static [&'static str] {
    match method {
        "hy_summary" => &["hqtj", "hyzq", "hygy", "zszfb"],
        "hy_members_rank" => &["cwzkpm", "gsgmpm", "gzsppm", "scbxpm", "zysrpm"],
        "qh_position_report" => &["cjcc", "cjjg", "jcgc", "ccjj", "ykfx"],
        "qh_variety_info" => &["pzjj", "hyjj", "xgzq"],
        _ => &[],
    }
}

#[doc(hidden)]
pub fn sector_selection(method: &str, section: &str) -> Option<SimpleEndpoint> {
    Some(match (method, section) {
        ("hy_summary", "hqtj") => definition("hy_summary", "CWServ.skef10_hy_hqtj", &[Code]),
        ("hy_summary", "hyzq") => definition("hy_summary", "CWServ.skef10_hy_hyzq", &[Code]),
        ("hy_summary", "hygy") => definition("hy_summary", "CWServ.skef10_hy_hygy", &[Code]),
        ("hy_summary", "zszfb") => definition("hy_summary", "CWServ.skef10_hy_zszfb", &[Code]),
        ("hy_members_rank", "cwzkpm") => definition("hy_members_rank", "CWServ.skef10_hy_hydw_cwzkpm", &[Literal("002"), Code, Literal(""), Literal("e"), Literal("")]),
        ("hy_members_rank", "gsgmpm") => definition("hy_members_rank", "CWServ.skef10_hy_hydw_gsgmpm", &[Code]),
        ("hy_members_rank", "gzsppm") => definition("hy_members_rank", "CWServ.skef10_hy_hydw_gzsppm", &[Literal("01"), Code, Literal("o")]),
        ("hy_members_rank", "scbxpm") => definition("hy_members_rank", "CWServ.skef10_hy_hydw_scbxpm", &[Literal("scpm"), Literal(""), Code]),
        ("hy_members_rank", "zysrpm") => definition("hy_members_rank", "CWServ.skef10_hy_hydw_zysrpm", &[Literal("001"), Code, Literal("")]),
        ("qh_variety_info", "pzjj") => definition("qh_variety_info", "CWServ.qhf9_sp_pzzl_pzjj", &[Code, Literal("1")]),
        ("qh_variety_info", "hyjj") => definition("qh_variety_info", "CWServ.qhf9_sp_pzzl_hyjj", &[Code]),
        ("qh_variety_info", "xgzq") => definition("qh_variety_info", "CWServ.qhf9_sp_pzzl_xgzq", &[Code]),
        ("qh_position_report", "cjcc") => definition("qh_position_report", "cjcc", position_arguments("cjcc")),
        ("qh_position_report", "cjjg") => definition("qh_position_report", "cjjg", position_arguments("cjjg")),
        ("qh_position_report", "jcgc") => definition("qh_position_report", "jcgc", position_arguments("jcgc")),
        ("qh_position_report", "ccjj") => definition("qh_position_report", "ccjj", position_arguments("ccjj")),
        ("qh_position_report", "ykfx") => definition("qh_position_report", "ykfx", position_arguments("ykfx")),
        _ => return None,
    })
}

#[doc(hidden)]
pub fn position_arguments(section: &str) -> &'static [SimpleArgument] {
    match section {
        "cjcc" => &[Literal("1"), Code, Input("date")],
        "cjjg" => &[Literal("1"), Code, Input("date"), Literal("")],
        _ => &[Literal("1"), Code, Input("date"), Input("end_date"), Literal("")],
    }
}

#[doc(hidden)]
pub fn position_prefix(index: bool) -> &'static str {
    if index { "CWServ.qhf9_gz_ccbg_" } else { "CWServ.qhf9_sp_ccbg_" }
}

#[doc(hidden)]
pub fn sector_dynamic(method: &str) -> Option<SimpleEndpoint> {
    let mut selected = match method {
        "qh_index_info" => definition("qh_index_info", "CWServ.qhf9_gz_zszl_", &[SectionChoice("zsgk", "1", "2"), Code]),
        "qh_index_valuation" => definition("qh_index_valuation", "CWServ.qhf9_gz_zsgz_", &[Literal("1"), Code, DateInput("start"), DateInput("end")]),
        "qh_weights" => definition("qh_weights", "CWServ.qhf9_gz_cfg_", &[Literal("1"), Code, Input("date")]),
        _ => return None,
    };
    selected.append_section = true;
    Some(selected)
}

#[doc(hidden)]
pub fn sector_weights_without_date() -> SimpleEndpoint {
    definition("qh_weights", "CWServ.qhf9_gz_cfg_cfghyfb", &[Literal("1"), Code])
}

pub(super) fn selected(method: &str, section: &str) -> crate::Result<SimpleEndpoint> {
    sector_selection(method, section).ok_or_else(||
        crate::Error::new(crate::ErrorKind::InvalidArgument, "unknown F10 sector section"))
}
