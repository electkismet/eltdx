#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Source-only public HQ company contracts; no HTTP F10 dependency.
#[path = "support/hq_server.rs"]
mod hq_server;
use eltdx::{CompanySectionQuery, ErrorKind, OperationOptions, RawPolicy, SecurityId, TdxClient};
fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap()
}

#[test]
fn strict_filename_is_bounded_by_encoded_bytes_and_range_is_checked() {
    assert!(CompanySectionQuery::new("公".repeat(40), 0, 1).is_ok());
    for filename in [
        "公".repeat(41),
        "😀".to_owned(),
        "a\0b".to_owned(),
        " ".to_owned(),
        "€".to_owned(),
    ] {
        assert_eq!(
            CompanySectionQuery::new(filename, 0, 1).unwrap_err().kind(),
            ErrorKind::InvalidArgument
        );
    }
    assert!(CompanySectionQuery::new("a".to_owned(), u32::MAX, 1).is_err());
    assert!(CompanySectionQuery::new("a".to_owned(), 0, 0).is_err());
    assert!(CompanySectionQuery::new("a".to_owned(), u32::MAX - 1, 1).is_ok());
}

#[test]
fn catalog_uses_fixed_gbk_byte_fields_and_content_preserves_loss_diagnostics() {
    let server = hq_server::Server::start(|request| match request.msg_type {
        0x02cf => {
            let mut bytes = vec![1, 0];
            let mut record = [0; 152];
            record[..4].copy_from_slice(b"\xb9\xab\xcb\xbe");
            record[64..69].copy_from_slice(b"a.txt");
            record[144..148].copy_from_slice(&7_u32.to_le_bytes());
            record[148..152].copy_from_slice(&6_u32.to_le_bytes());
            bytes.extend_from_slice(&record);
            bytes
        }
        0x02d0 => {
            assert_eq!(&request.data[10..14], b"\xb9\xab\xb9\xab");
            assert_eq!(&request.data[90..94], &7_u32.to_le_bytes());
            assert_eq!(&request.data[94..98], &6_u32.to_le_bytes());
            let mut bytes = vec![0; 12];
            bytes[10..12].copy_from_slice(&6_u16.to_le_bytes());
            bytes.extend_from_slice(b"A\xb9\xab\x80B\0");
            bytes
        }
        _ => panic!("company info should not fetch metadata"),
    })
    .unwrap();
    let client = client(&server);
    let id: SecurityId = "sh600000".parse().unwrap();
    let catalog = client
        .company_info()
        .category(
            &id,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        )
        .unwrap();
    assert_eq!(catalog.rows().len(), 1);
    let row = &catalog.rows()[0];
    assert_eq!(row.name(), "公司");
    assert_eq!(row.filename(), "a.txt");
    assert_eq!(row.offset(), 7);
    assert_eq!(row.length(), 6);
    assert_eq!(catalog.meta().raw().unwrap().row_ranges(), &[(0, 2, 152)]);
    let value = client
        .company_info()
        .content(
            &id,
            &CompanySectionQuery::new("公公".to_owned(), 7, 6).unwrap(),
            &OperationOptions::default(),
        )
        .unwrap();
    assert_eq!(value.text(), "A公�B\0");
    assert!(value.decoding_had_errors());
    assert_eq!(value.offset(), 7);
    assert!(value.meta().raw().is_none());
    assert!(value
        .meta()
        .diagnostics()
        .iter()
        .any(|value| value.stage() == "gbk"));
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn truncated_company_payload_is_a_protocol_failure() {
    for mode in 0..2 {
        let server = hq_server::Server::start(move |_| {
            if mode == 0 {
                vec![1, 0]
            } else {
                let mut bytes = vec![0; 12];
                bytes[10..12].copy_from_slice(&3_u16.to_le_bytes());
                bytes.push(b'x');
                bytes
            }
        })
        .unwrap();
        let client = client(&server);
        let id: SecurityId = "sh600000".parse().unwrap();
        let options = OperationOptions::default();
        let kind = if mode == 0 {
            client
                .company_info()
                .category(&id, &options)
                .unwrap_err()
                .kind()
        } else {
            client
                .company_info()
                .content(
                    &id,
                    &CompanySectionQuery::new("a".to_owned(), 0, 10).unwrap(),
                    &options,
                )
                .unwrap_err()
                .kind()
        };
        assert_eq!(kind, ErrorKind::Protocol);
        client.close().unwrap();
        server.finish().unwrap();
    }
}
