pub mod cli;
pub mod client;
pub mod error;
pub mod flows;
pub mod mcp;
#[doc(hidden)]
pub mod test_support;
pub mod types;
pub mod util;

mod codec;

pub use client::{EdcbClient, PluginKind};
pub use error::{EdcbError, Result};
pub use types::{EventKey, ProgramSearchQuery, ServiceKey};
