---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# F10-基金与ETF

## 作用

查询 ETF 与场外基金的 F10 资料：名称与市场、基金经理、基金管理人、基本概况、交易席位、规模与份额、净值与回报、分红、资产与行业配置、重仓股、持股情况、财务指标、销售信息、评级等。30 个分区由 `client.f10.fund_report()` 的 `section` 参数分派。

**ETF（`510300`、`159915` …）与场外基金（`003004`、`016947` …）共用同一批 Entry**，没有「ETF 专区」：同一 section 传不同 `code` 即可，只有 `etf` / `margin` 这类 ETF 专有分区对非 ETF 返回「是否 ETF = 否」。

| 项目 | 内容 |
| --- | --- |
| 调用方法 | `client.f10.fund_report(code, section="basic", *, arg="", date=None, page=1, page_size=20)` |
| 底层 Entry | [`CWServ.skef10_jj_jjzl`](../F10_7615.md#entry-cwserv-skef10-jj-jjzl) 等 `CWServ.skef10_jj_*` 家族 |
| 返回模型 | `F10Response` |

## 分区（section）

| section | 中文分区 | 需要额外参数 |
| --- | --- | --- |
| `name` | 名称与市场 | — |
| `managers` | 基金经理 | — |
| `company` | 基金管理人 | — |
| `basic` | 基本概况 | — |
| `seats` | 交易席位 | `arg` |
| `scale` | 规模与份额 | — |
| `scale_history` | 份额/规模历史 | `arg` 默认 `1`，可配合 `date` |
| `nav` | 单位净值序列 | — |
| `nav_period` | 区间收益 | — |
| `nav_rank` | 同类排名 | — |
| `dividends` | 基金分红 | — |
| `asset_allocation` | 资产配置 | — |
| `industry_allocation` | 行业分布 | `date` |
| `holdings_compare` | 重仓股变动 | — |
| `holdings_rank` | 重仓排名 | — |
| `holding_summary` | 基金持仓汇总 | — |
| `bond_allocation` | 债券配置 | `date` |
| `futures_allocation` | 期货持仓明细 | `date` |
| `holdings` | 持股情况 | — |
| `holdings_detail` | 持股明细 | — |
| `holdings_changes` | 持有基金明细 | — |
| `fund_of_funds` | 下属基金资产 | — |
| `finance` | 财务指标 | `arg` 选表，默认 `zcfzb` |
| `sales_info` | 费率 | — |
| `sales_announcements` | 销售公告 | — |
| `sales_news` | 相关资讯 | — |
| `etf` | 是否 ETF | `date` 默认 `3M` |
| `margin` | ETF 融资融券 | `date` 默认 `3M` |
| `ratings` | 基金评级 | `arg` 默认 `001` |
| `categories` | 分类/同类对比 | `arg` 默认 `bgqzf` |

## 示例

```python
from eltdx import F10Client

f10 = F10Client(timeout=3)

name = f10.fund_report("510300", "name")
nav = f10.fund_report("510300", "nav")
dividends = f10.fund_report("510300", "dividends")
finance = f10.fund_report("510300", "finance", arg="zcfzb")

print(name.rows[0])
print(len(nav.rows), nav.rows[0])          # 973 行净值序列
print(dividends.rows[0])
print(finance.rows[0]["rq"])               # 报告期
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `code` | 基金 / ETF 代码 |
| `section` | 分区，见上表，默认 `basic` |
| `arg` | 分区附加参数：`seats`、`scale_history`、`finance`、`ratings`、`categories` 使用 |
| `date` | `industry_allocation` / `bond_allocation` / `futures_allocation` 的报告期；`etf` / `margin` 未传时用 `3M` |
| `page` / `page_size` | 页码 / 每页条数，`managers` / `company` / `basic` / `seats` 使用 |

## 请求对照

```python
# name                -> CWServ.skef10_jj_idreq  {"Params": ["name", "510300"]}
# managers            -> CWServ.skef10_jj_jjzl  {"Params": ["1", "510300", "", "1", "1", "20"]}
# company             -> CWServ.skef10_jj_jjzl  {"Params": ["2", "510300", "", "1", "1", "20"]}
# basic               -> CWServ.skef10_jj_jjzl  {"Params": ["4", "510300", "", "1", "1", "20"]}
# seats               -> CWServ.skef10_jj_jjzl  {"Params": ["5", "510300", "", "1", "1", "20"]}
# scale               -> CWServ.skef10_jj_gmfe  {"Params": ["4", "510300", "", ""]}
# scale_history       -> CWServ.skef10_jj_gmfe  {"Params": ["1", "510300", "", ""]}
# nav                 -> CWServ.skef10_jj_jzhb  {"Params": ["0", "510300", "0", ""]}
# nav_period          -> CWServ.skef10_jj_jzhb  {"Params": ["1", "510300", "0", ""]}
# nav_rank            -> CWServ.skef10_jj_jzhb  {"Params": ["3", "510300", "0", ""]}
# dividends           -> CWServ.skef10_jj_jzhb  {"Params": ["5", "510300", "0", ""]}
# asset_allocation    -> CWServ.skef10_jj_tzzh  {"Params": ["zcpz", "510300", ""]}
# industry_allocation -> CWServ.skef10_jj_tzzh  {"Params": ["hyfb", "510300", "20260630"]}
# holdings_compare    -> CWServ.skef10_jj_tzzh  {"Params": ["cgjzd", "510300", ""]}
# holdings_rank       -> CWServ.skef10_jj_tzzh  {"Params": ["zjpz", "510300", ""]}
# holding_summary     -> CWServ.skef10_jj_tzzh  {"Params": ["jjxtzzh", "510300", ""]}
# bond_allocation     -> CWServ.skef10_jj_tzzh  {"Params": ["zczq", "510300", "20260630"]}
# futures_allocation  -> CWServ.skef10_jj_tzzh  {"Params": ["ccqhmx", "510300", "20260630"]}
# holdings            -> CWServ.skef10_jj_cgqk  {"Params": ["cgqkbgq", "510300", "", "", "1", "1", "1"]}
# holdings_detail     -> CWServ.skef10_jj_cgqk  {"Params": ["cgmxbgq", "510300", "", "", "1", "1", "1"]}
# holdings_changes    -> CWServ.skef10_jj_cgqk  {"Params": ["cyjjmxbgq", "510300", "", "", "1", "1", "1"]}
# fund_of_funds       -> CWServ.skef10_jj_cgqk  {"Params": ["sxjjzcbgq", "510300", "", "", "1", "1", "1"]}
# finance             -> CWServ.skef10_jj_cwzk  {"Params": ["zcfzb", "510300"]}
# sales_info          -> CWServ.skef10_jj_xsxx  {"Params": ["0", "510300"]}
# sales_announcements -> CWServ.skef10_jj_xsxx  {"Params": ["4", "510300"]}
# sales_news          -> CWServ.skef10_jj_xsxx  {"Params": ["5", "510300"]}
# etf                 -> CWServ.skef10_jj_etfzj  {"Params": ["etfzjlx", "510300", "3M"]}
# margin              -> CWServ.skef10_jj_etfzj  {"Params": ["etfrzrq", "510300", "3M"]}
# ratings             -> CWServ.skef10_jj_jjpj  {"Params": ["001", "510300"]}
# categories          -> CWServ.skef10_jj_fxsy  {"Params": ["bgqzf", "510300", ""]}
```

`Params[0]` 是页面内的子命令，同一个 Entry 靠它区分多个分区。

## 返回示例

`section="name"`（名称与市场）：

```python
f10.fund_report("510300", "name").rows[0]
# {"T003": "沪深300ETF华泰柏瑞", "T002": "510300", "T006": "003004", "SC": "1"}
```

`section="dividends"`（基金分红，实测 15 行）：

```python
f10.fund_report("510300", "dividends").rows[0]
# {
#     "qydjr": "2026-01-16",              # 权益登记日
#     "sjlx": "基金分红",                  # 事件类型
#     "cxr": "2026-01-19",                # 除息日
#     "sjsm": "每10份基金份额派发红利1.23元",  # 分红说明
# }
```

`section="asset_allocation"`（资产配置，实测 10 行）：

```python
f10.fund_report("510300", "asset_allocation").rows[0]
# {"rq": "2026-06-30", "T018": 95.92, "T054": 4.05, "T008": 94872183996.4, "T058": 0.89}
```

`section="nav"`（单位净值序列，实测 973 行）：

```python
f10.fund_report("510300", "nav").rows[0]
# {"rq": 20261013, "vl": 4.3479}
```

`section="etf"` / `section="margin"`：

```python
f10.fund_report("510300", "etf").rows[0]
# {"sfetf": "是"}      # 非 ETF 基金返回「否」
```

## 字段对照

| section | 常用字段 |
| --- | --- |
| `name` | `T003` 名称、`T002` 代码、`T006` 关联代码、`SC` 市场 |
| `managers` | `T003` 基金经理、`T011` 任职日期、`T018` 任职天数、`T022` 任职回报%、`rec_id` 详情 ID |
| `company` | `T024` 公司全称、`T002` 简称、`T009` 法定代表人、`T015` 成立日期、`T013` 注册资本（万元）、`T016` 网址 |
| `basic`、`scale`、`scale_history` | `rq` / `T002` 报告期 |
| `nav` | `rq` 净值日期、`vl` 单位净值 |
| `nav_period` | `rq` 日期、`T003` 单位净值、`T004`/`T005` 累计净值、`T008` 涨跌幅%、`T016` 其它口径 |
| `nav_rank` | `cd` 对比代码、`T002`～`T009` 各区间收益 |
| `dividends` | `qydjr` 权益登记日、`cxr` 除息日、`sjlx` 类型、`sjsm` 分红说明 |
| `asset_allocation` | `rq` 报告期、`T018` 股票占比%、`T054` 债券占比%、`T008` 资产净值、`T058` 其它占比% |
| `holdings_compare` / `holding_summary` | `N001` 日期、`N002` 指标；`holding_summary` 用 `rq` 标记报告期 |
| `holdings_rank` | `T001`～`T021` 重仓明细、`rq` 报告期 |
| `bond_allocation` | `code`/`name` 债券、`T008` 市值、`T009`/`T010` 数量与占比 |
| `futures_allocation` | `N002` 合约代码、`N003` 名称、`N004`～`N006` 持仓与市值 |
| `holdings` / `holdings_detail` / `holdings_changes` / `fund_of_funds` | `rq` 报告期；明细列随报表类型变化 |
| `finance` | `rq` 报告期、`T008` 起各科目金额；`arg` 可换 `zcfzb` 等报表 |
| `sales_info` | `T021` 申购费率%、`T020` 赎回费率% |
| `sales_announcements` / `sales_news` | `rq` 日期、`Title` 标题、`rec_id` 详情 ID、`currq` 标记 |
| `etf` / `margin` | `sfetf` 是否 ETF |
| `ratings` | `N001` 评级日期、`N002`～`N004` 各机构评级 |
| `categories` | `T001` 年度、`T002`/`T004`/`T006` 区间收益、`T003`/`T005`/`T007` 同类排名 |

各分区返回的表数不定（多为 1～2 张，`sales_info` 可达 7 张），全部表可用：

```python
for table in response.tables:
    print(table.key, table.columns, table.rows[:2])
```

适合做 ETF / 基金筛选、净值跟踪、持仓穿透和费率对比。
