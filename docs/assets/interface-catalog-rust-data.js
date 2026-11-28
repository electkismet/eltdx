window.ELTDX_RUST_CATALOG = (function (pythonCatalog) {
  "use strict";

  var version = "3.4.0-beta.4";
  var docRoot = "rust-api/" + version + "/eltdx/";
  var entries = Object.create(null);

  function addEntries(page, owner, feature, rows) {
    rows.forEach(function ([id, methods, summary]) {
      var calls = methods.map(function ([method, label, signature, result]) {
        return {
          label: label,
          api: owner + "::" + signature,
          doc: docRoot + page,
          doc_anchor: "method." + method,
          return_model: result
        };
      });
      entries[id] = {
        calls: calls,
        summary: summary,
        kind: feature ? "feature: " + feature : "默认功能"
      };
    });
  }

  addEntries("services/struct.SessionService.html", "SessionService", "", [
    ["7709-handshake", [["handshake", "", "handshake(&self, options: &OperationOptions)", "Result<HandshakeInfo>"]], "读取服务端时间、交易时段、主站名和产品标识。"],
    ["7709-heartbeat", [["heartbeat", "", "heartbeat(&self, options: &OperationOptions)", "Result<HeartbeatAck>"]], "发起一次心跳请求并读取应答。"],
  ]);

  addEntries("services/struct.CodeService.html", "CodeService", "", [
    ["7709-code-count", [
      ["count", "市场全部代码数", "count(&self, market: HqMarket, options: &OperationOptions)", "Result<u32>"],
      ["a_share_count", "仅 A 股数量", "a_share_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions)", "Result<usize>"],
    ], "count() 统计市场全部代码，a_share_count() 只统计 A 股。"],
    ["7709-code-list", [
      ["all", "指定市场自动翻页", "all(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<Security>>"],
      ["all_a_shares", "沪深北 A 股", "all_a_shares(&self, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<SecurityId>>"],
      ["list", "手动分页", "list(&self, market: HqMarket, query: &CodeQuery, options: &OperationOptions)", "Result<DataPage<Security>>"],
      ["all_etfs", "ETF 列表", "all_etfs(&self, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<SecurityId>>"],
      ["all_indices", "指数列表", "all_indices(&self, query: &CodeScanQuery, options: &OperationOptions)", "Result<Collected<SecurityId>>"],
    ], "按市场和筛选条件读取证券代码。全量查询受分页预算限制。"],
  ]);

  addEntries("services/struct.QuoteService.html", "QuoteService", "", [
    ["7709-quote-snapshots", [
      ["snapshots", "批量快照", "snapshots(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<SnapshotBatch>"],
      ["snapshots_with_policy", "指定缺失策略", "snapshots_with_policy(&self, ids: &[SecurityId], policy: MissingPolicy, options: &OperationOptions)", "Result<SnapshotBatch>"],
    ], "按输入顺序返回当前行情快照和各项状态。"],
    ["7709-legacy-quotes", [["legacy", "", "legacy(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<LegacyQuote>>"]], "读取旧版批量报价，保留各标的的独立结果。"],
    ["7709-category-quotes", [["category", "", "category(&self, query: &CategoryQuery, options: &OperationOptions)", "Result<DataPage<CategoryQuote>>"]], "按分类、排序及分页条件读取一页行情。"],
    ["7709-quote-refresh", [["subscribe", "", "subscribe(&self, ids: &[SecurityId], subscription: &SubscriptionOptions, options: &OperationOptions)", "Result<Subscription>"]], "返回订阅句柄，通过句柄接收行情事件。使用 SubscriptionOptions 配置订阅。"],
  ]);

  addEntries("services/struct.KlineService.html", "KlineService", "", [
    ["7709-kline", [
      ["page", "单证券分页", "page(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions)", "Result<KlineSeries>"],
      ["collect", "单证券自动翻页", "collect(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions)", "Result<KlineSeries>"],
      ["page_many", "批量分页", "page_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions)", "Result<BatchOutcome<KlineSeries>>"],
      ["collect_many", "批量自动翻页", "collect_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions)", "Result<BatchOutcome<KlineSeries>>"],
    ], "使用 KlineQuery 指定周期、复权及分页条件。"],
    ["7709-delisted-bars", [
      ["delisted_page", "单证券分页", "delisted_page(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions)", "Result<KlineSeries>"],
      ["delisted_collect", "单证券自动翻页", "delisted_collect(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions)", "Result<KlineSeries>"],
      ["delisted_page_many", "批量分页", "delisted_page_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions)", "Result<BatchOutcome<KlineSeries>>"],
      ["delisted_collect_many", "批量自动翻页", "delisted_collect_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions)", "Result<BatchOutcome<KlineSeries>>"],
    ], "查询退市证券不复权日线。"],
  ]);

  addEntries("services/struct.MinuteService.html", "MinuteService", "", [
    ["7709-minute-today", [
      ["today", "单证券", "today(&self, id: &SecurityId, options: &OperationOptions)", "Result<MinuteSeries>"],
      ["today_many", "多证券", "today_many(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<MinuteSeries>>"],
    ]],
    ["7709-minute-history", [
      ["history", "单证券", "history(&self, id: &SecurityId, date: TradingDate, options: &OperationOptions)", "Result<MinuteSeries>"],
      ["history_many", "多证券", "history_many(&self, ids: &[SecurityId], date: TradingDate, options: &OperationOptions)", "Result<BatchOutcome<MinuteSeries>>"],
    ]],
    ["7709-minute-recent", [
      ["recent", "单证券", "recent(&self, id: &SecurityId, query: &MinuteQuery, options: &OperationOptions)", "Result<MinuteSeries>"],
      ["recent_many", "多证券", "recent_many(&self, ids: &[SecurityId], query: &MinuteQuery, options: &OperationOptions)", "Result<BatchOutcome<MinuteSeries>>"],
    ]],
    ["7709-minute-aux", [["aux", "", "aux(&self, id: &SecurityId, kind: AuxKind, options: &OperationOptions)", "Result<MinuteAuxSeries>"]]],
    ["7709-sparkline", [["sparkline", "", "sparkline(&self, id: &SecurityId, query: &SparklineQuery, options: &OperationOptions)", "Result<SparklineSeries>"]]],
  ]);

  addEntries("services/struct.TradeService.html", "TradeService", "", [
    ["7709-trades-today", [
      ["page", "单证券分页", "page(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradePage>"],
      ["collect", "单证券自动翻页", "collect(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradePage>"],
      ["page_many", "多证券分页", "page_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<TradePage>>"],
      ["collect_many", "多证券自动翻页", "collect_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<TradePage>>"],
    ], "使用默认日期 MarketDateChoice::Current 获取当日成交明细。"],
    ["7709-trades-history", [
      ["page", "单证券分页", "page(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradePage>"],
      ["collect", "单证券自动翻页", "collect(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<TradePage>"],
      ["page_many", "多证券分页", "page_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<TradePage>>"],
      ["collect_many", "多证券自动翻页", "collect_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<TradePage>>"],
    ], "在 TradeQuery 中设置 MarketDateChoice::Date(date) 读取指定交易日的成交明细。"],
    ["7709-opening-match-today", [
      ["opening_match", "单证券", "opening_match(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<Option<TradeTick>>"],
      ["opening_match_many", "多证券", "opening_match_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<Option<TradeTick>>>"],
    ], "使用 MarketDateChoice::Current，从当日成交明细提取 09:25 正式撮合。"],
    ["7709-opening-match-history", [
      ["opening_match", "单证券", "opening_match(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)", "Result<Option<TradeTick>>"],
      ["opening_match_many", "多证券", "opening_match_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)", "Result<BatchOutcome<Option<TradeTick>>>"],
    ], "使用 MarketDateChoice::Date(date)，从指定交易日成交明细提取 09:25 正式撮合。"],
  ]);

  addEntries("services/struct.AuctionService.html", "AuctionService", "", [
    ["7709-auction-series", [["series", "", "series(&self, id: &SecurityId, query: &AuctionQuery, options: &OperationOptions)", "Result<AuctionSeries>"]], "通过 AuctionQuery 选择当日或指定交易日，返回集合竞价过程序列。"],
  ]);

  addEntries("services/struct.CorporateService.html", "CorporateService", "", [
    ["7709-gbbq", [["capital_changes", "", "capital_changes(&self, ids: &[SecurityId], query: &CapitalQuery, options: &OperationOptions)", "Result<BatchOutcome<CapitalChangeBlock>>"]], "按证券批量读取股本变迁与权息事件。"],
    ["7709-local-factors", [["adjustment_factors", "", "adjustment_factors(&self, ids: &[SecurityId], query: &FactorQuery, options: &OperationOptions)", "Result<BatchOutcome<AdjustmentFactors>>"]], "从权息事件生成本地复权因子。"],
    ["7709-finance", [
      ["finance", "财务快照", "finance(&self, ids: &[SecurityId], query: &FinanceQuery, options: &OperationOptions)", "Result<BatchOutcome<FinanceRecord>>"],
      ["finance_fields", "指定财务字段", "finance_fields(&self, ids: &[SecurityId], fields: &[FinanceField], query: &FinanceQuery, options: &OperationOptions)", "Result<BatchOutcome<FinanceProjection>>"],
    ], "按证券批量读取基础财务信息，也可通过字段列表获得投影结果。"],
  ]);

  addEntries("services/struct.MoneyFlowService.html", "MoneyFlowService", "", [
    ["7709-money-flow", [
      ["daily", "单证券", "daily(&self, id: &SecurityId, options: &OperationOptions)", "Result<DailyMoneyFlow>"],
      ["daily_many", "多证券", "daily_many(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<DailyMoneyFlow>>"],
    ]],
  ]);

  addEntries("services/struct.LimitService.html", "LimitService", "", [
    ["7709-special-limits", [
      ["special", "分页", "special(&self, query: &SpecialLimitQuery, options: &OperationOptions)", "Result<DataPage<SpecialLimit>>"],
      ["scan_special", "连续扫描", "scan_special(&self, query: &SpecialLimitQuery, options: &OperationOptions)", "Result<Collected<SpecialLimit>>"],
    ]],
  ]);

  addEntries("services/struct.ResourceService.html", "ResourceService", "", [
    ["7709-file-content", [["read", "", "read(&self, path: &str, offset: u64, size: usize, options: &OperationOptions)", "Result<DownloadResult>"]], "按路径、偏移和读取长度获取服务器文件块。"],
    ["helper-server-download", [
      ["download_file", "带上限下载", "download_file(&self, path: &str, max_bytes: usize, options: &OperationOptions)", "Result<DownloadResult>"],
      ["download_complete", "要求完整下载", "download_complete(&self, path: &str, max_bytes: usize, options: &OperationOptions)", "Result<Bytes>"],
    ], "按块读取服务器文件。download_complete() 可用于要求文件下载完整的场景。"],
    ["helper-server-stats", [["read_stats", "", "read_stats(&self, query: &StatsQuery, options: &OperationOptions)", "Result<StatsResource>"]]],
    ["7709-block-meta", [["block_meta", "", "block_meta(&self, filename: &str, options: &OperationOptions)", "Result<BlockMeta>"]]],
  ]);

  addEntries("services/struct.CompanyInfoService.html", "CompanyInfoService", "", [
    ["7709-company-info-category", [["category", "", "category(&self, id: &SecurityId, options: &OperationOptions)", "Result<Table<CompanySection>>"]]],
    ["7709-company-info-content", [["content", "", "content(&self, id: &SecurityId, query: &CompanySectionQuery, options: &OperationOptions)", "Result<CompanyText>"]]],
  ]);

  addEntries("f10/struct.F10Client.html", "F10Client", "f10", [
    ["f10-generic-entry", [
      ["call", "通用请求", "call(&self, query: &CallQuery, options: &OperationOptions)", "Result<Response>"],
      ["params", "按参数发起请求", "params(&self, query: &ParamsQuery, options: &OperationOptions)", "Result<Response>"],
    ], "用 CallQuery 或 ParamsQuery 指定 Entry 和请求参数。"],
    ["f10-stock-info", [
      ["stock_info", "股票基本信息", "stock_info(&self, query: &StockInfoQuery, options: &OperationOptions)", "Result<Response>"],
      ["business_periods", "主营报告期", "business_periods(&self, query: &BusinessPeriodsQuery, options: &OperationOptions)", "Result<Response>"],
      ["topic_ids", "题材 ID", "topic_ids(&self, query: &TopicIdsQuery, options: &OperationOptions)", "Result<Response>"],
    ]],
    ["f10-limit-board-ladder", [["limit_up_down_list", "", "limit_up_down_list(&self, query: &LimitUpDownListQuery, options: &OperationOptions)", "Result<LimitBoardLadder>"]]],
    ["f10-company-profile", [["company_profile", "", "company_profile(&self, query: &CompanyProfileQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-business-composition", [["business_composition", "", "business_composition(&self, query: &BusinessCompositionQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-shareholder-change", [["shareholder_change_plans", "", "shareholder_change_plans(&self, query: &ShareholderChangePlansQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-dividend-financing", [["dividend_financing", "", "dividend_financing(&self, query: &DividendFinancingQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-allotment", [
      ["allotment_dates", "增发日期", "allotment_dates(&self, query: &AllotmentDatesQuery, options: &OperationOptions)", "Result<Response>"],
      ["allotment_details", "获配明细", "allotment_details(&self, query: &AllotmentDetailsQuery, options: &OperationOptions)", "Result<Response>"],
    ]],
    ["f10-finance-report", [["finance_report", "", "finance_report(&self, query: &FinanceReportQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-finance-diagnosis", [["finance_diagnosis", "", "finance_diagnosis(&self, query: &FinanceDiagnosisQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-stock-score", [["stock_score", "", "stock_score(&self, query: &StockScoreQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-profit-forecast", [["profit_forecast", "", "profit_forecast(&self, query: &ProfitForecastQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-theme-market", [["theme_market", "", "theme_market(&self, query: &ThemeMarketQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-valuation", [["valuation", "", "valuation(&self, query: &ValuationQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-ranking", [["ranking_detail", "", "ranking_detail(&self, query: &RankingDetailQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-governance", [["governance", "", "governance(&self, query: &GovernanceQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-hot-topics", [["hot_topics", "", "hot_topics(&self, query: &HotTopicsQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-topic-compare", [
      ["topic_compare", "指定题材", "topic_compare(&self, query: &TopicCompareQuery, options: &OperationOptions)", "Result<Response>"],
      ["topic_compare_first", "首个题材", "topic_compare_first(&self, query: &TopicCompareFirstQuery, options: &OperationOptions)", "Result<Response>"],
    ]],
    ["f10-market-news", [["market_news", "", "market_news(&self, query: &MarketNewsQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-channel-news", [["channel_news", "", "channel_news(&self, query: &ChannelNewsQuery, options: &OperationOptions)", "Result<Response>"]], "根据资讯频道和分页条件读取市场资讯，可指定关键词。"],
    ["f10-news-channels", [["news_channels", "", "news_channels(&self)", "Vec<NewsChannelInfo>"]], "返回内置资讯频道，供频道资讯查询使用。"],
    ["f10-company-news", [["company_news", "", "company_news(&self, query: &CompanyNewsQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-northbound", [["northbound_holding", "", "northbound_holding(&self, query: &NorthboundHoldingQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-detail", [["detail", "", "detail(&self, query: &DetailQuery, options: &OperationOptions)", "Result<Response>"]]],
    ["f10-news-cache", [
      ["cache_list", "通用缓存栏目", "cache_list(&self, query: &CacheListQuery, options: &OperationOptions)", "Result<Response>"],
      ["news", "新闻", "news(&self, query: &NewsQuery, options: &OperationOptions)", "Result<Response>"],
      ["announcements", "公告", "announcements(&self, query: &AnnouncementsQuery, options: &OperationOptions)", "Result<Response>"],
      ["roadshows", "路演", "roadshows(&self, query: &RoadshowsQuery, options: &OperationOptions)", "Result<Response>"],
    ]],
  ]);

  addEntries("helpers/struct.HelperService.html", "HelperService", "", [
    ["7709-quote-depth", [["full_quotes", "", "full_quotes(&self, ids: &[SecurityId], options: &OperationOptions)", "Result<BatchOutcome<FullQuote>>"]], "组合报价快照与盘口深度，返回完整行情。"],
    ["helper-latest-stock-list", [["latest_stock_list", "", "latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions)", "Result<Collected<Security>>"]]],
    ["helper-latest-st", [["latest_st", "", "latest_st(&self, market: Option<HqMarket>, options: &OperationOptions)", "Result<Collected<Security>>"]]],
    ["helper-latest-suspended", [["latest_suspended", "", "latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions)", "Result<Collected<Security>>"]]],
    ["helper-daily-share-capital", [["daily_share_capital", "", "daily_share_capital(&self, query: &DailySharesQuery, options: &OperationOptions)", "Result<Table<DailyShareCapital>>"]]],
    ["helper-daily-price-limits", [["daily_price_limits", "", "daily_price_limits(&self, query: &DailyLimitsQuery, options: &OperationOptions)", "Result<Table<DailyPriceLimit>>"]]],
    ["helper-realtime-rank", [["realtime_rank", "", "realtime_rank(&self, query: &RankQuery, options: &OperationOptions)", "Result<Table<RealtimeRankRow>>"]], "基于分类行情分页获取排行，整理名称和排名。"],
    ["helper-board-quotes", [["board_quotes", "", "board_quotes(&self, query: &BoardQuery, options: &OperationOptions)", "Result<Table<BoardQuote>>"]], "按 BoardQuery 选择板块类别并取得板块行情。"],
    ["helper-board-member-quotes", [["board_member_quotes", "", "board_member_quotes(&self, board: &BoardId, query: &BoardQuery, options: &OperationOptions)", "Result<BoardMemberQuotes>"]], "按板块标识读取板块成分股及行情。"],
    ["helper-buy-sell-strength", [["buy_sell_strength", "", "buy_sell_strength(&self, id: &SecurityId, options: &OperationOptions)", "Result<MinuteAuxSeries>"]]],
    ["helper-volume-comparison", [["volume_comparison", "", "volume_comparison(&self, id: &SecurityId, options: &OperationOptions)", "Result<MinuteAuxSeries>"]]],
    ["helper-limit-ladder", [["limit_ladder", "", "limit_ladder(&self, query: &LadderQuery, options: &OperationOptions)", "Result<Table<ShortlineIndicator>>"]]],
    ["helper-stock-profile", [["stock_profile_table", "", "stock_profile_table(&self, ids: &[SecurityId], query: &ProfileQuery, options: &OperationOptions)", "Result<Table<StockProfile>>"]]],
    ["helper-shortline-indicators", [["shortline_indicators", "", "shortline_indicators(&self, query: &ShortlineQuery, options: &OperationOptions)", "Result<ShortlineTable>"]]],
    ["helper-auction-data", [["auction_data", "", "auction_data(&self, id: &SecurityId, query: &AuctionDataQuery, options: &OperationOptions)", "Result<AuctionData>"]], "组合集合竞价过程、正式撮合与前收盘参考价。"],
  ]);

  addEntries("helpers/struct.HelperService.html", "HelperService", "f10", [
    ["helper-theme-strength-rank", [["theme_strength_rank", "", "theme_strength_rank(&self, query: &ThemeStrengthQuery, options: &OperationOptions)", "Result<ThemeStrengthTable>"]]],
    ["helper-stock-topics", [["stock_topics", "", "stock_topics(&self, query: &StockTopicsQuery, options: &OperationOptions)", "Result<StockTopics>"]]],
    ["helper-topic-stocks", [["topic_stocks", "", "topic_stocks(&self, query: &TopicStocksQuery, options: &OperationOptions)", "Result<TopicStockTable>"]]],
  ]);

  addEntries("ex/struct.ExClient.html", "ExClient", "ex", [
    ["ex-connection", [
      ["builder", "配置客户端", "builder()", "ExClientBuilder"],
      ["connect", "连接", "connect(&self, options: &OperationOptions)", "Result<()>"],
      ["probe_endpoints", "探测主机", "probe_endpoints(&self, endpoints: &[Endpoint], discovery: &DiscoveryOptions, options: &OperationOptions)", "Result<Vec<HostProbe>>"],
      ["close", "关闭", "close(&self)", "Result<()>"],
    ], "创建 EX 客户端，管理连接并探测主机。"],
    ["ex-markets", [["markets", "", "markets(&self, options: &OperationOptions)", "Result<Vec<MarketInfo>>"]]],
    ["ex-count", [["instrument_count", "", "instrument_count(&self, options: &OperationOptions)", "Result<u32>"]]],
    ["ex-info", [["instruments", "", "instruments(&self, query: &InstrumentsQuery, options: &OperationOptions)", "Result<PageResult<InstrumentInfo>>"]]],
    ["ex-quote", [["quote", "", "quote(&self, query: &QuoteQuery, options: &OperationOptions)", "Result<Option<Quote>>"]]],
    ["ex-quotes", [["quote_list", "", "quote_list(&self, query: &QuoteListQuery, options: &OperationOptions)", "Result<PageResult<QuoteListRow>>"]]],
    ["ex-bars", [["bars", "", "bars(&self, query: &BarsQuery, options: &OperationOptions)", "Result<PageResult<Bar>>"]]],
    ["ex-history-bars", [["bars_range", "", "bars_range(&self, query: &BarsRangeQuery, options: &OperationOptions)", "Result<Vec<Bar>>"]]],
    ["ex-minute", [["minutes", "", "minutes(&self, query: &MinutesQuery, options: &OperationOptions)", "Result<Vec<Minute>>"]]],
    ["ex-history-minute", [["history_minutes", "", "history_minutes(&self, query: &HistoryMinutesQuery, options: &OperationOptions)", "Result<Vec<Minute>>"]]],
    ["ex-trades", [["transactions", "", "transactions(&self, query: &TransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>"]]],
    ["ex-history-trades", [["history_transactions", "", "history_transactions(&self, query: &HistoryTransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>"]]],
  ]);

  addEntries("mac/struct.MacClient.html", "MacClient", "mac", [
    ["mac-connection", [
      ["builder", "配置客户端", "builder()", "MacClientBuilder"],
      ["connect", "连接", "connect(&self, options: &OperationOptions)", "Result<()>"],
      ["probe_endpoints", "探测主机", "probe_endpoints(&self, endpoints: &[Endpoint], discovery: &DiscoveryOptions, options: &OperationOptions)", "Result<Vec<HostProbe>>"],
      ["close", "关闭", "close(&self)", "Result<()>"],
    ], "创建 MAC 客户端，管理连接并探测主机。"],
    ["mac-quotes", [["quotes", "", "quotes(&self, query: &QuotesQuery, options: &OperationOptions)", "Result<Vec<Quote>>"]]],
    ["mac-ranking", [["quote_list", "", "quote_list(&self, query: &QuoteListQuery, options: &OperationOptions)", "Result<PageResult<Quote>>"]]],
    ["mac-bars", [["bars", "", "bars(&self, query: &BarsQuery, options: &OperationOptions)", "Result<Vec<Bar>>"]], "读取多周期 K 线，可在 BarsQuery 中配置分页和复权方式。"],
    ["mac-minute", [["tick_chart", "", "tick_chart(&self, query: &TickChartQuery, options: &OperationOptions)", "Result<TickChart>"]]],
    ["mac-get-tick-charts", [["tick_charts", "", "tick_charts(&self, query: &TickChartsQuery, options: &OperationOptions)", "Result<MultiTickChart>"]]],
    ["mac-get-chart-sampling", [["chart_sampling", "", "chart_sampling(&self, query: &ChartSamplingQuery, options: &OperationOptions)", "Result<Vec<WireFloat>>"]]],
    ["mac-trades", [["transactions", "", "transactions(&self, query: &TransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>"]]],
    ["mac-get-symbol-info", [["symbol_info", "", "symbol_info(&self, query: &SymbolInfoQuery, options: &OperationOptions)", "Result<SymbolInfo>"]]],
    ["mac-boards", [["boards", "", "boards(&self, query: &BoardsQuery, options: &OperationOptions)", "Result<Vec<BoardInfo>>"]]],
    ["mac-get-board-members", [["board_members", "", "board_members(&self, query: &BoardMembersQuery, options: &OperationOptions)", "Result<PageResult<Quote>>"]]],
    ["mac-get-belong-board", [["belonging_boards", "", "belonging_boards(&self, query: &BelongingBoardsQuery, options: &OperationOptions)", "Result<Vec<BelongBoard>>"]]],
    ["mac-get-board-summary", [["board_summary", "", "board_summary(&self, query: &BoardSummaryQuery, options: &OperationOptions)", "Result<BoardSummary>"]]],
    ["mac-get-board-ranking", [["board_ranking", "", "board_ranking(&self, query: &BoardRankingQuery, options: &OperationOptions)", "Result<Vec<BoardRank>>"]]],
    ["mac-get-board-change-ranking", [["board_change_ranking", "", "board_change_ranking(&self, query: &BoardChangeRankingQuery, options: &OperationOptions)", "Result<Vec<BoardChangeRank>>"]]],
    ["mac-get-capital-flow", [["capital_flow", "", "capital_flow(&self, query: &CapitalFlowQuery, options: &OperationOptions)", "Result<Option<CapitalFlow>>"]]],
    ["mac-get-auction", [["auction", "", "auction(&self, query: &AuctionQuery, options: &OperationOptions)", "Result<Vec<Auction>>"]]],
    ["mac-get-unusual", [["unusual", "", "unusual(&self, query: &UnusualQuery, options: &OperationOptions)", "Result<PageResult<Unusual>>"]]],
    ["mac-get-server-info", [["server_info", "", "server_info(&self, options: &OperationOptions)", "Result<ServerSession>"]]],
    ["mac-get-kline-offset", [["kline_offsets", "", "kline_offsets(&self, query: &KlineOffsetsQuery, options: &OperationOptions)", "Result<KlineOffsetInfo>"]]],
    ["mac-get-file-meta", [["file_meta", "", "file_meta(&self, query: &FileMetaQuery, options: &OperationOptions)", "Result<FileMeta>"]]],
    ["mac-download-file-chunk", [["download_chunk", "", "download_chunk(&self, query: &DownloadChunkQuery, options: &OperationOptions)", "Result<Vec<u8>>"]]],
    ["mac-download-file", [
      ["download_file", "读取并保留完成状态", "download_file(&self, query: &DownloadFileQuery, options: &OperationOptions)", "Result<DownloadResult>"],
      ["download_complete", "要求完整下载", "download_complete(&self, query: &DownloadFileQuery, options: &OperationOptions)", "Result<Bytes>"],
    ], "查询文件长度后分块下载。完整下载入口会检查完成状态。"],
    ["mac-get-goods-list", [["goods", "", "goods(&self, query: &GoodsQuery, options: &OperationOptions)", "Result<PageResult<GoodsItem>>"]]],
  ]);

  addEntries("mac/struct.MacExClient.html", "MacExClient", "mac", [
    ["macex-connection", [
      ["builder", "配置客户端", "builder()", "MacExClientBuilder"],
      ["connect", "连接", "connect(&self, options: &OperationOptions)", "Result<()>"],
      ["probe_endpoints", "探测主机", "probe_endpoints(&self, endpoints: &[Endpoint], discovery: &DiscoveryOptions, options: &OperationOptions)", "Result<Vec<HostProbe>>"],
      ["close", "关闭", "close(&self)", "Result<()>"],
    ], "创建 MAC-EX 客户端，管理连接并探测主机。连接需要配置登录应答规则。"],
    ["macex-instruments", [["instruments", "", "instruments(&self, query: &InstrumentsQuery, options: &OperationOptions)", "Result<PageResult<InstrumentInfo>>"]]],
    ["macex-goods-count", [["goods_count", "", "goods_count(&self, query: &GoodsCountQuery, options: &OperationOptions)", "Result<u32>"]]],
    ["macex-goods-list", [["goods", "", "goods(&self, query: &GoodsQuery, options: &OperationOptions)", "Result<PageResult<GoodsItem>>"]]],
    ["macex-quotes", [["quotes", "", "quotes(&self, query: &QuotesQuery, options: &OperationOptions)", "Result<Vec<Quote>>"]]],
    ["macex-goods-quotes-list", [["quote_list", "", "quote_list(&self, query: &QuoteListQuery, options: &OperationOptions)", "Result<PageResult<Quote>>"]]],
    ["macex-bars", [["bars", "", "bars(&self, query: &BarsQuery, options: &OperationOptions)", "Result<Vec<Bar>>"]]],
    ["macex-minute", [["tick_chart", "", "tick_chart(&self, query: &TickChartQuery, options: &OperationOptions)", "Result<TickChart>"]]],
    ["macex-goods-chart-sampling", [["chart_sampling", "", "chart_sampling(&self, query: &ChartSamplingQuery, options: &OperationOptions)", "Result<Vec<WireFloat>>"]]],
    ["macex-trades", [["transactions", "", "transactions(&self, query: &TransactionsQuery, options: &OperationOptions)", "Result<PageResult<Transaction>>"]]],
    ["macex-goods-transaction-all", [["all_transactions", "", "all_transactions(&self, query: &AllTransactionsQuery, options: &OperationOptions)", "Result<Vec<Transaction>>"]]],
    ["macex-macro-indicators", [["macro_indicators", "", "macro_indicators(&self, options: &OperationOptions)", "Result<Vec<GoodsItem>>"]]],
    ["macex-macro-quotes", [["macro_quotes", "", "macro_quotes(&self, query: &MacroQuotesQuery, options: &OperationOptions)", "Result<Vec<Quote>>"]]],
    ["macex-macro-kline", [["macro_bars", "", "macro_bars(&self, query: &MacroBarsQuery, options: &OperationOptions)", "Result<Vec<Bar>>"]]],
  ]);

  var localDocs = {
    "7709-auction-series": "rust-sdk/auction-series.md",
    "7709-block-meta": "rust-sdk/block-meta.md",
    "7709-category-quotes": "rust-sdk/category-quotes.md",
    "7709-code-count": "rust-sdk/code-count.md",
    "7709-code-list": "rust-sdk/code-list.md",
    "7709-company-info-category": "rust-sdk/company-info-category.md",
    "7709-company-info-content": "rust-sdk/company-info-content.md",
    "7709-delisted-bars": "rust-sdk/delisted-bars.md",
    "7709-file-content": "rust-sdk/file-content.md",
    "7709-finance": "rust-sdk/finance.md",
    "7709-gbbq": "rust-sdk/capital-changes.md",
    "7709-handshake": "rust-sdk/handshake.md",
    "7709-heartbeat": "rust-sdk/heartbeat.md",
    "7709-kline": "rust-sdk/kline.md",
    "7709-legacy-quotes": "rust-sdk/legacy-quotes.md",
    "7709-local-factors": "rust-sdk/adjustment-factors.md",
    "7709-minute-aux": "rust-sdk/7709-minute-aux.md",
    "7709-minute-history": "rust-sdk/minute-history.md",
    "7709-minute-recent": "rust-sdk/minute-recent.md",
    "7709-minute-today": "rust-sdk/minute-today.md",
    "7709-money-flow": "rust-sdk/money-flow.md",
    "7709-opening-match-history": "rust-sdk/opening-match-history.md",
    "7709-opening-match-today": "rust-sdk/opening-match-today.md",
    "7709-quote-depth": "rust-sdk/7709-quote-depth.md",
    "7709-quote-refresh": "rust-sdk/quote-refresh.md",
    "7709-quote-snapshots": "rust-sdk/quote-snapshots.md",
    "7709-sparkline": "rust-sdk/sparkline.md",
    "7709-special-limits": "rust-sdk/special-limits.md",
    "7709-trades-history": "rust-sdk/trades-history.md",
    "7709-trades-today": "rust-sdk/trades-today.md",
    "ex-bars": "rust-sdk/ex-bars.md",
    "ex-connection": "rust-sdk/ex-connection.md",
    "ex-count": "rust-sdk/ex-count.md",
    "ex-history-bars": "rust-sdk/ex-history-bars.md",
    "ex-history-minute": "rust-sdk/ex-history-minute.md",
    "ex-history-trades": "rust-sdk/ex-history-trades.md",
    "ex-info": "rust-sdk/ex-info.md",
    "ex-markets": "rust-sdk/ex-markets.md",
    "ex-minute": "rust-sdk/ex-minute.md",
    "ex-quote": "rust-sdk/ex-quote.md",
    "ex-quotes": "rust-sdk/ex-quotes.md",
    "ex-trades": "rust-sdk/ex-trades.md",
    "f10-allotment": "rust-sdk/f10-allotment.md",
    "f10-business-composition": "rust-sdk/f10-business-composition.md",
    "f10-channel-news": "rust-sdk/f10-channel-news.md",
    "f10-company-news": "rust-sdk/f10-company-news.md",
    "f10-company-profile": "rust-sdk/f10-company-profile.md",
    "f10-detail": "rust-sdk/f10-detail.md",
    "f10-dividend-financing": "rust-sdk/f10-dividend-financing.md",
    "f10-finance-diagnosis": "rust-sdk/f10-finance-diagnosis.md",
    "f10-finance-report": "rust-sdk/f10-finance-report.md",
    "f10-generic-entry": "rust-sdk/f10-generic-entry.md",
    "f10-governance": "rust-sdk/f10-governance.md",
    "f10-hot-topics": "rust-sdk/f10-hot-topics.md",
    "f10-limit-board-ladder": "rust-sdk/f10-limit-board-ladder.md",
    "f10-market-news": "rust-sdk/f10-market-news.md",
    "f10-news-cache": "rust-sdk/f10-news-cache.md",
    "f10-news-channels": "rust-sdk/f10-news-channels.md",
    "f10-northbound": "rust-sdk/f10-northbound.md",
    "f10-profit-forecast": "rust-sdk/f10-profit-forecast.md",
    "f10-ranking": "rust-sdk/f10-ranking.md",
    "f10-shareholder-change": "rust-sdk/f10-shareholder-change.md",
    "f10-stock-info": "rust-sdk/f10-stock-info.md",
    "f10-stock-score": "rust-sdk/f10-stock-score.md",
    "f10-theme-market": "rust-sdk/f10-theme-market.md",
    "f10-topic-compare": "rust-sdk/f10-topic-compare.md",
    "f10-valuation": "rust-sdk/f10-valuation.md",
    "helper-auction-data": "rust-sdk/auction-data.md",
    "helper-board-member-quotes": "rust-sdk/board-member-quotes.md",
    "helper-board-quotes": "rust-sdk/board-quotes.md",
    "helper-buy-sell-strength": "rust-sdk/buy-sell-strength.md",
    "helper-daily-price-limits": "rust-sdk/daily-price-limits.md",
    "helper-daily-share-capital": "rust-sdk/daily-share-capital.md",
    "helper-latest-st": "rust-sdk/helper-latest-st.md",
    "helper-latest-stock-list": "rust-sdk/helper-latest-stock-list.md",
    "helper-latest-suspended": "rust-sdk/helper-latest-suspended.md",
    "helper-limit-ladder": "rust-sdk/limit-ladder.md",
    "helper-realtime-rank": "rust-sdk/realtime-rank.md",
    "helper-server-download": "rust-sdk/server-download.md",
    "helper-server-stats": "rust-sdk/server-stats.md",
    "helper-shortline-indicators": "rust-sdk/shortline-indicators.md",
    "helper-stock-profile": "rust-sdk/stock-profile.md",
    "helper-stock-topics": "rust-sdk/stock-topics.md",
    "helper-theme-strength-rank": "rust-sdk/theme-strength-rank.md",
    "helper-topic-stocks": "rust-sdk/topic-stocks.md",
    "helper-volume-comparison": "rust-sdk/volume-comparison.md",
    "mac-bars": "rust-sdk/mac-bars.md",
    "mac-boards": "rust-sdk/mac-boards.md",
    "mac-connection": "rust-sdk/mac-connection.md",
    "mac-download-file": "rust-sdk/mac-download-file.md",
    "mac-download-file-chunk": "rust-sdk/mac-download-file-chunk.md",
    "mac-get-auction": "rust-sdk/mac-get-auction.md",
    "mac-get-belong-board": "rust-sdk/mac-get-belong-board.md",
    "mac-get-board-change-ranking": "rust-sdk/mac-get-board-change-ranking.md",
    "mac-get-board-members": "rust-sdk/mac-get-board-members.md",
    "mac-get-board-ranking": "rust-sdk/mac-get-board-ranking.md",
    "mac-get-board-summary": "rust-sdk/mac-get-board-summary.md",
    "mac-get-capital-flow": "rust-sdk/mac-get-capital-flow.md",
    "mac-get-chart-sampling": "rust-sdk/mac-get-chart-sampling.md",
    "mac-get-file-meta": "rust-sdk/mac-get-file-meta.md",
    "mac-get-goods-list": "rust-sdk/mac-get-goods-list.md",
    "mac-get-kline-offset": "rust-sdk/mac-get-kline-offset.md",
    "mac-get-server-info": "rust-sdk/mac-get-server-info.md",
    "mac-get-symbol-info": "rust-sdk/mac-get-symbol-info.md",
    "mac-get-tick-charts": "rust-sdk/mac-get-tick-charts.md",
    "mac-get-unusual": "rust-sdk/mac-get-unusual.md",
    "mac-minute": "rust-sdk/mac-minute.md",
    "mac-quotes": "rust-sdk/mac-quotes.md",
    "mac-ranking": "rust-sdk/mac-ranking.md",
    "mac-trades": "rust-sdk/mac-trades.md",
    "macex-bars": "rust-sdk/macex-bars.md",
    "macex-connection": "rust-sdk/macex-connection.md",
    "macex-goods-chart-sampling": "rust-sdk/macex-goods-chart-sampling.md",
    "macex-goods-count": "rust-sdk/macex-goods-count.md",
    "macex-goods-list": "rust-sdk/macex-goods-list.md",
    "macex-goods-quotes-list": "rust-sdk/macex-goods-quotes-list.md",
    "macex-goods-transaction-all": "rust-sdk/macex-goods-transaction-all.md",
    "macex-instruments": "rust-sdk/macex-instruments.md",
    "macex-macro-indicators": "rust-sdk/macex-macro-indicators.md",
    "macex-macro-kline": "rust-sdk/macex-macro-kline.md",
    "macex-macro-quotes": "rust-sdk/macex-macro-quotes.md",
    "macex-minute": "rust-sdk/macex-minute.md",
    "macex-quotes": "rust-sdk/macex-quotes.md",
    "macex-trades": "rust-sdk/macex-trades.md",
  };

  return {
    release: {version: version},
    description: "按与 Python 目录相同的业务项浏览 Rust 接口。",
    taxonomy: pythonCatalog.taxonomy,
    items: pythonCatalog.items.map(function (item) {
      var entry = entries[item.id];
      var primary = entry.calls[0];
      var localDoc = localDocs[item.id];
      return {
        id: item.id,
        title: item.title,
        source: item.source,
        category: item.category,
        protocol: item.protocol,
        kind: entry.kind,
        summary: entry.summary || item.summary,
        api: primary.api,
        calls: entry.calls.length > 1 ? entry.calls.map(function (call) {
          return localDoc ? Object.assign({}, call, {doc: localDoc, doc_anchor: ""}) : call;
        }) : undefined,
        return_model: Array.from(new Set(entry.calls.map(function (call) {
          return call.return_model;
        }))).join(" / "),
        doc: localDoc || primary.doc,
        doc_anchor: localDoc ? "" : primary.doc_anchor
      };
    })
  };
})(window.ELTDX_CATALOG);
