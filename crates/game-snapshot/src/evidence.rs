//! Shared evidence dictionaries retain private stamps and namespace-sensitive payloads.
mod archive;
mod view;
pub use archive::Archive;
pub use view::View;
mod parts;
pub use parts::Parts;
