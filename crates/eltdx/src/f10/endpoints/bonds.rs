//! Convertible, government, corporate, repo, ABS and exchangeable bonds.
//! Explicit bond market is retained, including overlapping numerical ranges.
use super::SimpleArgument::{Code, Literal, Market, Section};
use crate::f10::BondCode;

code_endpoint!(BondProfileQuery, bond_profile, BondCode, code, "CWServ.skef10_bond_kzz_zqgk", wire [Literal("sdcyrbgq"), Code, Literal("")]);
section_endpoint!(BondFinanceQuery, bond_finance, BondCode, "zcfzb", code, section, "CWServ.skef10_bond_kzz_cwfx", wire [Section, Code]);
section_endpoint!(BondRatingQuery, bond_rating, BondCode, "pjzst", code, section, "CWServ.skef10_bond_kzz_zqpj", wire [Section, Code]);
section_endpoint!(BondConversionQuery, bond_conversion, BondCode, "zgjg", code, section, "CWServ.skef10_bond_kzz_zg", wire [Section, Code]);
section_endpoint!(BondRateQuery, bond_rate, BondCode, "zqll", code, section, "CWServ.skef10_bond_kzz_zqll", wire [Section, Code]);
section_endpoint!(BondAnnouncementsQuery, bond_announcements, BondCode, "1", code, section, "CWServ.skef10_bond_kzz_zjzx", wire [Code, Section]);

section_endpoint!(GzProfileQuery, gz_profile, BondCode, "zqgk", code, section, "CWServ.skef10_bond_gz_zjgk", wire [Section, Code, Market]);
section_endpoint!(GzInterestQuery, gz_interest, BondCode, "fxqk", code, section, "CWServ.skef10_bond_gz_fxqk", wire [Section, Code, Market]);
section_endpoint!(GzMarketDataQuery, gz_market_data, BondCode, "dzjy", code, section, "CWServ.skef10_bond_gz_zjsj", wire [Section, Code, Market]);
section_endpoint!(GzRatingQuery, gz_rating, BondCode, "dfzf", code, section, "CWServ.skef10_bond_gz_zjpj", wire [Section, Code, Market]);
code_endpoint!(GzFinanceQuery, gz_finance, BondCode, code, "CWServ.skef10_bond_gz_cwfx", wire [Literal("t"), Code, Market]);

section_endpoint!(QyzProfileQuery, qyz_profile, BondCode, "zqzl", code, section, "CWServ.skef10_bond_qyz_zqgk", wire [Section, Code, Market]);
section_endpoint!(QyzInterestQuery, qyz_interest, BondCode, "fxqk", code, section, "CWServ.skef10_bond_qyz_xjlfx", wire [Section, Code, Market]);
code_endpoint!(QyzGuaranteeQuery, qyz_guarantee, BondCode, code, "CWServ.skef10_bond_qyz_zqdb", wire [Literal("dbsm"), Code, Market]);
section_endpoint!(QyzRatingQuery, qyz_rating, BondCode, "zjpjzst", code, section, "CWServ.skef10_bond_qyz_zjpj", wire [Section, Code, Market]);
code_endpoint!(QyzRateQuery, qyz_rate, BondCode, code, "CWServ.skef10_bond_qyz_zjlv", wire [Literal("zqlv"), Code, Market]);
code_endpoint!(QyzReviewQuery, qyz_review, BondCode, code, "CWServ.skef10_bond_qyz_gsps", wire [Code, Market]);

code_endpoint!(GzhgInfoQuery, gzhg_info, BondCode, code, "CWServ.skef10_bond_gzhg_zqxx", wire [Literal("zqgk"), Code]);
section_endpoint!(GzhgQuotesQuery, gzhg_quotes, BondCode, "bpzs", code, section, "CWServ.skef10_bond_gzhg_zqxx", wire [Section, Code]);
code_endpoint!(ZczqProfileQuery, zczq_profile, BondCode, code, "CWServ.skef10_bond_zczqh_zqgk", wire [Code, Market]);
code_endpoint!(ZczqFinanceQuery, zczq_finance, BondCode, code, "CWServ.skef10_bond_zczqh_cwfx", wire [Literal("t"), Code, Market]);
section_endpoint!(ZczqRatingQuery, zczq_rating, BondCode, "pjzst", code, section, "CWServ.skef10_bond_zczqh_zqpj", wire [Section, Code, Market]);

code_endpoint!(KjhzProfileQuery, kjhz_profile, BondCode, code, "CWServ.skef10_bond_kjhz_zqgk", wire [Code, Market]);
code_endpoint!(KjhzFinanceQuery, kjhz_finance, BondCode, code, "CWServ.skef10_bond_kjhz_cwfx", wire [Literal("t"), Code]);
section_endpoint!(KjhzInterestQuery, kjhz_interest, BondCode, "fxqk", code, section, "CWServ.skef10_bond_kjhz_fxqk", wire [Section, Code]);
code_endpoint!(KjhzGuaranteeQuery, kjhz_guarantee, BondCode, code, "CWServ.skef10_bond_kjhz_zqdb", wire [Literal("dbsm"), Code]);
section_endpoint!(KjhzRatingQuery, kjhz_rating, BondCode, "pjzst", code, section, "CWServ.skef10_bond_kjhz_zqpj", wire [Section, Code]);
section_endpoint!(KjhzExchangeQuery, kjhz_exchange, BondCode, "jgtzzst", code, section, "CWServ.skef10_bond_kjhz_jhqk", wire [Section, Code]);
section_endpoint!(KjhzRateQuery, kjhz_rate, BondCode, "zqll", code, section, "CWServ.skef10_bond_kjhz_zqll", wire [Section, Code]);
section_endpoint!(BondTermsQuery, bond_terms, BondCode, "hstk", code, section, "CWServ.skef10_bond_kzz_lltk", wire [Section, Code]);
code_endpoint!(BondBulletinQuery, bond_bulletin, BondCode, code, "CWServ.skef10_bond_zjzx", wire [Code, Market]);

