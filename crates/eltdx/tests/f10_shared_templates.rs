//! Shared fixed-request definitions. Execution is deferred to the final gate.
#![cfg(feature = "f10")]

use eltdx::f10::{simple_endpoint, BondCode, MgTipsQuery, RequestPayload,
    SectionCode, SimpleArgument, UsTicker};

#[test]
fn fixed_definition_and_typed_query_share_nondefault_arguments() -> eltdx::Result<()> {
    let definition = simple_endpoint("mg_tips").unwrap();
    let values = definition.compose(|slot| Ok::<_, std::convert::Infallible>(match slot {
        SimpleArgument::Literal(value) => eltdx::f10::JsonValue::from(value),
        SimpleArgument::Code => eltdx::f10::JsonValue::from("AAPL"),
        SimpleArgument::Section => eltdx::f10::JsonValue::from("sypl"),
        SimpleArgument::Market => unreachable!(),
    })).unwrap();
    let query = MgTipsQuery::new(UsTicker::new("AAPL")?)
        .with_section(SectionCode::new("mg_tips", "sypl")?)?.to_call()?;
    assert_eq!(query.entry().as_str(), definition.entry);
    assert_eq!(query.payload(), &RequestPayload::Params(values));
    assert_eq!(definition.code_helper, Some("_us_code"));
    assert_eq!(definition.valid_sections, Some(&["zxdt", "sypl", "gsgy", "zysj"][..]));
    Ok(())
}

#[test]
fn bond_templates_keep_market_and_legacy_helper_differences() -> eltdx::Result<()> {
    let call = eltdx::f10::GzProfileQuery::new(BondCode::new("sz123001")?).to_call()?;
    assert_eq!(call.entry().as_str(), "CWServ.skef10_bond_gz_zjgk");
    assert_eq!(call.payload(), &RequestPayload::Params(vec!["zqgk".into(), "123001".into(), "0".into()]));
    assert_eq!(simple_endpoint("gz_profile").unwrap().code_helper, Some("_bond_code"));
    assert_eq!(simple_endpoint("bond_profile").unwrap().code_helper, Some("_code6"));
    assert_eq!(simple_endpoint("bond_bulletin").unwrap().code_helper, Some("_bond_code"));
    assert_eq!(simple_endpoint("gzhg_quotes").unwrap().code_helper, Some("_code6"));
    assert!(simple_endpoint("hy_profile").unwrap().code_helper.is_none());
    assert!(simple_endpoint("qh_inventory").unwrap().code_helper.is_none());
    assert!(simple_endpoint("unregistered").is_none());
    Ok(())
}

#[test]
fn shared_argument_visitor_stops_at_the_original_error() {
    let definition = simple_endpoint("gz_profile").unwrap();
    let mut visited = Vec::new();
    let result = definition.compose(|slot| {
        visited.push(slot);
        match slot {
            SimpleArgument::Code => Err("conversion"),
            _ => Ok(()),
        }
    });
    assert_eq!(result, Err("conversion"));
    assert!(matches!(visited.as_slice(), [SimpleArgument::Section, SimpleArgument::Code]));
}
