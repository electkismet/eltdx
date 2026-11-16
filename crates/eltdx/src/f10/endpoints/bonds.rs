//! Convertible, government, corporate, repo, ABS and exchangeable bonds.
//! Explicit bond market is retained, including overlapping numerical ranges.
use crate::f10::values::j;
use crate::f10::BondCode;

code_endpoint!(BondProfileQuery, bond_profile, BondCode, code, "CWServ.skef10_bond_kzz_zqgk", [j("sdcyrbgq"), j(code.as_str()), j("")]);
section_endpoint!(BondFinanceQuery, bond_finance, BondCode, "zcfzb", code, section, "CWServ.skef10_bond_kzz_cwfx", [j(section), j(code.as_str())]);
section_endpoint!(BondRatingQuery, bond_rating, BondCode, "pjzst", code, section, "CWServ.skef10_bond_kzz_zqpj", [j(section), j(code.as_str())]);
section_endpoint!(BondConversionQuery, bond_conversion, BondCode, "zgjg", code, section, "CWServ.skef10_bond_kzz_zg", [j(section), j(code.as_str())]);
section_endpoint!(BondRateQuery, bond_rate, BondCode, "zqll", code, section, "CWServ.skef10_bond_kzz_zqll", [j(section), j(code.as_str())]);
section_endpoint!(BondAnnouncementsQuery, bond_announcements, BondCode, "1", code, section, "CWServ.skef10_bond_kzz_zjzx", [j(code.as_str()), j(section)]);

section_endpoint!(GzProfileQuery, gz_profile, BondCode, "zqgk", code, section, "CWServ.skef10_bond_gz_zjgk", [j(section), j(code.as_str()), market(code)]);
section_endpoint!(GzInterestQuery, gz_interest, BondCode, "fxqk", code, section, "CWServ.skef10_bond_gz_fxqk", [j(section), j(code.as_str()), market(code)]);
section_endpoint!(GzMarketDataQuery, gz_market_data, BondCode, "dzjy", code, section, "CWServ.skef10_bond_gz_zjsj", [j(section), j(code.as_str()), market(code)]);
section_endpoint!(GzRatingQuery, gz_rating, BondCode, "dfzf", code, section, "CWServ.skef10_bond_gz_zjpj", [j(section), j(code.as_str()), market(code)]);
code_endpoint!(GzFinanceQuery, gz_finance, BondCode, code, "CWServ.skef10_bond_gz_cwfx", [j("t"), j(code.as_str()), market(code)]);

section_endpoint!(QyzProfileQuery, qyz_profile, BondCode, "zqzl", code, section, "CWServ.skef10_bond_qyz_zqgk", [j(section), j(code.as_str()), market(code)]);
section_endpoint!(QyzInterestQuery, qyz_interest, BondCode, "fxqk", code, section, "CWServ.skef10_bond_qyz_xjlfx", [j(section), j(code.as_str()), market(code)]);
code_endpoint!(QyzGuaranteeQuery, qyz_guarantee, BondCode, code, "CWServ.skef10_bond_qyz_zqdb", [j("dbsm"), j(code.as_str()), market(code)]);
section_endpoint!(QyzRatingQuery, qyz_rating, BondCode, "zjpjzst", code, section, "CWServ.skef10_bond_qyz_zjpj", [j(section), j(code.as_str()), market(code)]);
code_endpoint!(QyzRateQuery, qyz_rate, BondCode, code, "CWServ.skef10_bond_qyz_zjlv", [j("zqlv"), j(code.as_str()), market(code)]);
code_endpoint!(QyzReviewQuery, qyz_review, BondCode, code, "CWServ.skef10_bond_qyz_gsps", [j(code.as_str()), market(code)]);

code_endpoint!(GzhgInfoQuery, gzhg_info, BondCode, code, "CWServ.skef10_bond_gzhg_zqxx", [j("zqgk"), j(code.as_str())]);
section_endpoint!(GzhgQuotesQuery, gzhg_quotes, BondCode, "bpzs", code, section, "CWServ.skef10_bond_gzhg_zqxx", [j(section), j(code.as_str())]);
code_endpoint!(ZczqProfileQuery, zczq_profile, BondCode, code, "CWServ.skef10_bond_zczqh_zqgk", [j(code.as_str()), market(code)]);
code_endpoint!(ZczqFinanceQuery, zczq_finance, BondCode, code, "CWServ.skef10_bond_zczqh_cwfx", [j("t"), j(code.as_str()), market(code)]);
section_endpoint!(ZczqRatingQuery, zczq_rating, BondCode, "pjzst", code, section, "CWServ.skef10_bond_zczqh_zqpj", [j(section), j(code.as_str()), market(code)]);

code_endpoint!(KjhzProfileQuery, kjhz_profile, BondCode, code, "CWServ.skef10_bond_kjhz_zqgk", [j(code.as_str()), market(code)]);
code_endpoint!(KjhzFinanceQuery, kjhz_finance, BondCode, code, "CWServ.skef10_bond_kjhz_cwfx", [j("t"), j(code.as_str())]);
section_endpoint!(KjhzInterestQuery, kjhz_interest, BondCode, "fxqk", code, section, "CWServ.skef10_bond_kjhz_fxqk", [j(section), j(code.as_str())]);
code_endpoint!(KjhzGuaranteeQuery, kjhz_guarantee, BondCode, code, "CWServ.skef10_bond_kjhz_zqdb", [j("dbsm"), j(code.as_str())]);
section_endpoint!(KjhzRatingQuery, kjhz_rating, BondCode, "pjzst", code, section, "CWServ.skef10_bond_kjhz_zqpj", [j(section), j(code.as_str())]);
section_endpoint!(KjhzExchangeQuery, kjhz_exchange, BondCode, "jgtzzst", code, section, "CWServ.skef10_bond_kjhz_jhqk", [j(section), j(code.as_str())]);
section_endpoint!(KjhzRateQuery, kjhz_rate, BondCode, "zqll", code, section, "CWServ.skef10_bond_kjhz_zqll", [j(section), j(code.as_str())]);
section_endpoint!(BondTermsQuery, bond_terms, BondCode, "hstk", code, section, "CWServ.skef10_bond_kzz_lltk", [j(section), j(code.as_str())]);
code_endpoint!(BondBulletinQuery, bond_bulletin, BondCode, code, "CWServ.skef10_bond_zjzx", [j(code.as_str()), market(code)]);

fn market(code: &BondCode) -> crate::f10::JsonValue { j(code.market().get().to_string()) }
