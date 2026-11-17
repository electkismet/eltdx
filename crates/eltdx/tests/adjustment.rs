//! Independently calculated affine expectations, authored without execution.
use eltdx::equity::build_adjustment_factors;
use eltdx::types::{AdjustMode, ResultMeta};
use eltdx::{CapitalChangeBlock, CapitalEvent, FactorQuery, TradingDate};

fn date(day: u8) -> TradingDate {
    TradingDate::new(2026, 6, day).unwrap()
}
fn event(day: u8, values: [f32; 4]) -> CapitalEvent {
    CapitalEvent::from_wire(
        20260600 + u32::from(day),
        1,
        values.map(f32::to_le_bytes),
        9,
    )
}
fn block(events: Vec<CapitalEvent>) -> CapitalChangeBlock {
    CapitalChangeBlock::new(
        "sh600000".parse().unwrap(),
        events,
        7,
        ResultMeta::default(),
    )
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "{actual} differs from {expected}"
    );
}

#[test]
fn same_date_events_keep_their_source_order_in_both_directions() {
    // Day1: (m=2,o=1), then (m=3,o=4). Day2: (m=5,o=2).
    let changes = block(vec![
        event(1, [10., 0., 10., 0.]),
        event(1, [40., 0., 20., 0.]),
        event(2, [20., 0., 40., 0.]),
    ]);
    let factors = build_adjustment_factors(&changes, &FactorQuery::new()).unwrap();
    assert_eq!(
        factors
            .rows()
            .iter()
            .map(|row| row.date())
            .collect::<Vec<_>>(),
        vec![date(1), date(2)]
    );
    let first = &factors.rows()[0];
    let second = &factors.rows()[1];
    close(first.qfq_scale(), 1. / 30.);
    close(first.qfq_offset(), -0.7);
    close(first.hfq_scale(), 6.);
    close(first.hfq_offset(), 7.);
    close(second.qfq_scale(), 0.2);
    close(second.qfq_offset(), -0.4);
    close(second.hfq_scale(), 30.);
    close(second.hfq_offset(), 19.);
    let price = eltdx::adjustment::adjust_price(
        eltdx::Price::scaled_integer(1001, 2),
        first,
        AdjustMode::FixedQfq,
    )
    .unwrap();
    close(price.as_f64(), 10.01 / 30. - 0.7);
    assert!(
        matches!(price.value(), eltdx::NumericValue::F64(_)),
        "derived prices do not claim exact original scale"
    );
}

#[test]
fn anchor_and_start_filter_events_with_the_frozen_inclusive_windows() {
    let changes = block(vec![
        event(1, [10., 0., 10., 0.]),
        event(2, [20., 0., 40., 0.]),
    ]);
    let anchored = build_adjustment_factors(
        &changes,
        &FactorQuery::new().with_anchor_date(Some(date(1))).unwrap(),
    )
    .unwrap();
    close(anchored.rows()[0].qfq_scale(), 0.5);
    close(anchored.rows()[0].qfq_offset(), -0.5);
    close(anchored.rows()[1].qfq_scale(), 1.);
    close(anchored.rows()[1].qfq_offset(), 0.);
    let windowed = build_adjustment_factors(
        &changes,
        &FactorQuery::new().with_start_date(Some(date(2))).unwrap(),
    )
    .unwrap();
    assert_eq!(windowed.rows().len(), 1);
    close(windowed.rows()[0].hfq_scale(), 5.);
    close(windowed.rows()[0].hfq_offset(), 2.);
    assert!(FactorQuery::new()
        .with_start_date(Some(date(2)))
        .unwrap()
        .with_anchor_date(Some(date(1)))
        .is_err());
}

#[test]
fn invalid_dates_and_other_tags_are_preserved_but_do_not_affect_factors() {
    let nan = f32::from_bits(0x7fc12345).to_le_bytes();
    let invalid = CapitalEvent::from_wire(20260230, 1, [nan; 4], 17);
    let unknown = CapitalEvent::from_wire(20260601, 255, [nan; 4], 23);
    assert!(invalid.date().is_none());
    assert_eq!(invalid.date_raw(), 20260230);
    assert_eq!(unknown.coefficient_bits()[0], nan);
    assert_eq!(unknown.reserved(), 23);
    assert!(
        build_adjustment_factors(&block(vec![invalid, unknown]), &FactorQuery::new())
            .unwrap()
            .rows()
            .is_empty()
    );
    for values in [
        [f32::NAN, 0., 0., 0.],
        [0., 0., -10., 0.],
        [0., f32::INFINITY, 1., 1.],
    ] {
        assert_eq!(
            build_adjustment_factors(&block(vec![event(1, values)]), &FactorQuery::new())
                .unwrap_err()
                .kind(),
            eltdx::ErrorKind::InvalidArgument
        );
    }
}
