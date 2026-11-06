window.ELTDX_CATALOG = {
  "schema_version": 16,
  "release": {
    "version": "3.3.5",
    "status": "stable",
    "backend": "Rust native 7709 protocol and transport engine",
    "python_api": "compatible modular API and dataclasses",
    "native_commands": 25,
    "mcp_tools": 35,
    "mcp_resources": 8,
    "native_extensions": 1,
    "pure_python_7709_fallback": false
  },
  "taxonomy": {
    "layers": [
      {
        "id": "7709",
        "label": "7709 原生协议接口",
        "tag_label": "7709 原生",
        "stat_label": "7709 原生协议接口",
        "description": "26 个 7709 原生协议业务能力，按业务分类展示。",
        "source": "7709"
      },
      {
        "id": "7615",
        "label": "7615 原生 Entry 接口",
        "tag_label": "7615 原生 Entry",
        "stat_label": "7615 原生 Entry 接口",
        "description": "25 个 7615 F10 调用入口，涵盖 HTTP Entry 封装与静态资讯频道枚举。",
        "source": "F10"
      },
      {
        "id": "helpers",
        "label": "Helpers 封装",
        "tag_label": "Helpers",
        "stat_label": "Helpers 封装",
        "description": "23 个 Helpers 封装，组合协议调用、解析、整理和本地计算。",
        "source": "Helper"
      },
      {
        "id": "ex",
        "label": "扩展行情（7727）",
        "tag_label": "EX",
        "stat_label": "扩展行情（7727）",
        "description": "扩展行情（7727）接口；行情覆盖以服务器实际支持为准。",
        "source": "EX"
      },
      {
        "id": "mac",
        "label": "MAC 行情",
        "tag_label": "MAC",
        "stat_label": "MAC 行情",
        "description": "MAC 行情接口；行情覆盖以服务器实际支持为准。",
        "source": "MAC"
      },
      {
        "id": "macex",
        "label": "MAC-EX 基金与外盘",
        "tag_label": "MAC-EX",
        "stat_label": "MAC-EX 基金与外盘",
        "description": "MAC-EX 基金与外盘接口；行情覆盖以服务器实际支持为准。",
        "source": "MAC-EX"
      }
    ],
    "functional_groups": [
      {
        "id": "basics",
        "label": "基础接口（非手动调用）",
        "description": "连接时自动握手，长连接由后台定时保活，一般不需要手动调用。",
        "item_ids": [
          "7709-handshake",
          "7709-heartbeat"
        ]
      },
      {
        "id": "codes",
        "label": "证券代码",
        "description": "查询市场证券数量，以及分页或全量读取证券代码表。",
        "item_ids": [
          "7709-code-count",
          "7709-code-list",
          "helper-latest-stock-list",
          "helper-latest-st",
          "helper-latest-suspended"
        ]
      },
      {
        "id": "realtime",
        "label": "实时行情",
        "description": "当前行情快照、五档盘口、当日分时、当日成交和实时走势。",
        "item_ids": [
          "7709-quote-snapshots",
          "7709-legacy-quotes",
          "7709-quote-depth",
          "7709-category-quotes",
          "7709-quote-refresh",
          "7709-minute-today",
          "7709-minute-aux",
          "7709-sparkline",
          "7709-trades-today",
          "helper-daily-price-limits",
          "helper-realtime-rank",
          "helper-board-quotes",
          "helper-board-member-quotes",
          "helper-buy-sell-strength",
          "helper-volume-comparison"
        ]
      },
      {
        "id": "history",
        "label": "历史行情",
        "description": "指定日期或近期的历史分时与历史成交明细。",
        "item_ids": [
          "7709-minute-history",
          "7709-minute-recent",
          "7709-trades-history",
          "7709-money-flow"
        ]
      },
      {
        "id": "bars",
        "label": "K 线与复权",
        "description": "周期 K 线、自动分页、服务端复权，以及本地复权所需的权息和仿射系数。",
        "item_ids": [
          "7709-kline",
          "7709-delisted-bars",
          "7709-gbbq",
          "7709-local-factors"
        ]
      },
      {
        "id": "auction-shortline",
        "label": "集合竞价与短线",
        "description": "集合竞价过程、09:25 正式撮合、竞价整理和短线指标。",
        "item_ids": [
          "7709-auction-series",
          "7709-opening-match-today",
          "7709-opening-match-history",
          "helper-shortline-indicators",
          "helper-auction-data",
          "helper-daily-share-capital",
          "helper-limit-ladder",
          "helper-theme-strength-rank"
        ]
      },
      {
        "id": "f10",
        "label": "F10",
        "description": "F10 公司资料、财务、股东、题材、排名和资讯，以及相关的便捷查询。",
        "categories": [
          "网关",
          "公司资料",
          "经营财务",
          "股东与融资",
          "公司治理",
          "题材行情",
          "评价排名",
          "估值行情",
          "资讯公告",
          "连板天梯"
        ],
        "item_ids": [
          "helper-stock-topics",
          "helper-topic-stocks"
        ]
      },
      {
        "id": "common",
        "label": "常用封装",
        "description": "跨多个数据源的组合查询、特殊能力和暂未归入其他功能的新增接口。",
        "fallback": true,
        "item_ids": [
          "7709-special-limits",
          "7709-finance",
          "7709-file-content",
          "helper-stock-profile",
          "helper-server-download",
          "helper-server-stats"
        ]
      },
      {
        "id": "ex",
        "label": "扩展行情（7727）",
        "description": "扩展行情（7727）的标的、行情和历史数据。",
        "item_ids": [
          "ex-connection",
          "ex-markets",
          "ex-count",
          "ex-info",
          "ex-quote",
          "ex-quotes",
          "ex-bars",
          "ex-history-bars",
          "ex-minute",
          "ex-history-minute",
          "ex-trades",
          "ex-history-trades"
        ]
      },
      {
        "id": "mac",
        "label": "MAC 行情",
        "description": "MAC 行情的标的、行情和历史数据。",
        "item_ids": [
          "mac-connection",
          "mac-quotes",
          "mac-ranking",
          "mac-bars",
          "mac-minute",
          "mac-get-tick-charts",
          "mac-get-chart-sampling",
          "mac-trades",
          "mac-get-symbol-info",
          "mac-boards",
          "mac-get-board-members",
          "mac-get-belong-board",
          "mac-get-board-summary",
          "mac-get-board-ranking",
          "mac-get-board-change-ranking",
          "mac-get-capital-flow",
          "mac-get-auction",
          "mac-get-unusual",
          "mac-get-server-info",
          "mac-get-kline-offset",
          "mac-get-file-meta",
          "mac-download-file-chunk",
          "mac-download-file",
          "mac-get-goods-list"
        ]
      },
      {
        "id": "macex",
        "label": "MAC-EX 基金与外盘",
        "description": "MAC-EX 基金与外盘的标的、行情和历史数据。",
        "item_ids": [
          "macex-connection",
          "macex-instruments",
          "macex-goods-count",
          "macex-goods-list",
          "macex-quotes",
          "macex-goods-quotes-list",
          "macex-bars",
          "macex-minute",
          "macex-goods-chart-sampling",
          "macex-trades",
          "macex-goods-transaction-all",
          "macex-macro-indicators",
          "macex-macro-quotes",
          "macex-macro-kline"
        ]
      }
    ]
  },
  "items": [
    {
      "id": "7709-handshake",
      "title": "握手",
      "source": "7709",
      "category": "会话",
      "api": "client.session.handshake()",
      "aliases": [
        "handshake"
      ],
      "protocol": "0x000d",
      "kind": "底层协议",
      "summary": "读取服务端时间、交易时段、主站名和产品标识；连接后通常自动执行。",
      "return_model": "HandshakeInfo",
      "doc": "methods/7709-握手.md"
    },
    {
      "id": "7709-heartbeat",
      "title": "心跳",
      "source": "7709",
      "category": "会话",
      "api": "client.session.heartbeat()",
      "aliases": [
        "heartbeat",
        "keepalive"
      ],
      "protocol": "0x0004",
      "kind": "底层协议",
      "summary": "读取服务端心跳响应；长连接默认由后台定时保活。",
      "return_model": "HeartbeatAck",
      "doc": "methods/7709-心跳.md"
    },
    {
      "id": "7709-code-count",
      "title": "市场代码数量",
      "source": "7709",
      "category": "证券代码",
      "api": "client.codes.count(market)",
      "calls": [
        {
          "label": "市场全部代码数",
          "api": "client.codes.count(market)"
        },
        {
          "label": "仅 A 股数量",
          "api": "client.codes.a_share_count(market)"
        }
      ],
      "aliases": [
        "count",
        "a_share_count",
        "A 股数量"
      ],
      "protocol": "0x044e",
      "kind": "底层协议 / 便捷筛选",
      "summary": "count() 返回整个市场代码表条数，不限 A 股；a_share_count() 拉取代码表后只统计 A 股。",
      "return_model": "int",
      "doc": "methods/7709-代码数量.md"
    },
    {
      "id": "7709-code-list",
      "title": "代码表",
      "source": "7709",
      "category": "证券代码",
      "api": "client.codes.list() / client.codes.all()",
      "calls": [
        {
          "label": "推荐 · 市场全量",
          "api": "client.codes.all(market, ...)"
        },
        {
          "label": "常用 · 沪深北 A 股",
          "api": "client.codes.all_a_shares()"
        },
        {
          "label": "手动分页",
          "api": "client.codes.list(market, ...)"
        }
      ],
      "aliases": [
        "list",
        "all",
        "ETF",
        "指数"
      ],
      "protocol": "0x044d",
      "kind": "底层协议",
      "summary": "推荐用 all() 自动翻页；也可直接取某市场或沪深北全部 A 股、ETF、指数。",
      "return_model": "list[SecurityCode]",
      "doc": "methods/7709-代码表.md"
    },
    {
      "id": "7709-quote-snapshots",
      "title": "批量行情快照",
      "source": "7709",
      "category": "行情",
      "api": "client.quotes.get_snapshots(codes)",
      "aliases": [
        "quote",
        "ETF",
        "指数"
      ],
      "protocol": "0x054c",
      "kind": "底层协议",
      "summary": "查询当前行情，返回现价、成交量额和一档盘口（买一、卖一）。",
      "return_model": "list[QuoteSnapshot]",
      "doc": "methods/7709-批量快照.md"
    },
    {
      "id": "7709-legacy-quotes",
      "title": "旧版批量行情",
      "source": "7709",
      "category": "行情",
      "api": "client.quotes.legacy(codes)",
      "aliases": [
        "legacy_quotes",
        "legacy",
        "旧版行情",
        "五档盘口"
      ],
      "protocol": "0x053e",
      "kind": "底层协议",
      "summary": "无游标的旧版完整快照，一次返回五档、交易状态和旧版尾部字段；不支持增量刷新或推送。",
      "return_model": "list[LegacyQuote]",
      "doc": "methods/7709-旧版批量行情.md"
    },
    {
      "id": "7709-quote-depth",
      "title": "完整行情 / 五档盘口",
      "source": "Helper",
      "category": "行情封装",
      "api": "client.helpers.full_quotes(codes)",
      "aliases": [
        "level2",
        "买五",
        "卖五",
        "ETF",
        "指数"
      ],
      "protocol": "0x054c + 0x0547",
      "kind": "协议封装",
      "summary": "普通用户的推荐入口：自动组合 0x054c 基础快照与 0x0547 五档数据，返回完整行情。",
      "return_model": "list[QuoteSnapshot]",
      "doc": "helpers/完整行情.md"
    },
    {
      "id": "7709-category-quotes",
      "title": "分类行情",
      "source": "7709",
      "category": "行情",
      "api": "client.quotes.list_by_category(...)",
      "aliases": [
        "list_by_category",
        "sort",
        "分类行情原始分页"
      ],
      "protocol": "0x054b",
      "kind": "底层协议",
      "summary": "底层原始分页接口：按市场或板块返回一页行情；实时榜单封装基于此方法自动翻页并整理排名。",
      "return_model": "CategoryQuotePage",
      "doc": "methods/7709-分类行情.md"
    },
    {
      "id": "7709-quote-refresh",
      "title": "行情订阅接口",
      "source": "7709",
      "category": "行情",
      "api": "client.quotes.subscribe(codes, callback)",
      "aliases": [
        "subscribe",
        "subscribe_hq",
        "行情订阅",
        "实时推送",
        "refresh",
        "get_depth",
        "poll_push",
        "drain_pushes",
        "push queue"
      ],
      "protocol": "0x0547",
      "kind": "订阅接口",
      "summary": "订阅股票并持续接收行情推送；自动维护游标、每 7 秒续期，支持多连接批量订阅。",
      "return_model": "QuoteSubscription",
      "doc": "methods/7709-增量刷新推送队列.md"
    },
    {
      "id": "7709-kline",
      "title": "K 线 / 周期线",
      "source": "7709",
      "category": "K 线",
      "api": "client.bars.get(code_or_codes, ...)",
      "aliases": [
        "bars.get",
        "all_pages",
        "自动分页",
        "OHLC",
        "前复权",
        "后复权",
        "ETF",
        "指数"
      ],
      "protocol": "0x052d",
      "kind": "底层协议",
      "summary": "返回分钟、日、周、月、季、年 K 线，支持单页、自动分页、单只或多只证券并发查询和服务端复权参数。",
      "return_model": "KlineSeries | dict[str, KlineSeries]",
      "doc": "methods/7709-K线周期线.md"
    },
    {
      "id": "7709-delisted-bars",
      "title": "退市 K 线（日线、不复权）",
      "source": "7709",
      "category": "K 线",
      "api": "client.delisted_bars.get(code, ...)",
      "aliases": [
        "delisted_bars.get",
        "退市",
        "历史证券",
        "不复权",
        "自动分页",
        "批量"
      ],
      "protocol": "0x052b",
      "kind": "底层协议",
      "summary": "查询退市股票的不复权日线，支持单只或批量查询、自动分页。",
      "return_model": "KlineSeries | dict[str, KlineSeries]",
      "doc": "methods/7709-退市K线日线接口.md"
    },
    {
      "id": "7709-minute-today",
      "title": "当日分时",
      "source": "7709",
      "category": "分时",
      "api": "client.minutes.today(code)",
      "aliases": [
        "minutes.today",
        "分时图",
        "ETF",
        "指数"
      ],
      "protocol": "0x0537",
      "kind": "底层协议",
      "summary": "返回主站当前保存的每分钟价格、成交量和均价等分时序列。",
      "return_model": "MinuteSeries",
      "doc": "methods/7709-当日分时.md"
    },
    {
      "id": "7709-minute-history",
      "title": "指定日期历史分时",
      "source": "7709",
      "category": "分时",
      "api": "client.minutes.history(code, date)",
      "aliases": [
        "minutes.history",
        "历史分时"
      ],
      "protocol": "0x0fb4",
      "kind": "底层协议",
      "summary": "按指定日期返回某个交易日的分时价格和分钟成交量。",
      "return_model": "MinuteSeries",
      "doc": "methods/7709-指定日期历史分时.md"
    },
    {
      "id": "7709-minute-recent",
      "title": "近期历史分时",
      "source": "7709",
      "category": "分时",
      "api": "client.minutes.recent()",
      "aliases": [
        "minutes.recent",
        "近期分时"
      ],
      "protocol": "0x0feb",
      "kind": "底层协议",
      "summary": "返回服务端近期窗口内的历史分时，适合查询较近交易日的分钟走势。",
      "return_model": "MinuteSeries",
      "doc": "methods/7709-近期历史分时.md"
    },
    {
      "id": "7709-minute-aux",
      "title": "分时副图",
      "source": "7709",
      "category": "分时",
      "api": "client.minutes.aux()",
      "aliases": [
        "minutes.aux",
        "买卖力道",
        "成交对比"
      ],
      "protocol": "0x051b",
      "kind": "底层协议",
      "summary": "返回分时页副图序列，例如买卖力道和成交对比等数据。",
      "return_model": "MinuteAuxSeries",
      "doc": "methods/7709-分时副图.md"
    },
    {
      "id": "7709-sparkline",
      "title": "小走势图",
      "source": "7709",
      "category": "分时",
      "api": "client.minutes.sparkline()",
      "aliases": [
        "sparkline",
        "mini chart"
      ],
      "protocol": "0x0fd1",
      "kind": "底层协议",
      "summary": "返回单个标的的小型价格走势序列，适合列表页或概览图。",
      "return_model": "SparklineSeries",
      "doc": "methods/7709-小走势图.md"
    },
    {
      "id": "7709-trades-today",
      "title": "当日成交明细",
      "source": "7709",
      "category": "逐笔成交",
      "api": "client.trades.today(code, ..., batch_size=None) / client.trades.all_today(code, ..., batch_size=None)",
      "calls": [
        {
          "label": "主要调用",
          "api": "client.trades.today(code, ..., batch_size=None)"
        },
        {
          "label": "完整分页",
          "api": "client.trades.all_today(code, ..., batch_size=None)"
        }
      ],
      "aliases": [
        "trades.today",
        "ticks",
        "逐笔"
      ],
      "protocol": "0x0fc5",
      "kind": "底层协议",
      "summary": "返回主站当前保存的混合记录：普通成交、status=8 竞价快照和 09:25 正式撮合；code 可传单个代码或代码列表。",
      "return_model": "TradePage | dict[str, TradePage]",
      "doc": "methods/7709-当日成交明细.md"
    },
    {
      "id": "7709-trades-history",
      "title": "历史成交明细",
      "source": "7709",
      "category": "逐笔成交",
      "api": "client.trades.history(code, date, ..., batch_size=None) / client.trades.all_history(code, date, ..., batch_size=None)",
      "calls": [
        {
          "label": "主要调用 · 单页",
          "api": "client.trades.history(code, date, ..., batch_size=None)"
        },
        {
          "label": "完整分页",
          "api": "client.trades.all_history(code, date, ..., batch_size=None)"
        },
        {
          "label": "可选批量返回",
          "api": "client.trades.history_batch(code, date) / client.trades.all_history_batch(code, date)"
        }
      ],
      "aliases": [
        "trades.history",
        "trades.history_batch",
        "trades.all_history_batch",
        "历史逐笔"
      ],
      "protocol": "0x0fc6",
      "kind": "底层协议",
      "summary": "返回指定日期混合记录：普通成交、status=8 竞价快照和 09:25 正式撮合；code 可传单个代码或代码列表。",
      "return_model": "TradePage | dict[str, TradePage]",
      "doc": "methods/7709-历史成交明细.md"
    },
    {
      "id": "7709-auction-series",
      "title": "集合竞价过程快照",
      "source": "7709",
      "category": "集合竞价",
      "api": "client.auctions.series(code, date=None)",
      "aliases": [
        "auctions.series",
        "竞价序列"
      ],
      "protocol": "0x056a",
      "kind": "底层协议",
      "summary": "不传日期查询当日，传入日期查询历史；按主站顺序返回全部过程时间点，09:25 附近记录也不是正式成交。",
      "return_model": "AuctionSeries",
      "doc": "methods/7709-集合竞价明细.md"
    },
    {
      "id": "7709-opening-match-today",
      "title": "当日 09:25 正式撮合",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.trades.opening_match_today(code, ..., batch_size=None)",
      "protocol": "0x0fc5",
      "kind": "组合接口",
      "summary": "从当日成交明细中提取 09:25 正式撮合记录；code 可传单个代码或代码列表，无记录时返回 None。",
      "return_model": "TradeTick | None | dict[str, TradeTick | None]",
      "doc": "methods/7709-当日0925正式撮合.md"
    },
    {
      "id": "7709-opening-match-history",
      "title": "历史 09:25 正式撮合",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.trades.opening_match_history(code, date, ..., batch_size=None)",
      "protocol": "0x0fc6",
      "kind": "组合接口",
      "summary": "从指定日期的历史成交明细中提取 09:25 正式撮合记录；code 可传单个代码或代码列表，无记录时返回 None。",
      "return_model": "TradeTick | None | dict[str, TradeTick | None]",
      "doc": "methods/7709-历史0925正式撮合.md"
    },
    {
      "id": "7709-gbbq",
      "title": "股本变迁 / GBBQ",
      "source": "7709",
      "category": "公司基础",
      "api": "client.corporate.capital_changes(code_or_codes, batch_size=75)",
      "aliases": [
        "capital_changes",
        "股本事件"
      ],
      "protocol": "0x000f",
      "kind": "底层协议",
      "summary": "返回标签 1..15 的广义权息资料；默认每批 75 只，可调至 200，并支持跨主站并发拆批。",
      "return_model": "CapitalChangeBlock | CapitalChangeBatch",
      "doc": "methods/7709-股本变迁GBBQ.md"
    },
    {
      "id": "7709-money-flow",
      "title": "资金流向日数据",
      "source": "7709",
      "category": "资金流向",
      "api": "client.money_flow.daily(code, ..., batch_size=75)",
      "aliases": [
        "money_flow",
        "资金流向",
        "主力净额",
        "ASK_OneZJLX"
      ],
      "protocol": "0x0ffc",
      "kind": "底层协议",
      "summary": "读取一只或多只股票最近的日资金流向记录，返回主力/主买净额及占比，以及两种口径各自的超大单、大单、中单、小单净额和原始字段。",
      "return_model": "MoneyFlowBlock | MoneyFlowBatch",
      "doc": "methods/0x0ffc-资金流向日数据接口.md",
      "legacy_registry": false
    },
    {
      "id": "7709-local-factors",
      "title": "本地复权系数",
      "source": "Helper",
      "category": "公司基础",
      "api": "client.corporate.adjustment_factors(code_or_codes, anchor_date=None, *, start_date=None, batch_size=75)",
      "aliases": [
        "qfq",
        "hfq"
      ],
      "protocol": "0x000f",
      "kind": "组合接口",
      "summary": "根据批量权息资料，为本地不复权 K 线逐只计算事件级前、后复权仿射系数。",
      "return_model": "AdjustmentFactorResponse | AdjustmentFactorBatch",
      "doc": "methods/7709-本地复权系数.md"
    },
    {
      "id": "7709-finance",
      "title": "财务基础信息",
      "source": "7709",
      "category": "公司基础",
      "api": "client.corporate.finance_batch(codes)",
      "aliases": [
        "finance_batch",
        "EPS",
        "资产",
        "利润"
      ],
      "protocol": "0x0010",
      "kind": "底层协议",
      "summary": "批量返回流通股本、总股本、EPS、资产、负债、收入和利润等基础字段。",
      "return_model": "FinanceBatch",
      "doc": "methods/7709-财务基础信息.md"
    },
    {
      "id": "7709-special-limits",
      "title": "特殊品种涨跌停限制",
      "source": "7709",
      "category": "交易限制",
      "api": "client.limits.special() / client.limits.scan_special()",
      "calls": [
        {
          "label": "单页读取",
          "api": "client.limits.special(...)"
        },
        {
          "label": "连续扫描",
          "api": "client.limits.scan_special(...)"
        }
      ],
      "aliases": [
        "limits.special",
        "scan_special",
        "涨停",
        "跌停"
      ],
      "protocol": "0x0452",
      "kind": "底层协议",
      "summary": "分页或连续扫描特殊品种涨跌停限制表，并按代码建立索引。",
      "return_model": "SpecialLimitPage",
      "doc": "methods/7709-特殊品种涨跌停限制.md"
    },
    {
      "id": "7709-file-content",
      "title": "服务器文件分块读取",
      "source": "7709",
      "category": "服务器资源",
      "api": "client.resources.read(path, ...)",
      "aliases": [
        "file_content",
        "服务器文件",
        "文件块"
      ],
      "protocol": "0x06b9",
      "kind": "底层协议",
      "summary": "按路径、偏移和长度读取一个服务器文件块，返回长度头与原始内容。",
      "return_model": "FileContentChunk",
      "doc": "methods/7709-服务器文件读取.md"
    },
    {
      "id": "f10-generic-entry",
      "title": "高级调用（需手动指定 Entry 和参数）",
      "source": "F10",
      "category": "网关",
      "api": "client.f10.call(...) / client.f10.params(...)",
      "calls": [
        {
          "label": "请求体调用",
          "api": "client.f10.call(entry, body=...)"
        },
        {
          "label": "参数调用",
          "api": "client.f10.params(entry, ...)"
        }
      ],
      "aliases": [
        "call",
        "params",
        "TQLEX"
      ],
      "protocol": "7615 / TQLEX",
      "kind": "高级调用",
      "summary": "由用户手动指定 Entry 和参数；用于调试、验证资料函数或调用 SDK 尚未封装的 Entry，不会由 SDK 自动执行。",
      "return_model": "F10Response",
      "doc": "methods/F10-通用Entry调用.md"
    },
    {
      "id": "f10-stock-info",
      "title": "股票基础信息",
      "source": "F10",
      "category": "公司资料",
      "api": "client.f10.stock_info() / client.f10.business_periods() / client.f10.topic_ids()",
      "calls": [
        {
          "label": "股票信息",
          "api": "client.f10.stock_info(code)"
        },
        {
          "label": "主营报告期",
          "api": "client.f10.business_periods(code)"
        },
        {
          "label": "题材 ID",
          "api": "client.f10.topic_ids(code)"
        }
      ],
      "aliases": [
        "stock_info",
        "business_periods",
        "topic_ids"
      ],
      "protocol": "CWServ.tdxf10_gg_comreq",
      "kind": "Entry 封装",
      "summary": "查询股票名称、代码、市场，以及主营报告期和题材 ID 等辅助信息。",
      "return_model": "F10Response",
      "doc": "methods/F10-股票基础信息.md"
    },
    {
      "id": "f10-limit-board-ladder",
      "title": "涨跌停列表",
      "source": "F10",
      "category": "连板天梯",
      "api": "client.f10.limit_up_down_list()",
      "aliases": [
        "limit_up_down_list",
        "limit_board_ladder",
        "cfg_fx_lbtt",
        "涨跌停列表",
        "连板天梯",
        "涨停明细"
      ],
      "protocol": "CWServ.cfg_fx_lbtt",
      "kind": "Entry 封装",
      "summary": "按日期或日期范围返回逐股涨停、炸板和跌停列表，以及涨停原因、封单和炸板次数；可选市场概况。",
      "return_model": "LimitBoardLadder",
      "doc": "methods/F10-涨跌停列表.md"
    },
    {
      "id": "f10-company-profile",
      "title": "公司概况",
      "source": "F10",
      "category": "公司资料",
      "api": "client.f10.company_profile()",
      "aliases": [
        "company_profile",
        "上市日期",
        "发行价"
      ],
      "protocol": "CWServ.tdxf10_gg_gsgk",
      "kind": "Entry 封装",
      "summary": "返回发行上市信息，包括上市日期、发行方式、发行价、募资额和承销商等。",
      "return_model": "F10Response",
      "doc": "methods/F10-公司概况.md"
    },
    {
      "id": "f10-business-composition",
      "title": "主营构成",
      "source": "F10",
      "category": "经营财务",
      "api": "client.f10.business_composition()",
      "aliases": [
        "business_composition",
        "主营收入",
        "毛利率"
      ],
      "protocol": "CWServ.tdxf10_gg_jyfx",
      "kind": "Entry 封装",
      "summary": "按行业、产品或地区返回主营收入、成本、毛利、占比和毛利率。",
      "return_model": "F10Response",
      "doc": "methods/F10-主营构成.md"
    },
    {
      "id": "f10-shareholder-change",
      "title": "股东增减持",
      "source": "F10",
      "category": "股东与融资",
      "api": "client.f10.shareholder_change_plans()",
      "aliases": [
        "shareholder_change_plans",
        "减持计划",
        "增持计划"
      ],
      "protocol": "CWServ.tdxf10_gg_gdyj",
      "kind": "Entry 封装",
      "summary": "返回股东增减持计划的公告日、方向、拟变动数量比例和计划日期。",
      "return_model": "F10Response",
      "doc": "methods/F10-股东增减持.md"
    },
    {
      "id": "f10-dividend-financing",
      "title": "分红融资",
      "source": "F10",
      "category": "股东与融资",
      "api": "client.f10.dividend_financing()",
      "aliases": [
        "dividend_financing",
        "分红方案",
        "股权登记日"
      ],
      "protocol": "CWServ.tdxf10_gg_fhrz",
      "kind": "Entry 封装",
      "summary": "返回分红方案、登记日、除权派息日、股息率、支付率和融资数据。",
      "return_model": "F10Response",
      "doc": "methods/F10-分红融资.md"
    },
    {
      "id": "f10-allotment",
      "title": "增发获配",
      "source": "F10",
      "category": "股东与融资",
      "api": "client.f10.allotment_dates() / client.f10.allotment_details()",
      "calls": [
        {
          "label": "日期列表",
          "api": "client.f10.allotment_dates(code)"
        },
        {
          "label": "获配明细",
          "api": "client.f10.allotment_details(code, date)"
        }
      ],
      "aliases": [
        "allotment_dates",
        "allotment_details",
        "定增"
      ],
      "protocol": "CWServ.tdxf10_gg_fhrz_zfhpmx",
      "kind": "Entry 封装",
      "summary": "先取增发日期，再按日期查询获配机构、数量、金额和锁定期明细。",
      "return_model": "F10Response",
      "doc": "methods/F10-增发获配.md"
    },
    {
      "id": "f10-finance-report",
      "title": "财务报表",
      "source": "F10",
      "category": "经营财务",
      "api": "client.f10.finance_report()",
      "aliases": [
        "finance_report",
        "资产负债表",
        "利润表",
        "现金流量表"
      ],
      "protocol": "CWServ.tdxf10_gg_cwfx",
      "kind": "Entry 封装",
      "summary": "返回多期资产负债表、利润表或现金流量表等财务报表数据。",
      "return_model": "F10Response",
      "doc": "methods/F10-财务报表.md"
    },
    {
      "id": "f10-finance-diagnosis",
      "title": "财务诊断",
      "source": "F10",
      "category": "经营财务",
      "api": "client.f10.finance_diagnosis()",
      "aliases": [
        "finance_diagnosis",
        "盈利能力",
        "成长能力",
        "财务评分"
      ],
      "protocol": "CWServ.tdxf10_gg_cwzd",
      "kind": "Entry 封装",
      "summary": "返回营运、盈利、成长、现金流、资产质量、预警和综合评分等诊断项。",
      "return_model": "F10Response",
      "doc": "methods/F10-财务诊断.md"
    },
    {
      "id": "f10-stock-score",
      "title": "个股总评",
      "source": "F10",
      "category": "评价排名",
      "api": "client.f10.stock_score()",
      "aliases": [
        "stock_score",
        "综合评分",
        "基本面评分"
      ],
      "protocol": "CWServ.tdxf10_gg_ggzp",
      "kind": "Entry 封装",
      "summary": "返回综合评分、行业和市场排名，以及资金面、基本面、消息面和主题面评分。",
      "return_model": "F10Response",
      "doc": "methods/F10-个股总评.md"
    },
    {
      "id": "f10-profit-forecast",
      "title": "盈利预测",
      "source": "F10",
      "category": "经营财务",
      "api": "client.f10.profit_forecast()",
      "aliases": [
        "profit_forecast",
        "EPS 预测",
        "净利润预测"
      ],
      "protocol": "CWServ.tdxf10_gg_ybpj",
      "kind": "Entry 封装",
      "summary": "返回未来年度 EPS、归母净利润和营业收入预测及预测机构数量。",
      "return_model": "F10Response",
      "doc": "methods/F10-盈利预测.md"
    },
    {
      "id": "f10-theme-market",
      "title": "题材概念行情",
      "source": "F10",
      "category": "题材行情",
      "api": "client.f10.theme_market()",
      "aliases": [
        "theme_market",
        "概念板块",
        "成分股"
      ],
      "protocol": "HQServ.hq_nlp_tcihq",
      "kind": "Entry 封装",
      "summary": "返回相关板块、板块成分股、主力控盘比例、资金走势和区间统计等。",
      "return_model": "F10Response",
      "doc": "methods/F10-题材概念行情.md"
    },
    {
      "id": "f10-valuation",
      "title": "估值市场数据",
      "source": "F10",
      "category": "估值行情",
      "api": "client.f10.valuation()",
      "aliases": [
        "valuation",
        "PE",
        "PB",
        "市值",
        "估值百分位"
      ],
      "protocol": "HQServ.hq_nlp_gpsj",
      "kind": "Entry 封装",
      "summary": "返回 PE、PB、市销率、市现率、估值百分位、流通市值和总市值等。",
      "return_model": "F10Response",
      "doc": "methods/F10-估值市场数据.md"
    },
    {
      "id": "f10-ranking",
      "title": "市场 / 行业排名",
      "source": "F10",
      "category": "评价排名",
      "api": "client.f10.ranking_detail()",
      "aliases": [
        "ranking_detail",
        "行业排名",
        "市场排名"
      ],
      "protocol": "CWServ.tdxf10_gg_zxts_rqpm",
      "kind": "Entry 封装",
      "summary": "返回当前股票排名、排名变化，以及同组股票代码、简称、市场和更新时间。",
      "return_model": "F10Response",
      "doc": "methods/F10-市场行业排名.md"
    },
    {
      "id": "f10-governance",
      "title": "资本运作治理",
      "source": "F10",
      "category": "公司治理",
      "api": "client.f10.governance()",
      "aliases": [
        "governance",
        "违规处理",
        "担保明细"
      ],
      "protocol": "CWServ.tdxf10_gg_zbyz",
      "kind": "Entry 封装",
      "summary": "返回担保、违规处理、处罚日期、案情进展、处罚决定和详情记录 ID。",
      "return_model": "F10Response",
      "doc": "methods/F10-资本运作治理.md"
    },
    {
      "id": "f10-hot-topics",
      "title": "热点题材",
      "source": "F10",
      "category": "题材行情",
      "api": "client.f10.hot_topics()",
      "aliases": [
        "hot_topics",
        "题材名称",
        "入选原因"
      ],
      "protocol": "CWServ.tdxf10_gg_rdtc",
      "kind": "Entry 封装",
      "summary": "返回题材名称、关联度、入选日期、入选原因、事件名称和详情 ID。",
      "return_model": "F10Response",
      "doc": "methods/F10-热点题材.md"
    },
    {
      "id": "f10-topic-compare",
      "title": "题材内对比",
      "source": "F10",
      "category": "题材行情",
      "api": "client.f10.topic_compare() / client.f10.topic_compare_first()",
      "calls": [
        {
          "label": "指定题材",
          "api": "client.f10.topic_compare(code, topic_id, ...)"
        },
        {
          "label": "首个题材",
          "api": "client.f10.topic_compare_first(code, ...)"
        }
      ],
      "aliases": [
        "topic_compare",
        "topic_compare_first",
        "题材排名"
      ],
      "protocol": "CWServ.tdxf10_gg_rdtc_gndb",
      "kind": "Entry 封装",
      "summary": "返回题材内股票的财务、市值和区间涨幅排名，用于比较同题材个股。",
      "return_model": "F10Response",
      "doc": "methods/F10-题材内对比.md"
    },
    {
      "id": "f10-market-news",
      "title": "首页要闻",
      "source": "F10",
      "category": "资讯公告",
      "api": "client.f10.market_news()",
      "aliases": [
        "market_news",
        "要闻速递",
        "滚动资讯",
        "NewsHomeTab"
      ],
      "protocol": "CWServ.tdxi_zxxwsy",
      "kind": "Entry 封装",
      "summary": "首页要闻、滚动资讯和今日焦点，支持分页。",
      "return_model": "F10Response",
      "doc": "methods/F10-首页要闻.md"
    },
    {
      "id": "f10-channel-news",
      "title": "频道资讯",
      "source": "F10",
      "category": "资讯公告",
      "api": "client.f10.channel_news()",
      "aliases": [
        "channel_news",
        "财经要闻",
        "NewsChannel",
        "资讯频道"
      ],
      "protocol": "CWServ.tdxi_zxxwnews",
      "kind": "Entry 封装",
      "summary": "按 84 个频道号查询全市场资讯，支持分页和关键词参数。",
      "return_model": "F10Response",
      "doc": "methods/F10-频道资讯.md"
    },
    {
      "id": "f10-news-channels",
      "title": "资讯频道列表",
      "source": "F10",
      "category": "资讯公告",
      "api": "client.f10.news_channels()",
      "aliases": [
        "news_channels",
        "频道号",
        "NEWS_CHANNEL_LABELS"
      ],
      "protocol": "—（静态枚举，不发网络请求）",
      "kind": "静态枚举",
      "summary": "返回 84 个资讯频道号与名称，供 channel_news 查询使用。",
      "return_model": "list[tuple[int, str]]",
      "doc": "methods/F10-资讯频道列表.md"
    },
    {
      "id": "f10-company-news",
      "title": "公司资讯 / 研报",
      "source": "F10",
      "category": "资讯公告",
      "api": "client.f10.company_news()",
      "aliases": [
        "company_news",
        "研报",
        "监管措施"
      ],
      "protocol": "CWServ.tdxf10_gg_gszx",
      "kind": "Entry 封装",
      "summary": "返回研报标题、评级、研究员、日期和地址，也可查询监管措施。",
      "return_model": "F10Response",
      "doc": "methods/F10-公司资讯研报.md"
    },
    {
      "id": "f10-northbound",
      "title": "沪深股通持仓",
      "source": "F10",
      "category": "股东与融资",
      "api": "client.f10.northbound_holding()",
      "aliases": [
        "northbound_holding",
        "北向持仓",
        "沪股通",
        "深股通"
      ],
      "protocol": "CWServ.tdxf10_gg_zlcc",
      "kind": "Entry 封装",
      "summary": "返回沪深股通持股比例、持股数量、变动股数和收盘价等序列。",
      "return_model": "F10Response",
      "doc": "methods/F10-沪深股通持仓.md"
    },
    {
      "id": "f10-detail",
      "title": "详情正文",
      "source": "F10",
      "category": "资讯公告",
      "api": "client.f10.detail()",
      "aliases": [
        "detail",
        "正文",
        "record_id"
      ],
      "protocol": "CWServ.tdxf10_gg_idreq",
      "kind": "Entry 封装",
      "summary": "按记录 ID 返回标题和正文，常用于继续读取题材事件或违规处理详情。",
      "return_model": "F10Response",
      "doc": "methods/F10-详情正文.md"
    },
    {
      "id": "f10-news-cache",
      "title": "新闻 / 公告 / 路演",
      "source": "F10",
      "category": "资讯公告",
      "api": "client.f10.cache_list() / client.f10.news() / client.f10.announcements() / client.f10.roadshows()",
      "calls": [
        {
          "label": "通用调用",
          "api": "client.f10.cache_list(code, kind=...)"
        },
        {
          "label": "新闻",
          "api": "client.f10.news(code)"
        },
        {
          "label": "公告",
          "api": "client.f10.announcements(code)"
        },
        {
          "label": "路演",
          "api": "client.f10.roadshows(code)"
        }
      ],
      "aliases": [
        "news",
        "announcements",
        "roadshows",
        "cache_list",
        "PDF"
      ],
      "protocol": "CWSearch.tzx_rcache",
      "kind": "Entry 封装",
      "summary": "返回新闻、公告和路演列表，包含标题、日期、来源、公告类型和附件地址。",
      "return_model": "F10Response",
      "doc": "methods/F10-新闻公告路演.md"
    },
    {
      "id": "helper-latest-stock-list",
      "title": "最新股票列表",
      "source": "Helper",
      "category": "证券代码",
      "api": "client.helpers.latest_stock_list(market=None)",
      "aliases": [
        "latest_stock_list",
        "latest_stocks",
        "A 股列表"
      ],
      "protocol": "0x044d",
      "kind": "组合能力",
      "summary": "从当前代码表筛出沪深北 A 股证券，返回最新股票列表。",
      "return_model": "list[SecurityCode]",
      "doc": "helpers/最新股票列表.md"
    },
    {
      "id": "helper-latest-st",
      "title": "最新 ST 列表",
      "source": "Helper",
      "category": "证券代码",
      "api": "client.helpers.latest_st(market=None)",
      "aliases": [
        "latest_st",
        "st",
        "ST 股票"
      ],
      "protocol": "0x044d",
      "kind": "组合能力",
      "summary": "按代码表证券名称筛选当前 ST、*ST、SST 和 S*ST 股票。",
      "return_model": "list[SecurityCode]",
      "doc": "helpers/最新ST列表.md"
    },
    {
      "id": "helper-latest-suspended",
      "title": "最新停牌列表",
      "source": "Helper",
      "category": "证券代码",
      "api": "client.helpers.latest_suspended(market=None)",
      "aliases": [
        "latest_suspended",
        "suspended",
        "停牌股票"
      ],
      "protocol": "0x044d + 0x053e",
      "kind": "组合能力",
      "summary": "读取代码表并用 0x053e 交易状态位 0x20 筛出当前停牌股票。",
      "return_model": "list[SecurityCode]",
      "doc": "helpers/最新停牌列表.md"
    },
    {
      "id": "helper-daily-share-capital",
      "title": "每日股本（盘前）",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.helpers.daily_share_capital(codes=None)",
      "aliases": [
        "daily_share_capital",
        "daily_shares",
        "股本"
      ],
      "protocol": "0x0010 + 0x06b9",
      "kind": "组合能力",
      "summary": "合并财务快照与服务器统计资源，返回总股本、流通股本和自由流通股本。",
      "return_model": "DailyShareCapitalTable",
      "doc": "helpers/每日股本.md"
    },
    {
      "id": "helper-daily-price-limits",
      "title": "每日涨跌停价",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.daily_price_limits(codes=None, trade_date=...)",
      "aliases": [
        "daily_price_limits",
        "stock_daily_price_limits",
        "涨跌停"
      ],
      "protocol": "0x054c + 业务规则",
      "kind": "组合能力",
      "summary": "按不复权日线、当日权息和市场板块规则计算当前交易日涨跌停价格。",
      "return_model": "DailyPriceLimitTable",
      "doc": "helpers/每日涨跌停价.md"
    },
    {
      "id": "helper-realtime-rank",
      "title": "实时榜单",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.realtime_rank(sort_by=\"涨幅\")",
      "aliases": [
        "realtime_rank",
        "stock_realtime_rank",
        "涨幅榜"
      ],
      "protocol": "0x054b",
      "kind": "业务封装",
      "summary": "基于 list_by_category() 的榜单封装：自动分页、补名称并标准化为带排名的实时结果。",
      "return_model": "RealtimeRankTable",
      "doc": "helpers/实时榜单.md"
    },
    {
      "id": "helper-board-quotes",
      "title": "板块行情",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.board_quotes(category=\"概念\", refresh=False)",
      "aliases": [
        "board_quotes",
        "板块行情"
      ],
      "protocol": "0x06b9 + 0x044d + 0x054c",
      "kind": "组合能力",
      "summary": "默认读取概念板块资料，也可通过 category 选择其他分类或全部板块；按每批最多 80 个代码取得行情，不自行排序。",
      "return_model": "BoardQuoteTable",
      "doc": "helpers/板块行情.md"
    },
    {
      "id": "helper-board-member-quotes",
      "title": "板块成分股行情",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.board_member_quotes(board_code, refresh=False)",
      "aliases": [
        "board_member_quotes",
        "板块成分股行情"
      ],
      "protocol": "0x06b9 + 0x044d + 0x054c",
      "kind": "组合能力",
      "summary": "按板块代码读取原始成员，用当前 0x044d 证券名单核对有效性，再分批取得成分股行情。",
      "return_model": "BoardMemberQuoteTable",
      "doc": "helpers/板块成分股行情.md"
    },
    {
      "id": "helper-buy-sell-strength",
      "title": "买卖力道",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.buy_sell_strength(code)",
      "aliases": [
        "buy_sell_strength",
        "买卖力道"
      ],
      "protocol": "0x051b",
      "kind": "组合能力",
      "summary": "返回按分钟整理的买卖力道副图序列。",
      "return_model": "MinuteAuxSeries",
      "doc": "helpers/买卖力道.md"
    },
    {
      "id": "helper-volume-comparison",
      "title": "成交对比",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.volume_comparison(code)",
      "aliases": [
        "volume_comparison",
        "成交对比"
      ],
      "protocol": "0x051b",
      "kind": "组合能力",
      "summary": "返回当前与前一交易日累计成交量对比的分钟副图序列。",
      "return_model": "MinuteAuxSeries",
      "doc": "helpers/成交对比.md"
    },
    {
      "id": "helper-limit-ladder",
      "title": "实时涨停梯队",
      "source": "Helper",
      "category": "实时行情",
      "api": "client.helpers.limit_ladder(codes=None)",
      "aliases": [
        "limit_ladder",
        "stock_limit_ladder",
        "实时涨停",
        "连板"
      ],
      "protocol": "实时快照 + 0x06b9",
      "kind": "组合能力",
      "summary": "从实时短线指标中筛选封板或触板股票，按涨停梯队和封单金额排序。",
      "return_model": "LimitLadderTable",
      "doc": "helpers/连板天梯.md"
    },
    {
      "id": "helper-theme-strength-rank",
      "title": "题材强度排行",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.helpers.theme_strength_rank(codes=None)",
      "aliases": [
        "theme_strength_rank",
        "stock_theme_strength_rank",
        "题材强度"
      ],
      "protocol": "连板天梯 + 7615 F10",
      "kind": "组合能力",
      "summary": "按个股 F10 题材聚合涨停数量、最高板、连板数和封单金额。",
      "return_model": "ThemeStrengthTable",
      "doc": "helpers/题材强度排行.md"
    },
    {
      "id": "helper-stock-profile",
      "title": "股票信息汇总",
      "source": "Helper",
      "category": "股票与行情",
      "api": "client.helpers.stock_profile_table(codes)",
      "aliases": [
        "stock_profile_table",
        "股票表头",
        "流通市值"
      ],
      "protocol": "0x054c + 0x044d + 0x0010",
      "kind": "组合能力",
      "summary": "合并行情快照、代码表和财务基础信息，形成股票表头信息。",
      "return_model": "StockProfileTable",
      "doc": "helpers/股票信息汇总.md"
    },
    {
      "id": "helper-shortline-indicators",
      "title": "短线指标",
      "source": "Helper",
      "category": "股票与行情",
      "api": "client.helpers.shortline_indicators(codes)",
      "aliases": [
        "shortline_indicators",
        "流通股本Z",
        "流通市值Z",
        "开盘换手Z",
        "竞价昨比",
        "开盘昨封比",
        "昨封比",
        "封流比",
        "几天几板"
      ],
      "protocol": "0x06b9 + 0x054c + 0x0547 + 0x044d + 0x052d",
      "kind": "组合能力",
      "summary": "按交易日安全对齐统计资源、实时行情、近 5 日 K 线和财务快照，返回竞价、开盘量比、流通股本、封单和连板字段。",
      "return_model": "ShortlineIndicatorTable",
      "doc": "helpers/短线指标.md"
    },
    {
      "id": "helper-stock-topics",
      "title": "个股概念板块",
      "source": "Helper",
      "category": "题材与概念",
      "api": "client.helpers.stock_topics(code)",
      "aliases": [
        "stock_topics",
        "个股题材"
      ],
      "protocol": "F10 多 Entry",
      "kind": "组合能力",
      "summary": "合并股票基础信息和热点题材结果，整理某只股票的全部题材与概念。",
      "return_model": "StockTopics",
      "doc": "helpers/个股概念板块.md"
    },
    {
      "id": "helper-topic-stocks",
      "title": "概念板块成分股",
      "source": "Helper",
      "category": "题材与概念",
      "api": "client.helpers.topic_stocks(seed_code, ...)",
      "aliases": [
        "topic_stocks",
        "题材成分股"
      ],
      "protocol": "F10 多 Entry",
      "kind": "组合能力",
      "summary": "按题材 ID 或名称查询概念板块成分股，并整理排名和区间涨跌幅。",
      "return_model": "TopicStockTable",
      "doc": "helpers/概念板块成分股.md"
    },
    {
      "id": "helper-auction-data",
      "title": "集合竞价数据",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.helpers.auction_data(code, date=None)",
      "aliases": [
        "auction_data",
        "开盘涨幅",
        "开盘金额"
      ],
      "protocol": "当日 0x056a + 0x0fc5 + 0x054c / 历史 0x056a + 0x0fc6",
      "kind": "组合能力",
      "summary": "不传日期时合并 0x056A 竞价过程、0x0FC5 正式撮合和 0x054C 前收盘参考价；传日期时使用 0x056A 查询历史竞价过程，并通过 0x0FC6 自动分页取得正式撮合和前收盘参考价。",
      "return_model": "AuctionData",
      "doc": "helpers/竞价数据.md"
    },
    {
      "id": "helper-server-download",
      "title": "服务器文件下载",
      "source": "Helper",
      "category": "服务器资源",
      "api": "client.resources.download_file(path, ...)",
      "aliases": [
        "download_file",
        "服务器文件下载"
      ],
      "protocol": "基于 0x06b9",
      "kind": "协议封装",
      "summary": "循环读取服务器文件块并合并为完整 bytes。",
      "return_model": "bytes",
      "doc": "methods/7709-服务器文件下载.md"
    },
    {
      "id": "helper-server-stats",
      "title": "服务器统计文件解析",
      "source": "Helper",
      "category": "服务器资源",
      "api": "client.resources.read_stats(path=..., ...)",
      "aliases": [
        "read_stats",
        "zhb.zip",
        "tdxstat",
        "统计文件"
      ],
      "protocol": "基于 0x06b9",
      "kind": "协议封装",
      "summary": "下载并解析 zhb.zip 中的 tdxstat.cfg 与 tdxstat2.cfg。",
      "return_model": "TdxStatsResource",
      "doc": "methods/7709-服务器统计文件解析.md",
      "doc_anchor": "stats-resource"
    },
    {
      "id": "7709-block-meta",
      "title": "板块文件元数据",
      "source": "7709",
      "category": "服务器资源",
      "api": "client.resources.block_meta(filename)",
      "aliases": [
        "block_meta",
        "板块文件元数据",
        "板块文件"
      ],
      "protocol": "0x02c5",
      "kind": "底层协议",
      "summary": "查询板块文件在服务器上的字节大小与 MD5 摘要，用于判断本地缓存是否需要更新。",
      "return_model": "BlockMeta",
      "doc": "methods/0x02c5-板块文件元数据接口.md"
    },
    {
      "id": "7709-company-info-category",
      "title": "公司信息目录",
      "source": "7709",
      "category": "公司资料",
      "api": "client.company_info.category(code, market)",
      "aliases": [
        "company_info_category",
        "公司信息目录",
        "F10 目录"
      ],
      "protocol": "0x02cf",
      "kind": "底层协议",
      "summary": "读取证券 F10 公司信息文件的段落目录（名称、文件名、偏移、长度）。",
      "return_model": "list[CompanyInfoCategory]",
      "doc": "methods/0x02cf-公司信息目录接口.md"
    },
    {
      "id": "7709-company-info-content",
      "title": "公司信息内容",
      "source": "7709",
      "category": "公司资料",
      "api": "client.company_info.content(code, market, filename, offset, length)",
      "aliases": [
        "company_info_content",
        "公司信息内容",
        "F10 正文"
      ],
      "protocol": "0x02d0",
      "kind": "底层协议",
      "summary": "按目录读取某只证券 F10 公司信息的一段 GBK 正文。",
      "return_model": "str",
      "doc": "methods/0x02d0-公司信息内容接口.md"
    },
    {
      "id": "ex-connection",
      "title": "EX 连接与主机管理",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient(...)",
      "aliases": [
        "ExTdxClient",
        "connect",
        "close",
        "from_best_host",
        "ping_all"
      ],
      "protocol": "连接管理 / 7727",
      "kind": "客户端管理",
      "summary": "连接、主机探测、故障处理及生命周期。",
      "return_model": "ExTdxClient",
      "doc": "extended/ex-连接与主机.md"
    },
    {
      "id": "ex-markets",
      "title": "EX 市场列表",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_markets(...)",
      "aliases": [
        "ExTdxClient",
        "0x23F4",
        "get_markets"
      ],
      "protocol": "0x23F4 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "查询服务器提供的市场描述表。",
      "return_model": "list[ExMarketInfo]",
      "doc": "extended/ex-get_markets.md"
    },
    {
      "id": "ex-count",
      "title": "EX 商品总数",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_instrument_count(...)",
      "aliases": [
        "ExTdxClient",
        "0x23F0",
        "get_instrument_count"
      ],
      "protocol": "0x23F0 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取全市场商品总数。",
      "return_model": "int",
      "doc": "extended/ex-get_instrument_count.md"
    },
    {
      "id": "ex-info",
      "title": "EX 商品信息",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_instrument_info(...)",
      "aliases": [
        "ExTdxClient",
        "0x23F5",
        "get_instrument_info"
      ],
      "protocol": "0x23F5 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "分页读取全局商品元数据，包括市场、代码、名称和类别。",
      "return_model": "list[ExInstrumentInfo]",
      "doc": "extended/ex-get_instrument_info.md"
    },
    {
      "id": "ex-quote",
      "title": "EX 单商品五档报价",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_instrument_quote(...)",
      "aliases": [
        "ExTdxClient",
        "0x23FA",
        "get_instrument_quote"
      ],
      "protocol": "0x23FA / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "查询单个扩展商品的价格、成交与五档盘口。",
      "return_model": "ExInstrumentQuote | None",
      "doc": "extended/ex-get_instrument_quote.md"
    },
    {
      "id": "ex-quotes",
      "title": "EX 分类报价列表",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_instrument_quote_list(...)",
      "aliases": [
        "ExTdxClient",
        "0x2400",
        "get_instrument_quote_list"
      ],
      "protocol": "0x2400 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "分页读取扩展市场分类报价。",
      "return_model": "list[OrderedDict[str, object]]",
      "doc": "extended/ex-get_instrument_quote_list.md"
    },
    {
      "id": "ex-bars",
      "title": "EX 周期K线",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_instrument_bars(...)",
      "aliases": [
        "ExTdxClient",
        "0x23FF",
        "get_instrument_bars"
      ],
      "protocol": "0x23FF / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "按周期和偏移查询扩展商品 K 线。",
      "return_model": "list[ExInstrumentBar]",
      "doc": "extended/ex-get_instrument_bars.md"
    },
    {
      "id": "ex-history-bars",
      "title": "EX 日期范围历史K线",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_history_instrument_bars_range(...)",
      "aliases": [
        "ExTdxClient",
        "0x240D",
        "get_history_instrument_bars_range"
      ],
      "protocol": "0x240D / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "按起止日期请求一段历史 K 线。",
      "return_model": "list[ExInstrumentBar]",
      "doc": "extended/ex-get_history_instrument_bars_range.md"
    },
    {
      "id": "ex-minute",
      "title": "EX 当日分时",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_minute_time_data(...)",
      "aliases": [
        "ExTdxClient",
        "0x240B",
        "get_minute_time_data"
      ],
      "protocol": "0x240B / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取扩展商品当前日内分时点。",
      "return_model": "list[ExMinuteBar]",
      "doc": "extended/ex-get_minute_time_data.md"
    },
    {
      "id": "ex-history-minute",
      "title": "EX 历史分时",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_history_minute_time_data(...)",
      "aliases": [
        "ExTdxClient",
        "0x240C",
        "get_history_minute_time_data"
      ],
      "protocol": "0x240C / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取指定日期的扩展商品分时点。",
      "return_model": "list[ExMinuteBar]",
      "doc": "extended/ex-get_history_minute_time_data.md"
    },
    {
      "id": "ex-trades",
      "title": "EX 当日成交明细",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_transaction_data(...)",
      "aliases": [
        "ExTdxClient",
        "0x23FC",
        "get_transaction_data"
      ],
      "protocol": "0x23FC / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "单页查询扩展商品当日成交明细。",
      "return_model": "list[ExTransactionRecord]",
      "doc": "extended/ex-get_transaction_data.md"
    },
    {
      "id": "ex-history-trades",
      "title": "EX 历史成交明细",
      "source": "EX",
      "category": "扩展行情（7727）",
      "api": "ExTdxClient.get_history_transaction_data(...)",
      "aliases": [
        "ExTdxClient",
        "0x2406",
        "get_history_transaction_data"
      ],
      "protocol": "0x2406 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "单页读取指定日期的扩展商品成交明细。",
      "return_model": "list[ExTransactionRecord]",
      "doc": "extended/ex-get_history_transaction_data.md"
    },
    {
      "id": "mac-connection",
      "title": "MAC 连接与主机管理",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient(...)",
      "aliases": [
        "MacClient",
        "connect",
        "close",
        "from_best_host",
        "ping_all"
      ],
      "protocol": "连接管理 / 7709",
      "kind": "客户端管理",
      "summary": "连接、主机探测、故障处理及生命周期。",
      "return_model": "MacClient",
      "doc": "extended/mac-连接与主机.md"
    },
    {
      "id": "mac-quotes",
      "title": "MAC 批量报价",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_stock_quotes(...)",
      "aliases": [
        "MacClient",
        "0x122B",
        "get_stock_quotes"
      ],
      "protocol": "0x122B / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "用字段位图一次读取多只证券的报价和扩展指标。",
      "return_model": "list[MacQuoteField]",
      "doc": "extended/mac-get_stock_quotes.md"
    },
    {
      "id": "mac-ranking",
      "title": "MAC 分类排行",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_stock_quotes_list(...)",
      "aliases": [
        "MacClient",
        "0x122C",
        "get_stock_quotes_list"
      ],
      "protocol": "0x122C / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "按市场分类读取服务器排序后的报价列表。",
      "return_model": "list[MacQuoteField]",
      "doc": "extended/mac-get_stock_quotes_list.md"
    },
    {
      "id": "mac-bars",
      "title": "MAC 复权K线",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_stock_kline(...)",
      "aliases": [
        "MacClient",
        "0x122E",
        "get_stock_kline"
      ],
      "protocol": "0x122E / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取多周期 K 线，支持服务端复权和自动分页。",
      "return_model": "list[MacBar]",
      "doc": "extended/mac-get_stock_kline.md"
    },
    {
      "id": "mac-minute",
      "title": "MAC 单日分时",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_tick_chart(...)",
      "aliases": [
        "MacClient",
        "0x122D",
        "get_tick_chart",
        "get_stock_minute_time"
      ],
      "protocol": "0x122D / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "获取某交易日的分时曲线及日内汇总。",
      "return_model": "MacTickChart",
      "doc": "extended/mac-get_tick_chart.md"
    },
    {
      "id": "mac-get-tick-charts",
      "title": "MAC 多日分时",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_tick_charts(...)",
      "aliases": [
        "MacClient",
        "0x123E",
        "get_tick_charts",
        "get_stock_minute_time_hist"
      ],
      "protocol": "0x123E / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "一次获取最多五个交易日的分时曲线。",
      "return_model": "MacMultiTickChart",
      "doc": "extended/mac-get_tick_charts.md"
    },
    {
      "id": "mac-get-chart-sampling",
      "title": "MAC 分时缩略采样",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_chart_sampling(...)",
      "aliases": [
        "MacClient",
        "0x254D",
        "get_chart_sampling"
      ],
      "protocol": "0x254D / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取用于小走势图的价格采样序列。",
      "return_model": "list[float]",
      "doc": "extended/mac-get_chart_sampling.md"
    },
    {
      "id": "mac-trades",
      "title": "MAC 成交明细",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_transactions(...)",
      "aliases": [
        "MacClient",
        "0x122F",
        "get_transactions",
        "get_stock_transaction"
      ],
      "protocol": "0x122F / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "查询当日或指定日期的带秒成交明细；支持自动分页。",
      "return_model": "list[MacTransaction]",
      "doc": "extended/mac-get_transactions.md"
    },
    {
      "id": "mac-get-symbol-info",
      "title": "MAC 个股特征快照",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_symbol_info(...)",
      "aliases": [
        "MacClient",
        "0x122A",
        "get_symbol_info"
      ],
      "protocol": "0x122A / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取证券身份、行情时间、价格、成交与内外盘等简要特征。",
      "return_model": "MacSymbolInfo",
      "doc": "extended/mac-get_symbol_info.md"
    },
    {
      "id": "mac-boards",
      "title": "MAC 板块列表",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_board_list(...)",
      "aliases": [
        "MacClient",
        "0x1231",
        "get_board_list"
      ],
      "protocol": "0x1231 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "枚举板块，并附带板块和领涨证券的价格信息。",
      "return_model": "list[BoardInfo]",
      "doc": "extended/mac-get_board_list.md"
    },
    {
      "id": "mac-get-board-members",
      "title": "MAC 板块成分股报价",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_board_members(...)",
      "aliases": [
        "MacClient",
        "0x122C",
        "get_board_members"
      ],
      "protocol": "0x122C / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "按用户板块代码读取成分证券的排序报价。",
      "return_model": "list[MacQuoteField]",
      "doc": "extended/mac-get_board_members.md"
    },
    {
      "id": "mac-get-belong-board",
      "title": "MAC 个股所属板块",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_belong_board(...)",
      "aliases": [
        "MacClient",
        "0x1218 / head_flag=1",
        "get_belong_board"
      ],
      "protocol": "0x1218 / head_flag=1 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "反向查询一只证券所属的板块。",
      "return_model": "list[BelongBoardInfo]",
      "doc": "extended/mac-get_belong_board.md"
    },
    {
      "id": "mac-get-board-summary",
      "title": "MAC 板块汇总",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_board_summary(...)",
      "aliases": [
        "MacClient",
        "0x122C / 本地聚合",
        "get_board_summary"
      ],
      "protocol": "0x122C / 本地聚合 / 7709",
      "kind": "同步客户端 / 组合调用",
      "summary": "取全部成分股报价，在本地汇总成交额、资金流及涨跌家数。",
      "return_model": "dict[str, object]",
      "doc": "extended/mac-get_board_summary.md"
    },
    {
      "id": "mac-get-board-ranking",
      "title": "MAC 板块排行榜",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_board_ranking(...)",
      "aliases": [
        "MacClient",
        "0x1231 + 0x122C / 本地聚合",
        "get_board_ranking"
      ],
      "protocol": "0x1231 + 0x122C / 本地聚合 / 7709",
      "kind": "同步客户端 / 组合调用",
      "summary": "按涨幅、成交额、成交量或主力净额排列板块。",
      "return_model": "list[dict[str, object]]",
      "doc": "extended/mac-get_board_ranking.md"
    },
    {
      "id": "mac-get-board-change-ranking",
      "title": "MAC 板块多日涨幅排行",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_board_change_ranking(...)",
      "aliases": [
        "MacClient",
        "0x1231 + 0x122E / 本地计算",
        "get_board_change_ranking"
      ],
      "protocol": "0x1231 + 0x122E / 本地计算 / 7709",
      "kind": "同步客户端 / 组合调用",
      "summary": "从板块日线计算窗口涨幅后在本地排序。",
      "return_model": "list[dict[str, object]]",
      "doc": "extended/mac-get_board_change_ranking.md"
    },
    {
      "id": "mac-get-capital-flow",
      "title": "MAC 个股资金流",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_capital_flow(...)",
      "aliases": [
        "MacClient",
        "0x1218 / head_flag=2",
        "get_capital_flow"
      ],
      "protocol": "0x1218 / head_flag=2 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取 Stock_ZJLX 数据并整理为资金流模型。",
      "return_model": "CapitalFlowData | None",
      "doc": "extended/mac-get_capital_flow.md"
    },
    {
      "id": "mac-get-auction",
      "title": "MAC 集合竞价",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_auction(...)",
      "aliases": [
        "MacClient",
        "0x123D",
        "get_auction"
      ],
      "protocol": "0x123D / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取集合竞价过程中的价格、匹配量及带符号未匹配量。",
      "return_model": "list[AuctionItem]",
      "doc": "extended/mac-get_auction.md"
    },
    {
      "id": "mac-get-unusual",
      "title": "MAC 市场盘口异动",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_unusual(...)",
      "aliases": [
        "MacClient",
        "0x1237",
        "get_unusual"
      ],
      "protocol": "0x1237 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取市场异动事件及解析后的说明文字。",
      "return_model": "list[UnusualItem]",
      "doc": "extended/mac-get_unusual.md"
    },
    {
      "id": "mac-get-server-info",
      "title": "MAC 服务器交易时段",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_server_info(...)",
      "aliases": [
        "MacClient",
        "0x120F",
        "get_server_info"
      ],
      "protocol": "0x120F / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取服务器日期、最近交易日和两组交易时段参数。",
      "return_model": "ServerSession",
      "doc": "extended/mac-get_server_info.md"
    },
    {
      "id": "mac-get-kline-offset",
      "title": "MAC K线索引偏移",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_kline_offset(...)",
      "aliases": [
        "MacClient",
        "0x124A",
        "get_kline_offset"
      ],
      "protocol": "0x124A / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取 K 线索引数据的总量和返回量。",
      "return_model": "KlineOffsetInfo",
      "doc": "extended/mac-get_kline_offset.md"
    },
    {
      "id": "mac-get-file-meta",
      "title": "MAC 服务器文件元信息",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_file_meta(...)",
      "aliases": [
        "MacClient",
        "0x1215",
        "get_file_meta"
      ],
      "protocol": "0x1215 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "查询远端文件的偏移、大小、标志和哈希信息。",
      "return_model": "FileMeta",
      "doc": "extended/mac-get_file_meta.md"
    },
    {
      "id": "mac-download-file-chunk",
      "title": "MAC 服务器文件分片",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.download_file_chunk(...)",
      "aliases": [
        "MacClient",
        "0x1217",
        "download_file_chunk"
      ],
      "protocol": "0x1217 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "下载远端文件的一段原始字节。",
      "return_model": "bytes",
      "doc": "extended/mac-download_file_chunk.md"
    },
    {
      "id": "mac-download-file",
      "title": "MAC 服务器文件完整下载",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.download_file(...)",
      "aliases": [
        "MacClient",
        "0x1215 + 0x1217",
        "download_file"
      ],
      "protocol": "0x1215 + 0x1217 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "查询长度后循环下载，并在内存中拼接文件。",
      "return_model": "bytearray",
      "doc": "extended/mac-download_file.md"
    },
    {
      "id": "mac-get-goods-list",
      "title": "MAC 商品列表",
      "source": "MAC",
      "category": "MAC 行情",
      "api": "MacClient.get_goods_list(...)",
      "aliases": [
        "MacClient",
        "0x2562",
        "get_goods_list"
      ],
      "protocol": "0x2562 / 7709",
      "kind": "同步客户端 / 协议接口",
      "summary": "向当前 MAC 主机请求扩展商品列表。",
      "return_model": "list[GoodsItem]",
      "doc": "extended/mac-get_goods_list.md"
    },
    {
      "id": "macex-connection",
      "title": "MAC-EX 连接与主机管理",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient(...)",
      "aliases": [
        "MacExClient",
        "connect",
        "close",
        "from_best_host",
        "ping_all"
      ],
      "protocol": "连接管理 / 7727",
      "kind": "客户端管理",
      "summary": "连接、主机探测、故障处理及生命周期。",
      "return_model": "MacExClient",
      "doc": "extended/macex-连接与主机.md"
    },
    {
      "id": "macex-instruments",
      "title": "MAC-EX 标的元数据",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.instruments(...)",
      "aliases": [
        "MacExClient",
        "0x23F0 + 0x23F5",
        "instruments"
      ],
      "protocol": "0x23F0 + 0x23F5 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取某个扩展市场内的代码、名称、类别及描述。",
      "return_model": "list[ExInstrumentInfo]",
      "doc": "extended/macex-instruments.md"
    },
    {
      "id": "macex-goods-count",
      "title": "MAC-EX 商品数量",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_count(...)",
      "aliases": [
        "MacExClient",
        "0x23F0 / 0x2562 / 0x23F5",
        "goods_count"
      ],
      "protocol": "0x23F0 / 0x2562 / 0x23F5 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "获取全市场或指定市场的商品数量。",
      "return_model": "int",
      "doc": "extended/macex-goods_count.md"
    },
    {
      "id": "macex-goods-list",
      "title": "MAC-EX 商品列表",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_list(...)",
      "aliases": [
        "MacExClient",
        "0x2562 / 0x23F5",
        "goods_list"
      ],
      "protocol": "0x2562 / 0x23F5 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取市场商品列表，MAC 列表为空时回退 EX 元数据。",
      "return_model": "list[GoodsItem]",
      "doc": "extended/macex-goods_list.md"
    },
    {
      "id": "macex-quotes",
      "title": "MAC-EX 批量报价与净值",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_quotes(...)",
      "aliases": [
        "MacExClient",
        "0x122B",
        "goods_quotes"
      ],
      "protocol": "0x122B / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "按市场和代码批量读取基金净值或外盘行情。",
      "return_model": "list[MacQuoteField]",
      "doc": "extended/macex-goods_quotes.md"
    },
    {
      "id": "macex-goods-quotes-list",
      "title": "MAC-EX 列表报价",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_quotes_list(...)",
      "aliases": [
        "MacExClient",
        "0x2562 / 0x23F5 + 0x122B",
        "goods_quotes_list"
      ],
      "protocol": "0x2562 / 0x23F5 + 0x122B / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "枚举一段商品代码后读取基础报价。",
      "return_model": "list[MacQuoteField]",
      "doc": "extended/macex-goods_quotes_list.md"
    },
    {
      "id": "macex-bars",
      "title": "MAC-EX 扩展市场K线",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_kline(...)",
      "aliases": [
        "MacExClient",
        "0x122E",
        "goods_kline"
      ],
      "protocol": "0x122E / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取基金、外盘、期货等扩展品种的 K 线。",
      "return_model": "list[MacBar]",
      "doc": "extended/macex-goods_kline.md"
    },
    {
      "id": "macex-minute",
      "title": "MAC-EX 单日分时",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_tick_chart(...)",
      "aliases": [
        "MacExClient",
        "0x122D",
        "goods_tick_chart"
      ],
      "protocol": "0x122D / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取扩展市场某日分时曲线。",
      "return_model": "MacTickChart",
      "doc": "extended/macex-goods_tick_chart.md"
    },
    {
      "id": "macex-goods-chart-sampling",
      "title": "MAC-EX 分时缩略采样",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_chart_sampling(...)",
      "aliases": [
        "MacExClient",
        "0x254D",
        "goods_chart_sampling"
      ],
      "protocol": "0x254D / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取扩展品种的小走势图价格点。",
      "return_model": "list[float]",
      "doc": "extended/macex-goods_chart_sampling.md"
    },
    {
      "id": "macex-trades",
      "title": "MAC-EX 成交明细",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_transaction(...)",
      "aliases": [
        "MacExClient",
        "0x122F / 0x23FC / 0x2406",
        "goods_transaction"
      ],
      "protocol": "0x122F / 0x23FC / 0x2406 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "按市场路由到 MAC 成交或港股专用 EX 成交，返回统一模型。",
      "return_model": "list[MacTransaction]",
      "doc": "extended/macex-goods_transaction.md"
    },
    {
      "id": "macex-goods-transaction-all",
      "title": "MAC-EX 港股全量成交",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.goods_transaction_all(...)",
      "aliases": [
        "MacExClient",
        "0x23FC / 0x2406",
        "goods_transaction_all"
      ],
      "protocol": "0x23FC / 0x2406 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "为港股股票类市场逐页读取当日或指定日成交。",
      "return_model": "list[MacTransaction]",
      "doc": "extended/macex-goods_transaction_all.md"
    },
    {
      "id": "macex-macro-indicators",
      "title": "MAC-EX 宏观指标列表",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.macro_indicators(...)",
      "aliases": [
        "MacExClient",
        "0x2562 / 市场38",
        "macro_indicators"
      ],
      "protocol": "0x2562 / 市场38 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "枚举并缓存市场 38 的宏观指标代码。",
      "return_model": "list[GoodsItem]",
      "doc": "extended/macex-macro_indicators.md"
    },
    {
      "id": "macex-macro-quotes",
      "title": "MAC-EX 宏观指标最新值",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.macro_quotes(...)",
      "aliases": [
        "MacExClient",
        "0x122B / 市场38",
        "macro_quotes"
      ],
      "protocol": "0x122B / 市场38 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "读取宏观指标最新报告值及前值等字段。",
      "return_model": "list[MacQuoteField]",
      "doc": "extended/macex-macro_quotes.md"
    },
    {
      "id": "macex-macro-kline",
      "title": "MAC-EX 宏观指标历史序列",
      "source": "MAC-EX",
      "category": "MAC-EX 基金与外盘",
      "api": "MacExClient.macro_kline(...)",
      "aliases": [
        "MacExClient",
        "0x122E / 市场38",
        "macro_kline"
      ],
      "protocol": "0x122E / 市场38 / 7727",
      "kind": "同步客户端 / 协议接口",
      "summary": "借用 K 线模型读取宏观指标的历史报告期序列。",
      "return_model": "list[MacBar]",
      "doc": "extended/macex-macro_kline.md"
    }
  ]
};
