//! Signed updates: state, policy, check, download and installation have separate responsibilities.
pub(crate) mod check;
pub(crate) mod download;
mod errors;
pub(crate) mod install;
mod policy;
mod state;
pub use check::{check_updates, get_update_state};
pub use download::download_update;
use errors::error_message;
pub use install::install_update;
pub use policy::maybe_check_automatically;
use state::broadcast;
pub use state::{UpdateManager, UpdatePhase, UpdateState};
