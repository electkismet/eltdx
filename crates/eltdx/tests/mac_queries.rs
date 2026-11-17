// Authored source only; execution waits for the complete T01-T30 source gate.
#![cfg(feature = "mac")]
use eltdx::{ex, mac, types, DownloadOptions, ErrorKind, PageOptions, TradingDate};
use mac::{
    ex_query as mx,
    models::{MacAdjust, MacCode, MacInstrumentId, MacMarketId, MacPeriod},
    query as mq,
};
use mac::{
    BoardSymbol, BoardType, Category, FieldId, FieldSelection, FilterMask, FilterType, PresetField,
    SortOrder, SortType,
};
use std::num::{NonZeroU16, NonZeroU32};

fn mac_id() -> MacInstrumentId {
    MacInstrumentId::new(MacMarketId::new(0), MacCode::new("600000").unwrap())
}
fn ex_id(market: u8, code: &str) -> ex::ExInstrumentId {
    ex::ExInstrumentId::new(
        ex::ExMarketId::from_wire(market),
        ex::ExCode::new(code).unwrap(),
    )
}

#[test]
fn explicit_identity_conversion_preserves_namespace_and_rejects_loss() {
    for market in [0_u8, 1, 2, 31, 38, 255] {
        let original = ex_id(market, "00700");
        let converted = MacInstrumentId::try_from(&original).unwrap();
        assert_eq!(converted.market().get(), u16::from(market));
        assert_eq!(converted.code().as_str(), "00700");
        assert_eq!(ex::ExInstrumentId::try_from(&converted).unwrap(), original);
    }
    let wide_market = MacInstrumentId::new(MacMarketId::new(256), MacCode::new("00700").unwrap());
    assert_eq!(
        ex::ExInstrumentId::try_from(&wide_market)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    let long_utf8 = MacInstrumentId::new(MacMarketId::new(31), MacCode::new("一二三四").unwrap());
    assert!(ex::ExInstrumentId::try_from(&long_utf8).is_err());
    assert!(MacInstrumentId::try_from(&ex_id(31, "😀")).is_err());
    let chinese = ex_id(38, "一二三");
    assert_eq!(
        MacInstrumentId::try_from(&chinese)
            .unwrap()
            .code()
            .encoded()
            .len(),
        6
    );
}

#[test]
fn board_symbols_retain_caller_text_and_checked_source_numbers() {
    for (source, expected) in [
        ("US0401", 30401),
        ("HK0283", 20283),
        ("000686", 31686),
        ("399372", 30372),
        ("899050", 32050),
        ("880686", 20686),
        ("  HK0283  ", 20283),
    ] {
        let symbol = BoardSymbol::new(source).unwrap();
        assert_eq!(symbol.as_str(), source);
        assert_eq!(symbol.wire_code(), expected);
    }
    for source in [
        "",
        " ",
        "us0401",
        "hk0283",
        "-1",
        "+1",
        "ＨＫ0283",
        "US4294967295",
        "4294967296",
    ] {
        assert_eq!(
            BoardSymbol::new(source).unwrap_err().kind(),
            ErrorKind::InvalidArgument
        );
    }
}

#[test]
fn filter_duplicates_are_or_ed_and_registry_numbers_remain_distinct() {
    let mask = FilterMask::from_flags([FilterType::Kc, FilterType::Kc, FilterType::Registration]);
    assert_eq!(mask.bits(), 130);
    assert!(!mask.contains(FilterType::St));
    assert!(mask.contains(FilterType::Registration));
    assert_eq!(FilterMask::from_bits(255).unwrap().bits(), 255);
    assert!(FilterMask::from_bits(256).is_err());
    assert_eq!(SortType::Amount2m.wire(), 0x10c);
    assert_eq!(SortType::OpenSnatchPct.wire(), 0x10a);
    assert_eq!(Category::Sz.wire(), 2);
    assert_eq!(Category::BoardGn.wire(), 10004);
    assert_eq!(Category::Zs.wire(), 0x2b2c);
    assert_eq!(BoardType::All.wire(), 255);
    assert!(BoardType::from_wire(2).is_err());
    assert!(Category::from_wire(1).is_err());
    assert!(SortType::from_wire(0x0d).is_err());
    assert!(SortOrder::from_wire(3).is_err());
}

#[test]
fn mac_target_count_is_not_a_wire_page_u16() {
    let bars = mq::BarsQuery::new(mac_id()).with_count(100000).unwrap();
    assert_eq!(bars.count().get(), 100000);
    assert_eq!(bars.pages().page_size(), 700);
    assert_eq!(bars.pages().max_pages(), 200);
    assert_eq!(bars.pages().max_records(), 140000);
    let quotes = mq::QuoteListQuery::new(Category::A)
        .with_count(100000)
        .unwrap();
    assert_eq!(quotes.count().get(), 100000);
    assert_eq!(quotes.pages().page_size(), 80);
    let members = mq::BoardMembersQuery::new(BoardSymbol::new("880686").unwrap());
    assert_eq!(members.count().get(), 100000);
    assert_eq!(members.pages().max_pages(), 2000);
    let trades = mq::TransactionsQuery::new(mac_id());
    assert_eq!(trades.count().get(), 2000);
    assert_eq!(trades.pages().page_size(), 1000);
    assert!(bars.clone().with_start(u32::MAX).is_err());
    assert!(bars.with_count(0).is_err());
    assert!(quotes.with_count(0).is_err());
}

#[test]
fn request_bitmaps_and_gbk_subfields_validate_at_query_construction() {
    assert!(mq::QuotesQuery::new(vec![]).is_err());
    assert!(mq::QuotesQuery::new(vec![mac_id(); 81]).is_err());
    let duplicate = mq::QuotesQuery::new(vec![mac_id(), mac_id()]).unwrap();
    assert_eq!(duplicate.ids().len(), 2);
    let high = FieldSelection::from_ids([FieldId::new(128).unwrap()]);
    assert!(duplicate.with_fields(high.clone()).is_ok());
    assert!(mq::QuoteListQuery::new(Category::A)
        .with_fields(high.clone())
        .is_err());
    assert!(
        mq::BoardMembersQuery::new(BoardSymbol::new("880686").unwrap())
            .with_fields(high)
            .is_err()
    );
    let full = MacInstrumentId::new(MacMarketId::new(74), MacCode::new("123456789").unwrap());
    assert!(mq::BelongingBoardsQuery::new(full.clone()).is_err());
    assert!(mq::CapitalFlowQuery::new(full.clone()).is_err());
    assert_eq!(
        mq::SymbolInfoQuery::new(full).id().code().as_str(),
        "123456789"
    );
}

#[test]
fn mac_period_duration_uses_five_minute_multiplier_and_keeps_seconds() {
    let times = NonZeroU16::new(3).unwrap();
    assert_eq!(MacPeriod::Mins.duration_seconds(times), Some(900));
    assert_eq!(MacPeriod::Minute5.duration_seconds(times), Some(300));
    assert_eq!(MacPeriod::Seconds.duration_seconds(times), Some(3));
    assert_eq!(MacPeriod::Monthly.duration_seconds(times), None);
    assert!(MacPeriod::known(12).is_err());
    assert!(mq::BarsQuery::new(mac_id())
        .with_period(MacPeriod::Wire(12))
        .unwrap()
        .with_bar_time(types::BarTime::End)
        .is_err());
    assert!(mq::BarsQuery::new(mac_id()).with_times(0).is_err());
    assert!(mq::BarsQuery::new(mac_id()).with_times(65536).is_err());
    let seconds = mq::BarsQuery::new(mac_id())
        .with_period(MacPeriod::Seconds)
        .unwrap()
        .with_times(10)
        .unwrap()
        .with_bar_time(types::BarTime::End)
        .unwrap();
    assert_eq!(seconds.period().duration_seconds(seconds.times()), Some(10));
    assert_eq!(seconds.adjust(), MacAdjust::None);
}

#[test]
fn dates_day_slots_goods_and_download_ranges_are_checked_locally() {
    assert!(mq::TickChartsQuery::new(mac_id()).with_days(0).is_err());
    assert!(mq::TickChartsQuery::new(mac_id()).with_days(6).is_err());
    let leap = TradingDate::new(2024, 2, 29).unwrap();
    assert_eq!(
        mq::TickChartsQuery::new(mac_id())
            .with_date(Some(leap))
            .unwrap()
            .date(),
        Some(leap)
    );
    assert!(mq::GoodsQuery::new(MacMarketId::new(38))
        .with_count(1001)
        .is_err());
    assert!(mq::UnusualQuery::new(MacMarketId::new(0))
        .with_start(65536)
        .is_err());
    let name = mac::RemoteFileName::new("a".repeat(70)).unwrap();
    assert!(mq::DownloadChunkQuery::new(name.clone())
        .with_index(0)
        .is_err());
    assert!(mq::DownloadChunkQuery::new(name.clone())
        .with_size(30001)
        .is_err());
    assert!(mq::DownloadChunkQuery::new(name.clone())
        .with_offset(u32::MAX)
        .is_err());
    let none = DownloadOptions::default().with_max_bytes(0).unwrap();
    assert_eq!(
        mq::DownloadFileQuery::new(name.clone())
            .with_options(none)
            .unwrap()
            .options()
            .max_bytes(),
        0
    );
    let large = DownloadOptions::default()
        .with_chunk_size(NonZeroU32::new(30001).unwrap())
        .unwrap();
    assert!(mq::DownloadFileQuery::new(name)
        .with_options(large)
        .is_err());
}

#[test]
fn mac_ex_seven_defaults_and_source_order_do_not_claim_sorted_hundred() {
    let quotes = mx::QuotesQuery::new(vec![ex_id(31, "00700")]).unwrap();
    assert_eq!(
        quotes
            .fields()
            .ids()
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 7]
    );
    let list = mx::QuoteListQuery::new(ex::ExMarketId::from_wire(31));
    assert_eq!(list.count().get(), 100);
    assert_eq!(list.requested_goods(), 80);
    assert!(list
        .clone()
        .with_sort(SortType::ChangePct, SortOrder::Desc)
        .is_err());
    assert!(list
        .clone()
        .with_sort(SortType::Code, SortOrder::None)
        .is_ok());
    assert_eq!(list.with_count(5).unwrap().requested_goods(), 5);
    assert!(mx::QuotesQuery::new(vec![ex_id(31, "😀")]).is_err());
    assert!(mx::QuotesQuery::new(vec![ex_id(31, "00700"); 81]).is_err());
    let custom = mx::QuotesQuery::new(vec![ex_id(31, "00700")])
        .unwrap()
        .with_fields(FieldSelection::preset(PresetField::Volume))
        .unwrap()
        .with_ids(vec![ex_id(47, "IFL0")])
        .unwrap();
    assert_eq!(
        custom.fields(),
        &FieldSelection::preset(PresetField::Volume)
    );
}

#[test]
fn hk_trade_queries_have_distinct_route_sizes_signed_offsets_and_all_cap() {
    for market in [27_u8, 31, 48, 49, 71, 98] {
        let query = mx::TransactionsQuery::new(ex_id(market, "00700")).unwrap();
        assert_eq!(query.wire_page_maximum(), 1800);
        assert_eq!(query.pages().page_size(), 1800);
        assert_eq!(query.count().get(), 2000);
        assert!(query.with_start(i32::MAX as u32).is_err());
        let all = mx::AllTransactionsQuery::new(ex_id(market, "00700")).unwrap();
        assert_eq!(all.pages().max_pages(), 50);
        assert_eq!(all.pages().max_records(), 90000);
        assert!(all
            .clone()
            .with_pages(PageOptions::new(1800, 51, 90000).unwrap())
            .is_err());
        assert!(all
            .with_pages(PageOptions::new(1800, 50, 90001).unwrap())
            .is_err());
    }
    for market in [23_u8, 24, 25, 26, 47] {
        let query = mx::TransactionsQuery::new(ex_id(market, "IFL0")).unwrap();
        assert_eq!(query.pages().page_size(), 1000);
        assert!(mx::AllTransactionsQuery::new(ex_id(market, "IFL0")).is_err());
    }
    let hk_query = mx::TransactionsQuery::new(ex_id(31, "00700")).unwrap();
    assert!(hk_query.clone().with_id(ex_id(47, "IFL0")).is_err());
    assert!(hk_query
        .with_pages(PageOptions::new(1000, 100, 100000).unwrap())
        .unwrap()
        .with_id(ex_id(47, "IFL0"))
        .is_ok());
}

#[test]
fn macro_query_normalizes_only_whitespace_and_uses_monthly_bars() {
    let code = mx::MacroCode::new("  1_GDP  ").unwrap();
    assert_eq!(code.as_str(), "1_GDP");
    assert!(code.is_full_code());
    assert!(!mx::MacroCode::new("GDP").unwrap().is_full_code());
    assert_eq!(mx::MacroCode::new("gdp").unwrap().as_str(), "gdp");
    assert!(mx::MacroCode::new(" ").is_err());
    assert!(mx::MacroQuotesQuery::new(vec![]).is_err());
    let repeated = mx::MacroQuotesQuery::new(vec![code.clone(), code.clone()]).unwrap();
    assert_eq!(repeated.codes().len(), 2);
    assert_eq!(
        repeated
            .fields()
            .ids()
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 7]
    );
    let bars = mx::MacroBarsQuery::new(code).unwrap();
    assert_eq!(bars.period(), MacPeriod::Monthly);
    assert_eq!(bars.count().get(), 800);
    assert_eq!(bars.pages().page_size(), 700);
}
