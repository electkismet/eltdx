"""Context-scoped labels for raw 7615 F10 field names.

The same ``Txxx`` name can mean different things in different TQLEX entries,
so labels must always be resolved with both the Entry and its request scope.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any


_COMPANY_PROFILE_LISTING = {
    "T035": "股票类别",
    "T031": "上市日期",
    "T051": "发行方式",
    "fxzd": "发行制度",
    "mgmz": "每股面值",
    "T042": "发行价格",
    "T044": "总发行数量",
    "T056": "实际募资总额",
    "T057": "实际募资净额",
    "T059": "首日开盘价",
    "T060": "首日收盘价",
    "zcxs": "主承销商",
    "bjr": "上市保荐人",
}

_COMPANY_PROFILE_INDEX_CHANGES = {
    "N001": "公布日期",
    "N002": "调整方向",
    "N003": "指数名称",
    "N004": "公布日涨跌幅",
    "N005": "调整日期",
    "N006": "调整日涨跌幅",
}

_HOT_TOPICS = {
    "id": "题材 ID",
    "ztmc": "题材名称",
    "gld": "关联度",
    "rxsj": "入选日期",
    "ztrq": "题材日期",
    "ztnr": "入选原因",
    "arec": "详情记录 ID",
    "bflag": "服务端标记",
    "sslb": "所属类别标记",
}

# The finance response exposes its statement family in a metadata result set.
# ``nhytype`` is part of the schema: 0=general, 1=bank, 2=securities,
# 3=insurance. Raw Txxx names must not be shared across these templates.
_BALANCE_SHEET_COMMON = {
    "rtype": "报表类型",
    "nhytype": "行业报表模板",
    "zqname": "证券名称",
}

_BALANCE_SHEET_GENERAL = {
    "rq": "报告期",
    "T007": "货币资金",
    "T008": "交易性金融资产",
    "T120": "应收票据及应收账款",
    "T009": "应收票据",
    "T010": "应收账款",
    "T011": "预付款项",
    "T012": "其他应收款合计",
    "T013": "应收利息",
    "T014": "应收股利",
    "T016": "存货",
    "T017": "持有待售资产",
    "T018": "一年内到期的非流动资产",
    "T019": "其他流动资产",
    "T020": "流动资产合计",
    "T021": "可供出售金融资产",
    "T022": "持有至到期投资",
    "T024": "长期股权投资",
    "T025": "投资性房地产",
    "T026": "固定资产",
    "T027": "在建工程",
    "T030": "生产性生物资产",
    "T031": "油气资产",
    "T032": "无形资产",
    "T033": "开发支出",
    "T034": "商誉",
    "T035": "长期待摊费用",
    "T036": "递延所得税资产",
    "T037": "其他非流动资产",
    "T038": "非流动资产合计",
    "T039": "资产总计",
    "T040": "短期借款",
    "T041": "交易性金融负债",
    "T097": "衍生金融负债",
    "T118": "应付票据及应付账款",
    "T042": "应付票据",
    "T043": "应付账款",
    "T044": "预收款项",
    "T045": "应付职工薪酬",
    "T046": "应交税费",
    "T047": "应付利息",
    "T048": "应付股利",
    "T049": "其他应付款合计",
    "T051": "一年内到期的非流动负债",
    "T052": "其他流动负债",
    "T053": "流动负债合计",
    "T054": "长期借款",
    "T055": "应付债券",
    "T056": "长期应付款",
    "T057": "专项应付款",
    "T058": "预计负债",
    "T059": "递延所得税负债",
    "T060": "其他非流动负债",
    "T061": "非流动负债合计",
    "T083": "递延收益",
    "T109": "长期应付职工薪酬",
    "T119": "合同负债",
    "T062": "负债合计",
    "T063": "实收资本 / 股本",
    "T064": "资本公积金",
    "T065": "盈余公积",
    "T066": "库存股",
    "T067": "未分配利润",
    "T068": "少数股东权益",
    "T077": "归母权益合计",
    "T071": "所有者权益合计",
    "T072": "负债和股东权益合计",
    "T082": "其他综合收益",
    "T110": "其他权益工具",
    "T111": "优先股",
    "T112": "永续债",
    "T115": "其他权益工具投资",
    "T116": "其他非流动金融资产",
    "T121": "合同资产",
    "T123": "使用权资产",
    **_BALANCE_SHEET_COMMON,
}

_BALANCE_SHEET_BANK = {
    "rq": "报告期",
    "T010": "现金及存放中央银行款项",
    "T007": "存放同业款项",
    "T013": "贵金属",
    "T014": "拆出资金",
    "T015": "交易性金融资产",
    "T016": "衍生金融资产",
    "T017": "买入返售金融资产",
    "T018": "应收利息",
    "T028": "发放贷款及垫款",
    "T033": "可供出售金融资产",
    "T034": "持有至到期投资",
    "T035": "长期股权投资",
    "T037": "投资性房地产",
    "T039": "固定资产",
    "T041": "无形资产",
    "T046": "递延所得税资产",
    "T047": "其他资产",
    "T048": "资产总计",
    "T049": "向中央银行借款",
    "T050": "同业及其他金融机构存放款项",
    "T053": "拆入资金",
    "T054": "交易性金融负债",
    "T055": "衍生金融负债",
    "T056": "卖出回购金融资产款",
    "T057": "吸收存款",
    "T066": "应付职工薪酬",
    "T067": "应交税费",
    "T068": "应付利息",
    "T070": "预计负债",
    "T079": "应付债券",
    "T081": "递延所得税负债",
    "T082": "其他负债",
    "T083": "负债合计",
    "T084": "实收资本 / 股本",
    "T085": "资本公积金",
    "T087": "盈余公积",
    "T088": "一般风险准备",
    "T089": "未分配利润",
    "T090": "少数股东权益",
    "T091": "外币报表折算差额",
    "T093": "所有者权益合计",
    "T094": "负债和股东权益合计",
    "T099": "归母权益合计",
    "T105": "其他权益工具",
    "T106": "优先股",
    "T107": "永续债",
    "T108": "其他综合收益",
    "T109": "商誉",
    "T114": "债权投资 / 以摊余成本计量的金融资产",
    "T115": "其他债权投资",
    "T116": "其他权益工具投资",
    "T119": "使用权资产",
    **_BALANCE_SHEET_COMMON,
}

_BALANCE_SHEET_SECURITIES = {
    "rq": "报告期",
    "T008": "货币资金",
    "T009": "客户资金存款",
    "T011": "结算备付金",
    "T012": "客户备付金",
    "T098": "融出资金",
    "T015": "交易性金融资产",
    "T016": "衍生金融资产",
    "T017": "买入返售金融资产",
    "T104": "应收款项",
    "T018": "应收利息",
    "T029": "存出保证金",
    "T033": "可供出售金融资产",
    "T034": "持有至到期投资",
    "T035": "长期股权投资",
    "T039": "固定资产",
    "T041": "无形资产",
    "T042": "交易席位费",
    "T046": "递延所得税资产",
    "T047": "其他资产",
    "T048": "资产总计",
    "T051": "短期借款",
    "T110": "应付短期融资款",
    "T053": "拆入资金",
    "T054": "交易性金融负债",
    "T055": "衍生金融负债",
    "T056": "卖出回购金融资产款",
    "T058": "代理买卖证券款",
    "T059": "代理承销证券款",
    "T066": "应付职工薪酬",
    "T067": "应交税费",
    "T111": "应付款项",
    "T068": "应付利息",
    "T070": "预计负债",
    "T079": "应付债券",
    "T081": "递延所得税负债",
    "T082": "其他负债",
    "T083": "负债合计",
    "T084": "实收资本 / 股本",
    "T085": "资本公积金",
    "T087": "盈余公积",
    "T088": "一般风险准备",
    "T089": "未分配利润",
    "T090": "少数股东权益",
    "T093": "所有者权益合计",
    "T094": "负债和股东权益合计",
    "T099": "归母权益合计",
    "T105": "其他权益工具",
    "T106": "优先股",
    "T107": "永续债",
    "T014": "拆出资金",
    "T112": "库存股",
    "T114": "债权投资 / 以摊余成本计量的金融资产",
    "T108": "其他综合收益",
    "T109": "商誉",
    "T115": "其他债权投资",
    "T116": "其他权益工具投资",
    "T117": "其他负债调整项目",
    **_BALANCE_SHEET_COMMON,
}

_BALANCE_SHEET_INSURANCE = {
    **_BALANCE_SHEET_SECURITIES,
    "T011": "结算备付金",
    "T014": "拆出资金",
    "T015": "以公允价值计量且其变动计入当期损益的金融资产",
    "T018": "应收利息",
    "T019": "应收保费",
    "T021": "应收分保账款",
    "T022": "应收分保未到期责任准备金",
    "T023": "应收分保未决赔款准备金",
    "T024": "应收分保寿险责任准备金",
    "T025": "应收分保长期健康险责任准备金",
    "T026": "保户质押贷款",
    "T027": "定期存款",
    "T033": "可供出售金融资产",
    "T034": "持有至到期投资",
    "T036": "存出资本保证金",
    "T037": "投资性房地产",
    "T045": "独立账户资产",
    "T049": "其他负债调整项目",
    "T050": "同业及其他金融机构存放款项",
    "T051": "短期借款",
    "T053": "拆入资金",
    "T054": "以公允价值计量且其变动计入当期损益的金融负债",
    "T063": "预收保费",
    "T064": "应付手续费及佣金",
    "T065": "应付分保账款",
    "T071": "应付赔付款",
    "T072": "应付保单红利",
    "T073": "保户储金及投资款",
    "T074": "未到期责任准备金",
    "T075": "未决赔款准备金",
    "T076": "寿险责任准备金",
    "T077": "长期健康险责任准备金",
    "T078": "长期借款",
    "T080": "独立账户负债",
    "T106": "优先股",
    "T114": "债权投资",
    "T119": "使用权资产",
}

# Profit and loss statement (``lyb``).  These labels are based on the live
# 7615 response schemas for the four industry templates.  Fields whose
# meaning changes between releases are deliberately left unmapped.
_INCOME_GENERAL = {
    "rq": "报告期",
    "T035": "营业总收入",
    "T008": "营业收入",
    "T064": "营业总成本",
    "T009": "营业成本",
    "T010": "营业税金及附加",
    "T011": "销售费用",
    "T012": "管理费用",
    "T014": "财务费用",
    "T015": "资产减值损失（旧口径）",
    "T016": "公允价值变动收益",
    "T017": "投资收益",
    "T018": "对联营企业和合营企业的投资收益",
    "T020": "营业利润",
    "T022": "营业外收入",
    "T023": "营业外支出",
    "T026": "利润总额",
    "T027": "所得税费用",
    "T029": "净利润",
    "T030": "归属于母公司股东的净利润",
    "T031": "少数股东损益",
    "T038": "其他综合收益",
    "T039": "综合收益总额",
    "T040": "归属于母公司所有者的综合收益总额",
    "T041": "归属于少数股东的综合收益总额",
    "T057": "其他收益",
    "T058": "资产处置收益",
    "T059": "持续经营净利润",
    "T062": "终止经营净利润",
    "T061": "研发费用",
    "T065": "财务费用中的利息费用",
    "T066": "财务费用中的利息收入",
    "T067": "信用减值损失",
    "T068": "资产减值损失",
    "cT010": "扣除非经常性损益后的归属于母公司股东的净利润",
    **_BALANCE_SHEET_COMMON,
}

_INCOME_BANK = {
    "rq": "报告期",
    "T008": "营业收入",
    "T009": "利息净收入",
    "T010": "利息收入",
    "T011": "利息支出",
    "T012": "手续费及佣金净收入",
    "T013": "手续费及佣金收入",
    "T014": "手续费及佣金支出",
    "T023": "投资收益",
    "T024": "对联营企业和合营企业的投资收益",
    "T025": "公允价值变动收益",
    "T026": "汇兑收益",
    "T027": "其他业务收入",
    "T028": "营业支出",
    "T036": "营业税金及附加",
    "T038": "业务及管理费",
    "T040": "资产减值损失",
    "T041": "其他业务成本",
    "T042": "营业利润",
    "T044": "营业外收入",
    "T045": "营业外支出",
    "T047": "利润总额",
    "T048": "所得税费用",
    "T050": "净利润",
    "T051": "归属于母公司股东的净利润",
    "T052": "少数股东损益",
    "T060": "其他综合收益",
    "T061": "综合收益总额",
    "T062": "归属于母公司所有者的综合收益总额",
    "T063": "归属于少数股东的综合收益总额",
    "T064": "其他收益",
    "T065": "资产处置收益",
    "T066": "持续经营净利润",
    "T068": "信用减值损失",
    "cT010": "扣除非经常性损益后的归属于母公司股东的净利润",
    **_BALANCE_SHEET_COMMON,
}

_INCOME_SECURITIES = {
    "rq": "报告期",
    "T008": "营业收入",
    "T009": "利息净收入",
    "T012": "手续费及佣金净收入",
    "T015": "代理买卖证券业务净收入",
    "T016": "承销证券业务净收入",
    "T017": "资产管理业务净收入",
    "T023": "投资收益",
    "T024": "对联营企业和合营企业的投资收益",
    "T025": "公允价值变动收益",
    "T026": "汇兑收益",
    "T027": "其他业务收入",
    "T028": "营业支出",
    "T036": "营业税金及附加",
    "T038": "业务及管理费",
    "T040": "资产减值损失",
    "T041": "其他业务成本",
    "T042": "营业利润",
    "T044": "营业外收入",
    "T045": "营业外支出",
    "T047": "利润总额",
    "T048": "所得税费用",
    "T050": "净利润",
    "T051": "归属于母公司股东的净利润",
    "T052": "少数股东损益",
    "T060": "其他综合收益",
    "T061": "综合收益总额",
    "T062": "归属于母公司所有者的综合收益总额",
    "T063": "归属于少数股东的综合收益总额",
    "T064": "其他收益",
    "T065": "资产处置收益",
    "T066": "持续经营净利润",
    "T068": "信用减值损失",
    "cT010": "扣除非经常性损益后的归属于母公司股东的净利润",
    **_BALANCE_SHEET_COMMON,
}

_INCOME_INSURANCE = {
    "rq": "报告期",
    "T008": "营业收入",
    "T018": "已赚保费",
    "T019": "保险业务收入",
    "T020": "分保费收入",
    "T021": "分出保费",
    "T022": "提取未到期责任准备金",
    "T023": "投资收益",
    "T024": "对联营企业和合营企业的投资收益",
    "T025": "公允价值变动收益",
    "T026": "汇兑收益",
    "T027": "其他业务收入",
    "T028": "营业支出",
    "T029": "退保金",
    "T030": "赔付支出",
    "T031": "摊回赔付支出",
    "T032": "提取保险责任准备金",
    "T033": "摊回保险责任准备金",
    "T034": "保单红利支出",
    "T035": "分保费用",
    "T036": "营业税金及附加",
    "T037": "手续费及佣金支出",
    "T038": "业务及管理费",
    "T039": "摊回分保费用",
    "T040": "资产减值损失",
    "T041": "其他业务成本",
    "T042": "营业利润",
    "T044": "营业外收入",
    "T045": "营业外支出",
    "T047": "利润总额",
    "T048": "所得税费用",
    "T050": "净利润",
    "T051": "归属于母公司股东的净利润",
    "T052": "少数股东损益",
    "T060": "其他综合收益",
    "T061": "综合收益总额",
    "T062": "归属于母公司所有者的综合收益总额",
    "T063": "归属于少数股东的综合收益总额",
    "T064": "其他收益",
    "T065": "资产处置收益",
    "T066": "持续经营净利润",
    "T068": "信用减值损失",
    "cT010": "扣除非经常性损益后的归属于母公司股东的净利润",
    **_BALANCE_SHEET_COMMON,
}

# Cash-flow statement (``xjllb``).  Ambiguous duplicate/legacy fields are
# intentionally omitted; they remain visible through ``unmapped_fields`` and
# are explained through ``field_notes`` when their ambiguity is understood.
_CASHFLOW_GENERAL = {
    "rq": "报告期",
    "T008": "销售商品、提供劳务收到的现金",
    "T009": "收到的税费返还",
    "T010": "收到其他与经营活动有关的现金",
    "T011": "经营活动现金流入小计",
    "T012": "购买商品、接受劳务支付的现金",
    "T013": "支付给职工以及为职工支付的现金",
    "T014": "支付的各项税费",
    "T015": "支付其他与经营活动有关的现金",
    "T016": "经营活动现金流出小计",
    "T017": "经营活动产生的现金流量净额",
    "T018": "收回投资收到的现金",
    "T019": "取得投资收益收到的现金",
    "T020": "处置固定资产、无形资产和其他长期资产收回的现金净额",
    "T021": "处置子公司及其他营业单位收到的现金净额",
    "T022": "收到其他与投资活动有关的现金",
    "T023": "投资活动现金流入小计",
    "T024": "购建固定资产、无形资产和其他长期资产支付的现金",
    "T025": "投资支付的现金",
    "T026": "取得子公司及其他营业单位支付的现金净额",
    "T027": "支付其他与投资活动有关的现金",
    "T028": "投资活动现金流出小计",
    "T029": "投资活动产生的现金流量净额",
    "T030": "吸收投资收到的现金",
    "T031": "取得借款收到的现金",
    "T032": "收到其他与筹资活动有关的现金",
    "T033": "筹资活动现金流入小计",
    "T034": "偿还债务支付的现金",
    "T035": "分配股利、利润或偿付利息支付的现金",
    "T096": "子公司支付给少数股东的股利、利润",
    "T036": "支付其他与筹资活动有关的现金",
    "T037": "筹资活动现金流出小计",
    "T038": "筹资活动产生的现金流量净额",
    "T039": "汇率变动对现金及现金等价物的影响",
    "T041": "现金及现金等价物净增加额",
    "T042": "期初现金及现金等价物余额",
    "T043": "期末现金及现金等价物余额",
    "T046": "净利润",
    "T047": "资产减值准备",
    "T048": "固定资产折旧",
    "T049": "无形资产摊销",
    "T050": "长期待摊费用摊销",
    "T051": "处置固定资产等损失",
    "T052": "固定资产报废损失",
    "T053": "公允价值变动损失",
    "T054": "财务费用",
    "T055": "投资损失",
    "T056": "递延所得税资产减少",
    "T057": "递延所得税负债增加",
    "T058": "存货的减少",
    "T059": "经营性应收项目的减少",
    "T060": "经营性应付项目的增加",
    "T061": "其他",
    "T062": "经营活动产生的现金流量净额（补充资料）",
    # This is an abstract/grouping row in the cash-flow supplement, so it is
    # expected to be null even though its heading has stable semantics.
    "T063": "不涉及现金收支的重大投资和筹资活动",
    "T064": "债务转为资本",
    "T065": "一年内到期的可转换公司债券",
    "T066": "融资租入固定资产",
    "T067": "不涉及现金收支的投资和筹资活动其他项目",
    "T068": "期末现金余额",
    "T069": "期初现金余额",
    "T070": "期末现金及现金等价物余额",
    "T071": "期初现金及现金等价物余额",
    "T072": "现金及现金等价物净增加额的其他调节项目",
    "T073": "现金及现金等价物净增加额（补充资料）",
    "T078": "固定资产折旧（补充）",
    "T095": "子公司吸收少数股东投资收到的现金",
    "T098": "期末现金余额",
    "T108": "使用权资产折旧/摊销",
    **_BALANCE_SHEET_COMMON,
}

_CASHFLOW_GENERAL_NOTES = {
    "T063": "现金流量表补充资料的分组标题，通常不承载数值。",
}

_CASHFLOW_BANK = {
    "rq": "报告期",
    "T009": "同业及其他金融机构存放款项净增加额",
    "T010": "向中央银行借款净增加额",
    "T011": "向其他金融机构拆入资金净增加额",
    "T012": "收取利息、手续费及佣金的现金",
    "T021": "收到其他与经营活动有关的现金",
    "T022": "经营活动现金流入小计",
    "T023": "发放贷款及垫款净增加额",
    "T024": "向中央银行及同业款项净增加额",
    "T025": "支付利息、手续费及佣金的现金",
    "T029": "支付给职工以及为职工支付的现金",
    "T030": "支付的各项税费",
    "T031": "支付其他与经营活动有关的现金",
    "T032": "经营活动现金流出小计",
    "T033": "经营活动产生的现金流量净额",
    "T035": "收回投资收到的现金",
    "T036": "取得投资收益收到的现金",
    "T037": "处置固定资产、无形资产和其他长期资产收回的现金净额（旧字段）",
    "T038": "投资活动现金流入小计",
    "T039": "投资支付的现金",
    "T041": "购建固定资产、无形资产和其他长期资产支付的现金",
    "T042": "支付其他与投资活动有关的现金",
    "T043": "投资活动现金流出小计",
    "T044": "投资活动产生的现金流量净额",
    "T046": "筹资活动现金流入其他项目（旧字段）",
    "T047": "发行债券收到的现金",
    "T048": "取得借款收到的现金",
    "T049": "收到其他与筹资活动有关的现金",
    "T050": "筹资活动现金流入小计",
    "T051": "偿还债务支付的现金",
    "T052": "分配股利、利润或偿付利息支付的现金",
    "T053": "支付其他与筹资活动有关的现金",
    "T054": "筹资活动现金流出小计",
    "T055": "筹资活动产生的现金流量净额",
    "T056": "汇率变动对现金及现金等价物的影响",
    "T058": "现金及现金等价物净增加额",
    "T059": "期初现金及现金等价物余额",
    "T060": "期末现金及现金等价物余额",
    "T073": "处置固定资产、无形资产和其他长期资产收回的现金净额",
    "T076": "净利润",
    "T077": "资产减值准备",
    "T078": "固定资产折旧",
    "T079": "无形资产摊销",
    "T080": "长期待摊费用摊销",
    "T081": "处置固定资产等损失",
    "T082": "固定资产报废损失",
    "T083": "公允价值变动损失",
    "T084": "债券利息支出",
    "T085": "投资损失",
    "T086": "递延所得税",
    "T087": "递延所得税资产减少",
    # T088 is intentionally omitted: it is a dynamic extension row.  For
    # example, historical bank observations align with long-asset disposal
    # gains for one issuer and unrealised foreign-exchange losses for another.
    # Neither industry nor report date is enough to select one fixed label.
    "T089": "经营性应收项目的减少",
    "T090": "经营性应付项目的增加",
    "T091": "经营活动现金流量调节项目的其他项",
    "T092": "经营活动产生的现金流量净额（补充资料）",
    "T093": "支付其他与筹资活动有关的现金",
    "T094": "债务转为资本",
    "T095": "一年内到期的可转换公司债券",
    "T096": "融资租入固定资产",
    "T097": "不涉及现金收支的投资和筹资活动其他项目",
    "T098": "期末现金余额",
    "T099": "期初现金余额",
    "T100": "期末现金及现金等价物余额",
    "T101": "期初现金及现金等价物余额",
    "T102": "现金及现金等价物净增加额的其他调节项目",
    "T103": "现金及现金等价物净增加额（补充资料）",
    "T111": "信用减值损失",
    **_BALANCE_SHEET_COMMON,
}

_CASHFLOW_BANK_NOTES = {
    "T088": (
        "银行历史报表的动态扩展调节项，含义随证券及报告期变化；"
        "不能使用统一字段名。"
    ),
}

_CASHFLOW_SECURITIES = {
    "rq": "报告期",
    "T013": "经营活动现金流入其他项目",
    "T012": "收取利息、手续费及佣金的现金",
    "T014": "拆入资金净增加额",
    "T015": "回购业务资金净增加额",
    "T106": "经营活动现金流入其他项目（旧字段）",
    "T107": "收到代理买卖证券款净额",
    "T021": "收到其他与经营活动有关的现金",
    "T022": "经营活动现金流入小计",
    "T108": "经营活动现金流出其他项目",
    "T109": "支付代理买卖证券款净额",
    "T025": "支付利息、手续费及佣金的现金",
    "T029": "支付给职工以及为职工支付的现金",
    "T030": "支付的各项税费",
    "T031": "支付其他与经营活动有关的现金",
    "T032": "经营活动现金流出小计",
    "T033": "经营活动产生的现金流量净额",
    "T035": "收回投资收到的现金",
    "T036": "取得投资收益收到的现金",
    "T073": "处置固定资产、无形资产和其他长期资产收回的现金净额",
    "T037": "收到其他与投资活动有关的现金",
    "T038": "投资活动现金流入小计",
    "T039": "投资支付的现金",
    "T041": "购建固定资产、无形资产和其他长期资产支付的现金",
    "T042": "支付其他与投资活动有关的现金",
    "T043": "投资活动现金流出小计",
    "T044": "投资活动产生的现金流量净额",
    "T046": "吸收投资收到的现金",
    "T047": "发行债券收到的现金",
    "T048": "取得借款收到的现金",
    "T049": "收到其他与筹资活动有关的现金",
    "T050": "筹资活动现金流入小计",
    "T051": "偿还债务支付的现金",
    "T052": "分配股利、利润或偿付利息支付的现金",
    "T053": "支付其他与筹资活动有关的现金",
    "T054": "筹资活动现金流出小计",
    "T055": "筹资活动产生的现金流量净额",
    "T056": "汇率变动对现金及现金等价物的影响",
    "T058": "现金及现金等价物净增加额",
    "T059": "期初现金及现金等价物余额",
    "T060": "期末现金及现金等价物余额",
    "T076": "净利润",
    "T077": "资产减值准备",
    "T078": "固定资产及使用权资产折旧",
    "T079": "无形资产摊销",
    "T080": "长期待摊费用摊销",
    "T081": "处置固定资产等损失",
    "T082": "固定资产报废损失",
    "T083": "公允价值变动损失",
    "T084": "经营活动现金流量调节项目的其他项",
    "T085": "投资损失",
    "T086": "递延所得税资产减少",
    "T087": "递延所得税负债增加",
    "T088": "存货的减少",
    "T089": "经营性应收项目的减少",
    "T090": "经营性应付项目的增加",
    "T091": "其他",
    "T092": "经营活动产生的现金流量净额（补充资料）",
    "T093": "筹资活动现金流出其他项目",
    "T094": "债务转为资本",
    "T095": "一年内到期的可转换公司债券",
    "T096": "融资租入固定资产",
    "T097": "不涉及现金收支的投资和筹资活动其他项目",
    "T098": "期末现金余额",
    "T099": "期初现金余额",
    "T100": "期末现金及现金等价物余额",
    "T101": "期初现金及现金等价物余额",
    "T102": "现金及现金等价物净增加额的其他调节项目",
    "T103": "现金及现金等价物净增加额（补充资料）",
    "T111": "信用减值损失",
    **_BALANCE_SHEET_COMMON,
}

_CASHFLOW_INSURANCE = {
    **_CASHFLOW_SECURITIES,
    "T018": "收到原保险合同保费取得的现金",
    "T019": "收到再保业务现金净额",
    "T020": "保户储金及投资款净增加额",
    "T021": "收到其他与经营活动有关的现金",
    "T027": "支付原保险合同赔付款项的现金",
    "T028": "支付保单红利的现金",
    "T040": "保户质押贷款净增加额",
    "T073": "处置固定资产、无形资产和其他长期资产收回的现金净额",
    "T078": "固定资产及使用权资产折旧",
    "T084": "经营活动现金流量调节项目的其他项",
    "T086": "递延所得税",
    "T088": "交易性金融资产的减少",
    "T093": "筹资活动现金流出其他项目",
}

_COMPANY_RESEARCH = {
    "T004": "评级类别",
    "T009": "研究员",
    "T012": "撰写日期",
    "T011": "研报详情 ID",
    "T039": "研报标题",
    "nflag": "服务端标记",
    "ybdz": "研报地址 / 附件标识",
    "zs": "总记录数",
}

_SUPERVISION = {
    "N001": "处罚公布日期",
    "N002": "处罚对象",
    "N003": "监管措施",
    "N004": "函件内容",
    "N005": "链接",
    "zs": "总记录数",
}

_SCOPE_PARAM_INDEX = {
    "CWServ.tdxf10_gg_gsgk": 0,
    "CWServ.tdxf10_gg_rdtc": 1,
    "CWServ.tdxf10_gg_cwfx": 1,
    "CWServ.tdxf10_gg_gszx": 1,
}

_FIELD_LABELS: dict[tuple[str, str], Mapping[str, str]] = {
    ("CWServ.tdxf10_gg_gsgk", "8"): _COMPANY_PROFILE_LISTING,
    ("CWServ.tdxf10_gg_gsgk", "9"): _COMPANY_PROFILE_INDEX_CHANGES,
    ("CWServ.tdxf10_gg_rdtc", "zttzbkz"): _HOT_TOPICS,
    ("CWServ.tdxf10_gg_cwfx", "zcfzb:0"): _BALANCE_SHEET_GENERAL,
    ("CWServ.tdxf10_gg_cwfx", "zcfzb:1"): _BALANCE_SHEET_BANK,
    ("CWServ.tdxf10_gg_cwfx", "zcfzb:2"): _BALANCE_SHEET_SECURITIES,
    ("CWServ.tdxf10_gg_cwfx", "zcfzb:3"): _BALANCE_SHEET_INSURANCE,
    ("CWServ.tdxf10_gg_cwfx", "lyb:0"): _INCOME_GENERAL,
    ("CWServ.tdxf10_gg_cwfx", "lyb:1"): _INCOME_BANK,
    ("CWServ.tdxf10_gg_cwfx", "lyb:2"): _INCOME_SECURITIES,
    ("CWServ.tdxf10_gg_cwfx", "lyb:3"): _INCOME_INSURANCE,
    ("CWServ.tdxf10_gg_cwfx", "xjllb:0"): _CASHFLOW_GENERAL,
    ("CWServ.tdxf10_gg_cwfx", "xjllb:1"): _CASHFLOW_BANK,
    ("CWServ.tdxf10_gg_cwfx", "xjllb:2"): _CASHFLOW_SECURITIES,
    ("CWServ.tdxf10_gg_cwfx", "xjllb:3"): _CASHFLOW_INSURANCE,
    ("CWServ.tdxf10_gg_gszx", "gsyj"): _COMPANY_RESEARCH,
    ("CWServ.tdxf10_gg_gszx", "jgcs"): _SUPERVISION,
}

_FIELD_NOTES: dict[tuple[str, str], Mapping[str, str]] = {
    ("CWServ.tdxf10_gg_cwfx", "xjllb:0"): _CASHFLOW_GENERAL_NOTES,
    ("CWServ.tdxf10_gg_cwfx", "xjllb:1"): _CASHFLOW_BANK_NOTES,
}


def field_labels_for(
    entry: str,
    request_body: Any,
    *,
    metadata: Mapping[str, Any] | None = None,
) -> dict[str, str]:
    """Return known labels for one Entry and request scope.

    An empty mapping means that the field semantics are not documented for
    this exact scope. Callers must preserve the raw field names in that case.
    """

    scope = _field_scope(entry, request_body, metadata=metadata)
    if scope is None:
        return {}
    return dict(_FIELD_LABELS.get((entry, scope), {}))


def field_notes_for(
    entry: str,
    request_body: Any,
    *,
    metadata: Mapping[str, Any] | None = None,
) -> dict[str, str]:
    """Return caveats for fields whose shape needs extra interpretation."""

    scope = _field_scope(entry, request_body, metadata=metadata)
    if scope is None:
        return {}
    return dict(_FIELD_NOTES.get((entry, scope), {}))


def describe_fields(
    entry: str,
    request_body: Any,
    fields: Sequence[str],
    *,
    metadata: Mapping[str, Any] | None = None,
) -> tuple[dict[str, str], list[str]]:
    """Split response field keys into known labels and explicit unknowns."""

    known = field_labels_for(entry, request_body, metadata=metadata)
    labels: dict[str, str] = {}
    unmapped: list[str] = []
    for field in fields:
        base, duplicate_index = _split_duplicate_suffix(field)
        label = known.get(field)
        if label is None:
            label = known.get(base)
        if label is None:
            unmapped.append(field)
            continue
        if duplicate_index is not None:
            label = f"{label}（{duplicate_index}）"
        labels[field] = label
    return labels, unmapped


def describe_field_notes(
    entry: str,
    request_body: Any,
    fields: Sequence[str],
    *,
    metadata: Mapping[str, Any] | None = None,
) -> dict[str, str]:
    """Return only the scoped notes that apply to fields in one result set."""

    known = field_notes_for(entry, request_body, metadata=metadata)
    notes: dict[str, str] = {}
    for field in fields:
        base, _ = _split_duplicate_suffix(field)
        note = known.get(field)
        if note is None:
            note = known.get(base)
        if note is not None:
            notes[field] = note
    return notes


def _split_duplicate_suffix(field: str) -> tuple[str, int | None]:
    base, separator, suffix = field.rpartition("__")
    if separator and suffix.isdigit() and int(suffix) >= 2:
        return base, int(suffix)
    return field, None


def _field_scope(
    entry: str,
    request_body: Any,
    *,
    metadata: Mapping[str, Any] | None,
) -> str | None:
    param_index = _SCOPE_PARAM_INDEX.get(entry)
    if param_index is None or not isinstance(request_body, Mapping):
        return None
    params = request_body.get("Params")
    if not isinstance(params, Sequence) or isinstance(
        params, (str, bytes, bytearray)
    ):
        return None
    if param_index >= len(params):
        return None
    scope = str(params[param_index])
    if entry == "CWServ.tdxf10_gg_cwfx" and scope in {"zcfzb", "lyb", "xjllb"}:
        industry_type = metadata.get("nhytype") if metadata else None
        if industry_type is None:
            return None
        scope = f"{scope}:{industry_type}"
    return scope


__all__ = [
    "describe_field_notes",
    "describe_fields",
    "field_labels_for",
    "field_notes_for",
]
