window.ELTDX_CATALOG = {
  "schema_version": 11,
  "taxonomy": {
    "layers": [
      {
        "id": "7709",
        "label": "7709 原生协议接口",
        "tag_label": "7709 原生",
        "stat_label": "7709 原生协议接口",
        "description": "21 个 7709 原生协议接口，每项对应一个实际二进制命令。",
        "source": "7709"
      },
      {
        "id": "7615",
        "label": "7615 原生 Entry 接口",
        "tag_label": "7615 原生 Entry",
        "stat_label": "7615 原生 Entry 接口",
        "description": "21 个 7615 原生 Entry 接口，对应 TQLEX/F10 的 HTTP Entry 与功能调用。",
        "source": "F10"
      },
      {
        "id": "helpers",
        "label": "Helpers 封装",
        "tag_label": "Helpers",
        "stat_label": "Helpers 封装",
        "description": "15 个 Helpers 封装，组合协议调用、分页、解析、整理和本地计算。",
        "source": "Helper"
      }
    ],
    "functional_groups": [
      {
        "id": "basics",
        "label": "基础接口（非手动调用）",
        "description": "连接时自动握手，长连接由后台定时保活，一般不需要手动调用。",
        "categories": [
          "会话"
        ]
      },
      {
        "id": "codes",
        "label": "证券代码",
        "description": "查询市场证券数量，以及分页或全量读取证券代码表。",
        "categories": [
          "证券代码"
        ]
      },
      {
        "id": "entry",
        "label": "高级调用（需手动指定 Entry 和参数）",
        "description": "用户主动指定 Entry 和参数，直接调用 7615/TQLEX HTTP 网关；适合调试或访问尚未封装的资料功能。",
        "categories": [
          "网关"
        ]
      },
      {
        "id": "quotes",
        "label": "实时行情",
        "description": "快照、五档、分时、逐笔成交和面向行情的组合能力。",
        "categories": [
          "行情",
          "行情封装",
          "分时",
          "逐笔成交"
        ],
        "item_ids": [
          "helper-stock-profile",
          "helper-quote-table",
          "helper-shortline-indicators"
        ]
      },
      {
        "id": "bars",
        "label": "K 线与复权",
        "description": "周期 K 线、历史分页，以及前复权、后复权和定点复权。",
        "categories": [
          "K 线"
        ],
        "item_ids": [
          "helper-adjusted-kline"
        ]
      },
      {
        "id": "auction",
        "label": "集合竞价",
        "description": "竞价过程、09:25 成交快照和开盘数据整理。",
        "categories": [
          "集合竞价"
        ],
        "item_ids": [
          "helper-auction-data"
        ]
      },
      {
        "id": "company",
        "label": "公司与财务",
        "description": "股本、除权、财务基础信息和公司资料。",
        "categories": [
          "公司基础",
          "公司资料",
          "经营财务",
          "股东与融资",
          "公司治理"
        ]
      },
      {
        "id": "topics",
        "label": "题材与排名",
        "description": "题材行情、概念成分、估值和市场/行业排名。",
        "categories": [
          "题材行情",
          "题材与概念",
          "评价排名",
          "估值行情"
        ],
        "item_ids": [
          "helper-stock-topics",
          "helper-topic-stocks"
        ]
      },
      {
        "id": "news",
        "label": "资讯公告",
        "description": "新闻、公告、研报、路演和详情正文。",
        "categories": [
          "资讯公告"
        ]
      },
      {
        "id": "limits",
        "label": "交易限制",
        "description": "特殊品种的涨跌停限制数据。",
        "categories": [
          "交易限制"
        ]
      },
      {
        "id": "resources",
        "label": "服务器资源",
        "description": "服务器文件分块读取、完整下载和统计资源解析。",
        "categories": [
          "服务器资源"
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
      "summary": "无游标的一次性基础快照，返回现价、成交量额和已确认的一档盘口。",
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
        "板块行情"
      ],
      "protocol": "0x054b",
      "kind": "底层协议",
      "summary": "按市场或板块分页返回行情列表，并支持按涨幅、价格、成交额等服务端排序。",
      "return_model": "CategoryQuotePage",
      "doc": "methods/7709-分类行情.md"
    },
    {
      "id": "7709-quote-refresh",
      "title": "增量刷新 / 推送队列",
      "source": "7709",
      "category": "行情",
      "api": "client.quotes.get_depth() / client.quotes.refresh() / client.quotes.poll_push() / client.quotes.drain_pushes()",
      "calls": [
        {
          "label": "五档快捷入口",
          "api": "client.quotes.get_depth(codes)"
        },
        {
          "label": "行情刷新",
          "api": "client.quotes.refresh(...)"
        },
        {
          "label": "读取推送",
          "api": "client.quotes.poll_push(...)"
        },
        {
          "label": "清空队列",
          "api": "client.quotes.drain_pushes(...)"
        }
      ],
      "aliases": [
        "refresh",
        "get_depth",
        "poll_push",
        "drain_pushes",
        "push queue"
      ],
      "protocol": "0x0547",
      "kind": "底层协议",
      "summary": "按代码和游标刷新行情，单次最多 100 只；用于增量更新并配合推送队列。",
      "return_model": "QuoteRefreshPage",
      "doc": "methods/7709-增量刷新推送队列.md"
    },
    {
      "id": "7709-kline",
      "title": "K 线 / 周期线",
      "source": "7709",
      "category": "K 线",
      "api": "client.bars.get(code, ...)",
      "aliases": [
        "bars.get",
        "OHLC",
        "前复权",
        "后复权",
        "ETF",
        "指数"
      ],
      "protocol": "0x052d",
      "kind": "底层协议",
      "summary": "返回分钟、日、周、月、季、年 K 线，支持服务端复权和不复权参数。",
      "return_model": "KlineSeries",
      "doc": "methods/7709-K线周期线.md"
    },
    {
      "id": "7709-kline-all",
      "title": "全量 K 线分页",
      "source": "Helper",
      "category": "K 线",
      "api": "client.bars.all(code, ...)",
      "aliases": [
        "bars.all",
        "历史 K 线"
      ],
      "protocol": "0x052d",
      "kind": "协议封装",
      "summary": "自动连续分页并合并 K 线，适合补充历史日线或分钟线数据。",
      "return_model": "KlineSeries",
      "doc": "methods/7709-全量K线分页.md"
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
      "api": "client.trades.today(code, ...) / client.trades.all_today(code, ...)",
      "calls": [
        {
          "label": "主要调用",
          "api": "client.trades.today(code, ...)"
        },
        {
          "label": "完整分页",
          "api": "client.trades.all_today(code, ...)"
        }
      ],
      "aliases": [
        "trades.today",
        "ticks",
        "逐笔"
      ],
      "protocol": "0x0fc5",
      "kind": "底层协议",
      "summary": "返回主站当前保存的混合记录：普通成交、status=8 竞价快照和 09:25 正式撮合。",
      "return_model": "TradePage",
      "doc": "methods/7709-当日成交明细.md"
    },
    {
      "id": "7709-trades-history",
      "title": "历史成交明细",
      "source": "7709",
      "category": "逐笔成交",
      "api": "client.trades.history(code, date, ...) / client.trades.all_history(code, date, ...)",
      "calls": [
        {
          "label": "主要调用 · 单页",
          "api": "client.trades.history(code, date, ...)"
        },
        {
          "label": "完整分页",
          "api": "client.trades.all_history(code, date, ...)"
        }
      ],
      "aliases": [
        "trades.history",
        "历史逐笔"
      ],
      "protocol": "0x0fc6",
      "kind": "底层协议",
      "summary": "返回指定日期混合记录：普通成交、status=8 竞价快照和 09:25 正式撮合。",
      "return_model": "TradePage",
      "doc": "methods/7709-历史成交明细.md"
    },
    {
      "id": "7709-auction-series",
      "title": "当日集合竞价明细",
      "source": "7709",
      "category": "集合竞价",
      "api": "client.auctions.series(code)",
      "aliases": [
        "auctions.series",
        "竞价序列"
      ],
      "protocol": "0x056a",
      "kind": "底层协议",
      "summary": "返回主站当前保存的集合竞价过程快照；即使出现 09:25:00 也不是逐笔成交。",
      "return_model": "AuctionSeries",
      "doc": "methods/7709-集合竞价明细.md"
    },
    {
      "id": "7709-auction-0925",
      "title": "09:25 正式撮合",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.helpers.auction_0925(code, date)",
      "aliases": [
        "开盘竞价",
        "925"
      ],
      "protocol": "主站当前交易日 0x0fc5，其他日期 0x0fc6",
      "kind": "功能接口",
      "summary": "从当前或历史成交明细中扫描 09:25 opening_match，忽略 status=8 竞价快照。",
      "return_model": "Auction0925Result",
      "doc": "methods/7709-0925竞价成交快照.md"
    },
    {
      "id": "7709-gbbq",
      "title": "股本变迁 / GBBQ",
      "source": "7709",
      "category": "公司基础",
      "api": "client.corporate.capital_changes(code)",
      "aliases": [
        "capital_changes",
        "股本事件"
      ],
      "protocol": "0x000f",
      "kind": "底层协议",
      "summary": "返回除权除息、股本变化、增发和回购等股本事件记录。",
      "return_model": "CapitalChangeBlock",
      "doc": "methods/7709-股本变迁GBBQ.md"
    },
    {
      "id": "7709-xdxr",
      "title": "除权除息整理",
      "source": "Helper",
      "category": "公司基础",
      "api": "client.helpers.xdxr(code)",
      "aliases": [
        "分红",
        "送转",
        "配股"
      ],
      "protocol": "基于 0x000f",
      "kind": "功能接口",
      "summary": "从股本变迁记录中筛出除权除息事件，并整理分红、送转和配股字段。",
      "return_model": "list[XdxrRecord]",
      "doc": "methods/7709-除权除息整理.md"
    },
    {
      "id": "7709-equity",
      "title": "指定日期股本",
      "source": "Helper",
      "category": "公司基础",
      "api": "client.helpers.equity_changes(code) / client.helpers.equity(code, on=...)",
      "calls": [
        {
          "label": "变迁列表",
          "api": "client.helpers.equity_changes(code)"
        },
        {
          "label": "指定日期",
          "api": "client.helpers.equity(code, on=...)"
        }
      ],
      "aliases": [
        "流通股本",
        "总股本"
      ],
      "protocol": "基于 0x000f",
      "kind": "功能接口",
      "summary": "整理历次股本变化，并取得指定日期之前最近一次流通股本和总股本。",
      "return_model": "EquityResponse / EquityRecord",
      "doc": "methods/7709-指定日期股本.md"
    },
    {
      "id": "7709-turnover",
      "title": "换手率",
      "source": "Helper",
      "category": "公司基础",
      "api": "client.helpers.turnover(code, volume, on=None, unit=\"hand\")",
      "aliases": [
        "turnover rate"
      ],
      "protocol": "基于 0x000f",
      "kind": "功能接口",
      "summary": "使用成交量和指定日期的流通股本计算换手率。",
      "return_model": "float",
      "doc": "methods/7709-换手率.md"
    },
    {
      "id": "7709-local-factors",
      "title": "本地复权因子",
      "source": "Helper",
      "category": "公司基础",
      "api": "client.helpers.factors(code) / client.helpers.local_adjusted_kline(code, ...)",
      "calls": [
        {
          "label": "复权因子",
          "api": "client.helpers.factors(code)"
        },
        {
          "label": "本地复权 K 线",
          "api": "client.helpers.local_adjusted_kline(code, ...)"
        }
      ],
      "aliases": [
        "qfq",
        "hfq"
      ],
      "protocol": "0x052d + 0x000f",
      "kind": "功能接口",
      "summary": "根据不复权日 K 和除权除息记录计算本地前复权、后复权因子及 K 线。",
      "return_model": "FactorResponse / KlineSeries",
      "doc": "methods/7709-本地复权因子.md"
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
      "id": "helper-quote-table",
      "title": "批量行情表",
      "source": "Helper",
      "category": "股票与行情",
      "api": "client.helpers.quote_table(codes)",
      "aliases": [
        "quote_table",
        "行情表"
      ],
      "protocol": "0x054c + 0x044d",
      "kind": "组合能力",
      "summary": "合并批量行情快照和代码表，快速整理一组证券的行情表。",
      "return_model": "StockProfileTable",
      "doc": "helpers/批量行情表.md"
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
      "summary": "按交易日安全对齐统计资源和实时行情，返回流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比、几天几板等 21 个短线字段。",
      "return_model": "ShortlineIndicatorTable",
      "doc": "helpers/短线指标.md"
    },
    {
      "id": "helper-adjusted-kline",
      "title": "复权 K 线",
      "source": "Helper",
      "category": "股票与行情",
      "api": "client.helpers.adjusted_kline(code, ...)",
      "aliases": [
        "adjusted_kline",
        "qfq",
        "hfq",
        "定点复权"
      ],
      "protocol": "0x052d",
      "kind": "组合能力",
      "summary": "统一获取不复权、前复权、后复权或定点复权 K 线，并可自动分页。",
      "return_model": "KlineSeries",
      "doc": "helpers/复权K线.md"
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
      "title": "竞价数据",
      "source": "Helper",
      "category": "集合竞价",
      "api": "client.helpers.auction_data(code, date)",
      "aliases": [
        "auction_data",
        "开盘涨幅",
        "开盘金额"
      ],
      "protocol": "0x056a + (0x0fc5/0x0fc6) + 0x054c",
      "kind": "组合能力",
      "summary": "合并竞价序列、09:25 成交快照和昨收，计算开盘价、开盘金额及涨幅。",
      "return_model": "AuctionData",
      "doc": "helpers/竞价数据.md"
    },
    {
      "id": "helper-server-stats",
      "title": "服务器文件下载与统计解析",
      "source": "Helper",
      "category": "服务器资源",
      "api": "client.resources.download_file() / client.resources.read_stats()",
      "calls": [
        {
          "label": "完整下载",
          "api": "client.resources.download_file(path, ...)"
        },
        {
          "label": "下载并解析",
          "api": "client.resources.read_stats(...)"
        }
      ],
      "aliases": [
        "download_file",
        "read_stats",
        "zhb.zip",
        "tdxstat",
        "服务器文件"
      ],
      "protocol": "基于 0x06b9",
      "kind": "协议封装",
      "summary": "循环下载完整服务器文件，并可解析 zhb.zip 中的 tdxstat.cfg 与 tdxstat2.cfg。",
      "return_model": "bytes / TdxStatsResource",
      "doc": "methods/7709-服务器文件读取.md",
      "doc_anchor": "stats-resource"
    }
  ]
};
