#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum RawPolicy {
    #[default]
    Omit,
    RetainPayload,
    RetainWire,
}
