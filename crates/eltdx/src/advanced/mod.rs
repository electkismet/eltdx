//! Selected protocol tools. These pure codecs do not create a socket, admit a
//! network operation or grant access outside a client's owner and budgets.
#[cfg(feature = "mac")]
pub mod mac;
