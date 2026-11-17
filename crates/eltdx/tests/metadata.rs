//! Shared DTO source cases. Private cache/flight cases live beside metadata::store.
use eltdx::types::{Diagnostic, EnrichmentStatus, ResultMeta, SourceStamp};
use eltdx::ErrorKind;

#[test]
fn frozen_diagnostic_constructor_and_readonly_result_provenance() {
    let diagnostic = Diagnostic::new(
        "hq-security-list",
        "metadata",
        EnrichmentStatus::Failed,
        Some(ErrorKind::MetadataUnavailable),
        Some("incomplete catalog".to_owned()),
    );
    assert_eq!(diagnostic.source(), "hq-security-list");
    assert_eq!(diagnostic.stage(), "metadata");
    assert_eq!(diagnostic.status(), EnrichmentStatus::Failed);
    assert_eq!(
        diagnostic.error_kind(),
        Some(ErrorKind::MetadataUnavailable)
    );
    assert_eq!(diagnostic.message(), Some("incomplete catalog"));
    let source = SourceStamp::new(None, None, "owner-1", Some(7), None);
    let meta = ResultMeta::new(source, vec![diagnostic], None);
    assert_eq!(meta.source().metadata_version(), Some(7));
    assert_eq!(meta.source().command(), None);
    assert_eq!(meta.diagnostics().len(), 1);
}
