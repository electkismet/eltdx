window.ELTDX_RUST_CATALOG = (function () {
  "use strict";

  var version = "3.4.0-beta.2";
  var docRoot = "rust-api/" + version + "/eltdx/";
  var layers = [
    {id: "7709", label: "7709 原生协议接口", tag_label: "7709 原生", source: "7709", description: "HQ 行情连接、快照、K 线、分时和成交等接口。"},
    {id: "7615", label: "7615 F10", tag_label: "F10", source: "7615", description: "TQLEX HTTP 调用与命名业务方法，需启用 f10。"},
    {id: "helpers", label: "Helpers 封装", tag_label: "Helpers", source: "helpers", description: "行情与 F10 的组合查询、交易日历和业务计算。"},
    {id: "ex", label: "扩展行情（7727）", tag_label: "EX", source: "ex", description: "EX 市场、标的、报价、K 线、分时和成交，需启用 ex。"},
    {id: "mac", label: "MAC 行情", tag_label: "MAC", source: "mac", description: "MAC 报价、板块、行情与文件接口，需启用 mac。"},
    {id: "macex", label: "MAC-EX 与外盘", tag_label: "MAC-EX", source: "macex", description: "MAC-EX 报价、外盘、标的与宏观指标，需启用 mac。"},
    {id: "offline", label: "离线文件与财报", tag_label: "Offline", source: "offline", description: "本地行情文件、股本、板块及专业财报接口，需启用 offline。"}
  ];
  var functionalGroups = [
    {id: "basics", label: "连接与会话", description: "客户端配置、连接、握手、心跳与关闭。"},
    {id: "codes", label: "证券代码", description: "证券数量、分页代码表、全量证券与品种筛选。"},
    {id: "realtime", label: "实时行情", description: "快照、盘口、订阅、分类行情和当日分时。"},
    {id: "history", label: "历史行情与成交", description: "历史分时、近期分时和成交明细。"},
    {id: "klines", label: "K 线与复权", description: "K 线分页及收集、退市证券、股本变动与复权因子。"},
    {id: "auction", label: "集合竞价与短线", description: "集合竞价、开盘成交、短线指标和涨停梯队。"},
    {id: "f10", label: "F10", description: "公司资料、财报、资讯、资金流、市场信息与题材。"},
    {id: "common", label: "常用封装与资源", description: "交易日历、涨跌停规则、市场统计、板块和资源文件。"},
    {id: "ex", label: "扩展行情（7727）", description: "扩展市场目录和行情数据。"},
    {id: "mac", label: "MAC 行情", description: "MAC 盘口、K 线、分时、板块排行和文件。"},
    {id: "macex", label: "MAC-EX 与外盘", description: "MAC-EX 标的目录、行情数据和宏观指标。"},
    {id: "offline", label: "离线文件与财报", description: "本地文件的读写、专业财报的下载与解码。"}
  ];
  var groups = [
    {
      layer: "7709",
      functionId: "basics",
      page: "struct.TdxClient.html",
      owner: "TdxClient",
      feature: "",
      entries: [
        ["builder", "配置客户端", "builder()", "ClientBuilder", "创建 HQ 客户端配置。调用 build() 后，客户端会在首次查询时按需连接。"],
        ["connect", "连接行情主站", "connect(&self, options: &OperationOptions)", "Result<()>", "显式建立 HQ 行情连接。"],
        ["close", "关闭客户端", "close(&self)", "Result<()>", "关闭客户端及共享此会话的句柄。"]
      ]
    },
    {
      layer: "7709",
      functionId: "basics",
      page: "services/struct.SessionService.html",
      owner: "SessionService",
      feature: "",
      entries: [
        ["handshake", "握手", "handshake(&self, options: &OperationOptions)", "Result<HandshakeInfo>", "读取服务器时间、交易时段及服务标识。"],
        ["heartbeat", "心跳", "heartbeat(&self, options: &OperationOptions)", "Result<HeartbeatAck>", "发起一次心跳请求并检查响应。"]
      ]
    },
    {
      layer: "7709",
      functionId: "codes",
      page: "services/struct.CodeService.html",
      owner: "CodeService",
      feature: "",
      entries: [
        ["count", "市场代码数量", "count(&self, market: HqMarket, options: &OperationOptions)", "Result<u32>", "返回指定市场的证券数量。"],
        ["list", "证券代码表", "list(&self, market: HqMarket, query: &CodeQuery, options: &OperationOptions)", "Result<DataPage<Security>>", "分页读取市场证券列表。"],
        ["all", "全量证券列表", "all(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<Security>>", "按查询条件收集指定市场的所有证券代码。"],
        ["a_share_count", "A 股数量", "a_share_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions)", "Result<usize>", "统计指定市场的 A 股数量。"],
        ["all_a_shares", "沪深北 A 股列表", "all_a_shares(&self, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<SecurityId>>", ""],
        ["all_etfs", "市场 ETF 列表", "all_etfs(&self, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<SecurityId>>", ""],
        ["all_indices", "市场指数列表", "all_indices(&self, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<SecurityId>>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "realtime",
      page: "services/struct.QuoteService.html",
      owner: "QuoteService",
      feature: "",
      entries: [
        ["snapshots", "批量行情快照", "snapshots(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<SnapshotBatch>", "按输入顺序返回当前行情快照及每项的状态。"],
        ["snapshots_with_policy", "批量快照与缺失策略", "snapshots_with_policy(&self, ids: &[SecurityId], policy: MissingPolicy, options: &OperationOptions)", "Result<SnapshotBatch>", "查询批量快照并指定缺失数据的处理策略。"],
        ["depth", "盘口深度", "depth(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<QuoteUpdates>", "查询证券盘口及各档报价。"],
        ["category", "分类行情", "category(&self, query: &CategoryQuery, options: &OperationOptions)", "Result<DataPage<CategoryQuote>>", "按分类、排序和分页参数读取行情。"],
        ["legacy", "旧版报价", "legacy(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<LegacyQuote>>", "使用旧版报价协议读取行情。"],
        ["subscribe", "订阅行情", "subscribe(&self, ids: &[SecurityId], subscription: &SubscriptionOptions, options: &OperationOptions)", "Result<Subscription>", "创建行情订阅并通过订阅句柄读取事件。"]
      ]
    },
    {
      layer: "7709",
      functionId: "realtime",
      page: "services/struct.MinuteService.html",
      owner: "MinuteService",
      feature: "",
      entries: [
        ["today", "当日分时", "today(&self, id: &SecurityId, options: &OperationOptions)", "Result<MinuteSeries>", "查询指定证券的当日分时。"],
        ["today_many", "批量当日分时", "today_many(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<MinuteSeries>>", ""],
        ["aux", "分时辅助数据", "aux(&self, id: &SecurityId, kind: AuxKind, options: &OperationOptions)", "Result<MinuteAuxSeries>", ""],
        ["sparkline", "实时分时走势", "sparkline(&self, id: &SecurityId, query: &SparklineQuery, options: &OperationOptions)", "Result<SparklineSeries>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "history",
      page: "services/struct.MinuteService.html",
      owner: "MinuteService",
      feature: "",
      entries: [
        ["history", "历史分时", "history(&self, id: &SecurityId, date: TradingDate, options: &OperationOptions)", "Result<MinuteSeries>", "按证券和日期查询历史分时。"],
        ["history_many", "批量历史分时", "history_many(&self, ids: &[SecurityId], date: TradingDate, options: &OperationOptions)", "Result<BatchOutcome<MinuteSeries>>", ""],
        ["recent", "近几日分时", "recent(&self, id: &SecurityId, query: &MinuteQuery, options: &OperationOptions)", "Result<MinuteSeries>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "history",
      page: "services/struct.TradeService.html",
      owner: "TradeService",
      feature: "",
      entries: [
        ["page", "成交明细分页", "page(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradePage>", "按查询参数返回当日或历史成交明细。"],
        ["collect", "全量成交明细", "collect(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradePage>", "在分页预算内收集成交明细。"],
        ["columns", "成交明细列式结果", "columns(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradeColumns>", ""],
        ["page_many", "批量成交明细", "page_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<TradePage>>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "klines",
      page: "services/struct.KlineService.html",
      owner: "KlineService",
      feature: "",
      entries: [
        ["page", "K 线分页", "page(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions)", "Result<KlineSeries>", "按证券周期和分页条件查询 K 线。"],
        ["collect", "全量 K 线", "collect(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions)", "Result<KlineSeries>", "按查询条件自动翻页收集 K 线。"],
        ["page_many", "批量 K 线分页", "page_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions)", "Result<BatchOutcome<KlineSeries>>", ""],
        ["collect_many", "批量收集 K 线", "collect_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions)", "Result<BatchOutcome<KlineSeries>>", ""],
        ["delisted_page", "退市证券 K 线", "delisted_page(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions)", "Result<KlineSeries>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "klines",
      page: "services/struct.CorporateService.html",
      owner: "CorporateService",
      feature: "",
      entries: [
        ["capital_changes", "股本变动与除权除息", "capital_changes(&self, ids: &[SecurityId], query: &CapitalQuery, options: &OperationOptions)", "Result<BatchOutcome<CapitalChangeBlock>>", ""],
        ["adjustment_factors", "复权因子", "adjustment_factors(&self, ids: &[SecurityId], query: &FactorQuery, options: &OperationOptions)", "Result<BatchOutcome<AdjustmentFactors>>", "根据股本事件和价格构造复权因子。"],
        ["finance", "财务记录", "finance(&self, ids: &[SecurityId], query: &FinanceQuery, options: &OperationOptions)", "Result<BatchOutcome<FinanceRecord>>", "按证券读取 HQ 财务信息。"],
        ["finance_fields", "指定财务字段", "finance_fields(&self, ids: &[SecurityId], fields: &[FinanceField], query: &FinanceQuery, options: &OperationOptions)", "Result<BatchOutcome<FinanceProjection>>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "auction",
      page: "services/struct.AuctionService.html",
      owner: "AuctionService",
      feature: "",
      entries: [
        ["series", "集合竞价行情", "series(&self, id: &SecurityId, query: &AuctionQuery, options: &OperationOptions)", "Result<AuctionSeries>", "查询集合竞价序列。"]
      ]
    },
    {
      layer: "7709",
      functionId: "auction",
      page: "services/struct.TradeService.html",
      owner: "TradeService",
      feature: "",
      entries: [
        ["opening_match", "开盘集合竞价成交", "opening_match(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<Option<TradeTick>>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "common",
      page: "services/struct.MoneyFlowService.html",
      owner: "MoneyFlowService",
      feature: "",
      entries: [
        ["daily", "当日资金流向", "daily(&self, id: &SecurityId, options: &OperationOptions)", "Result<DailyMoneyFlow>", ""],
        ["daily_many", "批量当日资金流向", "daily_many(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<DailyMoneyFlow>>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "common",
      page: "services/struct.LimitService.html",
      owner: "LimitService",
      feature: "",
      entries: [
        ["special", "特殊涨跌停规则", "special(&self, query: &SpecialLimitQuery, options: &OperationOptions)", "Result<DataPage<SpecialLimit>>", ""],
        ["scan_special", "扫描特殊涨跌停规则", "scan_special(&self, query: &SpecialLimitQuery, options: &OperationOptions)", "Result<Collected<SpecialLimit>>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "common",
      page: "services/struct.CompanyInfoService.html",
      owner: "CompanyInfoService",
      feature: "",
      entries: [
        ["category", "公司资料栏目", "category(&self, id: &SecurityId, options: &OperationOptions)", "Result<Table<CompanySection>>", ""],
        ["content", "公司资料文本", "content(&self, id: &SecurityId, query: &CompanySectionQuery, options: &OperationOptions)", "Result<CompanyText>", ""]
      ]
    },
    {
      layer: "7709",
      functionId: "common",
      page: "services/struct.ResourceService.html",
      owner: "ResourceService",
      feature: "",
      entries: [
        ["block_meta", "板块文件元数据", "block_meta(&self, filename: &str, options: &OperationOptions)", "Result<BlockMeta>", ""],
        ["download_file", "下载行情资源文件", "download_file(&self, path: &str, max_bytes: usize, options: &OperationOptions)", "Result<DownloadResult>", ""],
        ["download_complete", "完整下载行情资源文件", "download_complete(&self, path: &str, max_bytes: usize, options: &OperationOptions)", "Result<Bytes>", ""],
        ["read_stats", "市场统计文件", "read_stats(&self, query: &StatsQuery, options: &OperationOptions)", "Result<StatsResource>", ""]
      ]
    },
    {
      layer: "7615",
      functionId: "f10",
      page: "f10/struct.F10Client.html",
      owner: "F10Client",
      feature: "f10",
      entries: [
        ["call", "通用 F10 调用", "call(&self, query: &CallQuery, options: &OperationOptions)", "Result<Response>", "按 Entry 和请求参数获取 F10 响应。"],
        ["market_report", "市场信息表", "market_report(&self, query: &MarketReportQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["index_members", "指数成份股", "index_members(&self, query: &IndexMembersQuery, options: &OperationOptions)", "Result<IndexMemberTable>", ""],
        ["index_tracking_products", "指数跟踪产品", "index_tracking_products(&self, query: &IndexTrackingProductsQuery, options: &OperationOptions)", "Result<IndexTrackingTable>", ""],
        ["board_members", "板块成份股", "board_members(&self, query: &BoardMembersQuery, options: &OperationOptions)", "Result<BoardMemberTable>", ""],
        ["ipo_calendar", "新股申购日历", "ipo_calendar(&self, options: &OperationOptions)", "Result<IpoCalendar>", ""],
        ["business_composition", "主营业务构成", "business_composition(&self, query: &BusinessCompositionQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["fund_report", "基金定期报告", "fund_report(&self, query: &FundReportQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["limit_up_down_list", "涨跌停名单", "limit_up_down_list(&self, query: &LimitUpDownListQuery, options: &OperationOptions)", "Result<LimitBoardLadder>", ""],
        ["limit_board_ladder", "连板梯队", "limit_board_ladder(&self, query: &LimitBoardLadderQuery, options: &OperationOptions)", "Result<LimitBoardLadder>", ""],
        ["money_flow_history", "历史资金流向", "money_flow_history(&self)", "MoneyFlowHistoryService", "Does not initialize a runtime, make a request, or construct an HQ client."],
        ["mg_capital", "美股股本", "mg_capital(&self, query: &MgCapitalQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["mg_shareholders", "美股股东", "mg_shareholders(&self, query: &MgShareholdersQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["mg_market", "美股行情资料", "mg_market(&self, query: &MgMarketQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["hk_pepb_band", "港股估值区间", "hk_pepb_band(&self, query: &HkPepbBandQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["hk_valuation", "港股估值", "hk_valuation(&self, query: &HkValuationQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["hk_analyst_forecast", "港股分析师预测", "hk_analyst_forecast(&self, query: &HkAnalystForecastQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["news_channels", "资讯频道枚举", "news_channels(&self)", "Vec<NewsChannelInfo>", "读取内置资讯频道，该方法不发起网络请求。"],
        ["company_news", "公司资讯", "company_news(&self, query: &CompanyNewsQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["announcements", "公司公告", "announcements(&self, query: &AnnouncementsQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["market_news", "市场资讯", "market_news(&self, query: &MarketNewsQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["channel_news", "频道资讯", "channel_news(&self, query: &ChannelNewsQuery, options: &OperationOptions)", "Result<Response>", ""],
        ["detail", "资讯详情", "detail(&self, query: &DetailQuery, options: &OperationOptions)", "Result<Response>", ""]
      ]
    },
    {
      layer: "helpers",
      functionId: "common",
      page: "helpers/struct.HelperService.html",
      owner: "HelperService",
      feature: "",
      entries: [
        ["full_quotes", "完整行情快照", "full_quotes(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<FullQuote>>", "组合快照与盘口等数据，返回完整行情。"],
        ["market_stat", "市场统计", "market_stat(&self, options: &OperationOptions)", "Result<MarketStat>", "Associate the three required pseudo-indices by identity. The legacy Python positional projection is a separate compatibility contract."],
        ["today_fund_flow", "市场当日资金流向", "today_fund_flow(&self, id: &SecurityId, options: &OperationOptions)", "Result<FundFlow>", "Full-day collection must prove EOF before any amounts are returned. Every page, classification pass and pin release shares this context."],
        ["realtime_rank", "实时排行", "realtime_rank(&self, query: &RankQuery, options: &OperationOptions)", "Result<Table<RealtimeRankRow>>", ""],
        ["board_quotes", "板块行情", "board_quotes(&self, query: &BoardQuery, options: &OperationOptions)", "Result<Table<BoardQuote>>", ""],
        ["board_member_quotes", "板块成份行情", "board_member_quotes(&self, board: &BoardId, query: &BoardQuery, options: &OperationOptions)", "Result<BoardMemberQuotes>", ""],
        ["stock_profile_table", "股票概况", "stock_profile_table(&self, ids: &[SecurityId], query: &ProfileQuery, options: &OperationOptions)", "Result<Table<StockProfile>>", ""],
        ["daily_price_limits", "每日涨跌停价", "daily_price_limits(&self, query: &DailyLimitsQuery, options: &OperationOptions)", "Result<Table<DailyPriceLimit>>", "Reference comes from an unadjusted daily bar and ordered same-day corporate actions. Only the approved dated policy may supply prices."],
        ["latest_stock_list", "最新股票列表", "latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions)", "Result<Collected<Security>>", ""],
        ["latest_st", "最新 ST 股票列表", "latest_st(&self, market: Option<HqMarket>, options: &OperationOptions)", "Result<Collected<Security>>", ""],
        ["latest_suspended", "最新停牌股票列表", "latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions)", "Result<Collected<Security>>", ""],
        ["buy_sell_strength", "买卖力度", "buy_sell_strength(&self, id: &SecurityId, options: &OperationOptions)", "Result<MinuteAuxSeries>", ""],
        ["volume_comparison", "量比数据", "volume_comparison(&self, id: &SecurityId, options: &OperationOptions)", "Result<MinuteAuxSeries>", ""]
      ]
    },
    {
      layer: "helpers",
      functionId: "auction",
      page: "helpers/struct.HelperService.html",
      owner: "HelperService",
      feature: "",
      entries: [
        ["auction_data", "集合竞价数据封装", "auction_data(&self, id: &SecurityId, query: &AuctionDataQuery, options: &OperationOptions)", "Result<AuctionData>", ""],
        ["shortline_indicators", "短线指标", "shortline_indicators(&self, query: &ShortlineQuery, options: &OperationOptions)", "Result<ShortlineTable>", ""],
        ["limit_ladder", "涨停梯队", "limit_ladder(&self, query: &LadderQuery, options: &OperationOptions)", "Result<Table<ShortlineIndicator>>", ""]
      ]
    },
    {
      layer: "helpers",
      functionId: "klines",
      page: "helpers/struct.HelperService.html",
      owner: "HelperService",
      feature: "",
      entries: [
        ["daily_share_capital", "每日股本", "daily_share_capital(&self, query: &DailySharesQuery, options: &OperationOptions)", "Result<Table<DailyShareCapital>>", "Required statistics, optional finance. A missing free-float field is never replaced by a different financial share count."]
      ]
    },
    {
      layer: "helpers",
      functionId: "f10",
      page: "helpers/struct.HelperService.html",
      owner: "HelperService",
      feature: "f10",
      entries: [
        ["stock_topics", "股票关联题材", "stock_topics(&self, query: &StockTopicsQuery, options: &OperationOptions)", "Result<StockTopics>", "Both HTTP sources are required. Nonzero business codes remain explicit in responses()/ok(); transport/owner/budget failures terminate the call."],
        ["topic_stocks", "题材成份股", "topic_stocks(&self, query: &TopicStocksQuery, options: &OperationOptions)", "Result<TopicStockTable>", ""],
        ["theme_strength_rank", "题材强度排行", "theme_strength_rank(&self, query: &ThemeStrengthQuery, options: &OperationOptions)", "Result<ThemeStrengthTable>", ""]
      ]
    },
    {
      layer: "helpers",
      functionId: "common",
      page: "calendar/struct.CalendarService.html",
      owner: "CalendarService",
      feature: "",
      entries: [
        ["today", "交易日历的当前日期", "today(&self)", "Result<TradingDate>", "Clock failures stay explicit; the current civil date is no claim that the exchange is open or that this date is covered by a loaded calendar."],
        ["is_workday", "判断是否为交易日", "is_workday(&self, date: TradingDate, query: &CalendarQuery, options: &OperationOptions)", "Result<bool>", ""],
        ["range", "区间交易日", "range(&self, start: TradingDate, end: TradingDate, descending: bool, query: &CalendarQuery, options: &OperationOptions)", "Result<Vec<TradingDate>>", ""]
      ]
    },
    {
      layer: "ex",
      functionId: "ex",
      page: "ex/struct.ExClient.html",
      owner: "ExClient",
      feature: "ex",
      entries: [
        ["markets", "扩展市场列表", "markets(&self, options: &OperationOptions)", "Result<Vec<MarketInfo>>", ""],
        ["instrument_count", "扩展标的数量", "instrument_count(&self, options: &OperationOptions)", "Result<u32>", ""],
        ["instruments", "扩展标的目录", "instruments(&self, query: &InstrumentsQuery, options: &OperationOptions)", "Result<PageResult<InstrumentInfo>>", ""],
        ["quote", "扩展行情快照", "quote(&self, query: &QuoteQuery, options: &OperationOptions)", "Result<Option<Quote>>", ""],
        ["quote_list", "扩展行情列表", "quote_list(&self, query: &QuoteListQuery, options: &OperationOptions)", "Result<PageResult<QuoteListRow>>", ""],
        ["bars", "扩展 K 线分页", "bars(&self, query: &BarsQuery, options: &OperationOptions)", "Result<PageResult<Bar>>", ""],
        ["bars_range", "扩展区间 K 线", "bars_range(&self, query: &BarsRangeQuery, options: &OperationOptions)", "Result<Vec<Bar>>", ""],
        ["minutes", "扩展当日分时", "minutes(&self, query: &MinutesQuery, options: &OperationOptions)", "Result<Vec<Minute>>", ""],
        ["history_minutes", "扩展历史分时", "history_minutes(&self, query: &HistoryMinutesQuery, options: &OperationOptions)", "Result<Vec<Minute>>", ""],
        ["transactions", "扩展当日成交", "transactions(&self, query: &TransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>", ""],
        ["history_transactions", "扩展历史成交", "history_transactions(&self, query: &HistoryTransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>", ""]
      ]
    },
    {
      layer: "mac",
      functionId: "mac",
      page: "mac/struct.MacClient.html",
      owner: "MacClient",
      feature: "mac",
      entries: [
        ["quotes", "MAC 批量报价", "quotes(&self, query: &QuotesQuery, options: &OperationOptions)", "Result<Vec<Quote>>", ""],
        ["quote_list", "MAC 报价列表", "quote_list(&self, query: &QuoteListQuery, options: &OperationOptions)", "Result<PageResult<Quote>>", ""],
        ["bars", "MAC K 线", "bars(&self, query: &BarsQuery, options: &OperationOptions)", "Result<Vec<Bar>>", "Each response is ordered oldest to newest; older pages are prepended intact. Unknown native units and all seven source float bits survive."],
        ["transactions", "MAC 成交明细", "transactions(&self, query: &TransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>", ""],
        ["tick_chart", "MAC 单日分时", "tick_chart(&self, query: &TickChartQuery, options: &OperationOptions)", "Result<TickChart>", ""],
        ["tick_charts", "MAC 多日分时", "tick_charts(&self, query: &TickChartsQuery, options: &OperationOptions)", "Result<MultiTickChart>", ""],
        ["chart_sampling", "MAC 分时采样", "chart_sampling(&self, query: &ChartSamplingQuery, options: &OperationOptions)", "Result<Vec<WireFloat>>", ""],
        ["symbol_info", "MAC 标的资料", "symbol_info(&self, query: &SymbolInfoQuery, options: &OperationOptions)", "Result<SymbolInfo>", ""],
        ["boards", "MAC 板块目录", "boards(&self, query: &BoardsQuery, options: &OperationOptions)", "Result<Vec<BoardInfo>>", ""],
        ["board_members", "MAC 板块成份", "board_members(&self, query: &BoardMembersQuery, options: &OperationOptions)", "Result<PageResult<Quote>>", ""],
        ["belonging_boards", "MAC 所属板块", "belonging_boards(&self, query: &BelongingBoardsQuery, options: &OperationOptions)", "Result<Vec<BelongBoard>>", ""],
        ["board_summary", "MAC 板块概览", "board_summary(&self, query: &BoardSummaryQuery, options: &OperationOptions)", "Result<BoardSummary>", "Every member page contributes to one total deadline and shared limits."],
        ["board_ranking", "MAC 板块排行", "board_ranking(&self, query: &BoardRankingQuery, options: &OperationOptions)", "Result<Vec<BoardRank>>", "Money/volume keys aggregate all candidates before a stable top-N. None retains the full bounded ranking. Missing reference values sort last."],
        ["board_change_ranking", "MAC 板块涨跌幅排行", "board_change_ranking(&self, query: &BoardChangeRankingQuery, options: &OperationOptions)", "Result<Vec<BoardChangeRank>>", "Fetches each board’s required history on this same operation. Missing intervals are diagnosed; required I/O, cancellation and limits propagate."],
        ["capital_flow", "MAC 资金流向", "capital_flow(&self, query: &CapitalFlowQuery, options: &OperationOptions)", "Result<Option<CapitalFlow>>", ""],
        ["auction", "MAC 集合竞价", "auction(&self, query: &AuctionQuery, options: &OperationOptions)", "Result<Vec<Auction>>", ""],
        ["unusual", "MAC 异动", "unusual(&self, query: &UnusualQuery, options: &OperationOptions)", "Result<PageResult<Unusual>>", ""],
        ["server_info", "MAC 服务端信息", "server_info(&self, options: &OperationOptions)", "Result<ServerSession>", ""],
        ["kline_offsets", "MAC K 线偏移", "kline_offsets(&self, query: &KlineOffsetsQuery, options: &OperationOptions)", "Result<KlineOffsetInfo>", ""],
        ["file_meta", "MAC 文件元数据", "file_meta(&self, query: &FileMetaQuery, options: &OperationOptions)", "Result<FileMeta>", ""],
        ["download_chunk", "MAC 分块下载", "download_chunk(&self, query: &DownloadChunkQuery, options: &OperationOptions)", "Result<Vec<u8>>", ""],
        ["download_file", "MAC 文件下载", "download_file(&self, query: &DownloadFileQuery, options: &OperationOptions)", "Result<DownloadResult>", ""],
        ["download_complete", "MAC 完整文件下载", "download_complete(&self, query: &DownloadFileQuery, options: &OperationOptions)", "Result<Bytes>", "Returns a complete file only after an observed EOF and consistent size. The starting offset must be zero; download_file supports resumed suffixes."]
      ]
    },
    {
      layer: "macex",
      functionId: "macex",
      page: "mac/struct.MacExClient.html",
      owner: "MacExClient",
      feature: "mac",
      entries: [
        ["goods_count", "MAC-EX 标的数量", "goods_count(&self, query: &GoodsCountQuery, options: &OperationOptions)", "Result<u32>", ""],
        ["goods", "MAC-EX 标的列表", "goods(&self, query: &GoodsQuery, options: &OperationOptions)", "Result<PageResult<GoodsItem>>", ""],
        ["instruments", "MAC-EX 扩展标的目录", "instruments(&self, query: &InstrumentsQuery, options: &OperationOptions)", "Result<PageResult<InstrumentInfo>>", ""],
        ["quotes", "MAC-EX 批量报价", "quotes(&self, query: &QuotesQuery, options: &OperationOptions)", "Result<Vec<Quote>>", ""],
        ["quote_list", "MAC-EX 报价列表", "quote_list(&self, query: &QuoteListQuery, options: &OperationOptions)", "Result<PageResult<Quote>>", ""],
        ["bars", "MAC-EX K 线", "bars(&self, query: &BarsQuery, options: &OperationOptions)", "Result<Vec<Bar>>", ""],
        ["tick_chart", "MAC-EX 分时", "tick_chart(&self, query: &TickChartQuery, options: &OperationOptions)", "Result<TickChart>", ""],
        ["transactions", "MAC-EX 成交明细", "transactions(&self, query: &TransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>", "HK stock markets use ordinary EX transaction bytes on this same logged in owner. Futures and other markets retain MAC0x122F encoding."],
        ["all_transactions", "MAC-EX 全量成交明细", "all_transactions(&self, query: &AllTransactionsQuery, options: &OperationOptions)", "Result<Vec<Transaction>>", "A full cap of 50 pages/90000 rows without EOF is LimitExceeded. The method cannot return success while claiming an unproven complete day."],
        ["chart_sampling", "MAC-EX 分时采样", "chart_sampling(&self, query: &ChartSamplingQuery, options: &OperationOptions)", "Result<Vec<WireFloat>>", ""],
        ["macro_indicators", "宏观指标目录", "macro_indicators(&self, options: &OperationOptions)", "Result<Vec<GoodsItem>>", ""],
        ["macro_quotes", "宏观指标报价", "macro_quotes(&self, query: &MacroQuotesQuery, options: &OperationOptions)", "Result<Vec<Quote>>", ""],
        ["macro_bars", "宏观指标 K 线", "macro_bars(&self, query: &MacroBarsQuery, options: &OperationOptions)", "Result<Vec<Bar>>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/daily/",
      owner: "eltdx::offline::daily",
      feature: "offline",
      entries: [
        ["read", "读取本地日线", "read(path: &Path, coefficients: Option<Coefficients>, options: &ReadOptions)", "Result<ReadResult<DailyBar>>", ""],
        ["write", "写入本地日线", "write(path: &Path, rows: &[DailyBar], coefficients: Coefficients, options: &AppendOptions)", "Result<WriteReport>", "Appends with the source daily sync policy. Coefficients::default() is .01/.01."],
        ["append", "追加本地日线", "append(path: &Path, rows: &[DailyBar], coefficients: Coefficients, options: &AppendOptions)", "Result<WriteReport>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/minute/",
      owner: "eltdx::offline::minute",
      feature: "offline",
      entries: [
        ["read_lc", "读取本地 LC 分钟线", "read_lc(path: &Path, options: &ReadOptions)", "Result<ReadResult<MinBar>>", ""],
        ["read_5", "读取本地 .5 分钟线", "read_5(path: &Path, options: &ReadOptions)", "Result<ReadResult<MinBar>>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/ex_daily/",
      owner: "eltdx::offline::ex_daily",
      feature: "offline",
      entries: [
        ["read", "读取本地扩展日线", "read(path: &Path, options: &ReadOptions)", "Result<ReadResult<ExDailyBar>>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/block/",
      owner: "eltdx::offline::block",
      feature: "offline",
      entries: [
        ["read", "读取本地板块文件", "read(path: &Path, options: &ReadOptions)", "Result<ReadResult<BlockInfo>>", ""],
        ["read_custom", "读取自定义板块", "read_custom(dir: &Path, options: &ReadOptions)", "Result<ReadResult<CustomerBlock>>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/gbbq/",
      owner: "eltdx::offline::gbbq",
      feature: "offline",
      entries: [
        ["read", "读取本地股本变迁文件", "read(path: &Path, options: &ReadOptions)", "Result<ReadResult<GbbqRecord>>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/industry/",
      owner: "eltdx::offline::industry",
      feature: "offline",
      entries: [
        ["read", "读取本地行业分类", "read(path: &Path, options: &ReadOptions)", "Result<IndustryMap>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "offline/financial/",
      owner: "eltdx::offline::financial",
      feature: "offline",
      entries: [
        ["read_detailed", "读取本地专业财报", "read_detailed(path: &Path, options: &FinancialReadOptions)", "Result<FinancialDatResult>", "读取本地专业财报并保留日期来源、诊断和完成状态。"],
        ["parse_dat", "解析专业财报 DAT", "parse_dat(data: &[u8], report_date: Option<TradingDate>, options: &FinancialParseOptions)", "Result<ReadResult<FinancialRecord>>", ""]
      ]
    },
    {
      layer: "offline",
      functionId: "offline",
      page: "financial/struct.FinancialService.html",
      owner: "FinancialService",
      feature: "offline",
      entries: [
        ["list_archive_files", "专业财报档案列表", "list_archive_files(&self, options: &OperationOptions)", "Result<Vec<FinancialFileInfo>>", ""],
        ["download_archive", "下载专业财报档案", "download_archive(&self, filename: &FinancialArchiveName, download: &DownloadOptions, options: &OperationOptions)", "Result<DownloadResult>", "Bounded download; LimitReached is inspectable and is never passed to ZIP."],
        ["download_records_detailed", "下载与解析专业财报", "download_records_detailed(&self, query: &FinancialRecordsQuery, options: &OperationOptions)", "Result<FinancialDatResult>", "下载完整财报档案后解码，返回记录与来源信息。"]
      ]
    }
  ];

  var protocols = {
    "7709": "7709",
    "7615": "HTTP / JSON",
    helpers: "—",
    ex: "7727",
    mac: "MAC",
    macex: "MAC-EX",
    offline: "文件 / 财报"
  };
  var groupsById = Object.create(null);
  functionalGroups.forEach(function (group) {
    group.item_ids = [];
    groupsById[group.id] = group;
  });

  var items = [];
  groups.forEach(function (group) {
    group.entries.forEach(function ([method, title, signature, result, summary]) {
      var isFunction = group.page.endsWith("/");
      var doc = group.page + (isFunction ? "fn." + method + ".html" : "");
      var anchor = isFunction ? "" : "method." + method;
      var id = doc + "#" + anchor;
      groupsById[group.functionId].item_ids.push(id);
      items.push({
        id: id,
        title: title,
        source: group.layer,
        category: group.owner,
        api: group.owner + "::" + signature,
        protocol: method === "news_channels" ? "—" : protocols[group.layer],
        kind: group.feature ? "feature: " + group.feature : "默认功能",
        summary: summary || "—",
        return_model: result,
        doc: docRoot + doc,
        doc_anchor: anchor
      });
    });
  });

  return {
    release: {version: version},
    description: "收录 " + items.length + " 项 Rust SDK 业务接口，按功能或协议浏览。",
    taxonomy: {layers: layers, functional_groups: functionalGroups},
    items: items
  };
})();
