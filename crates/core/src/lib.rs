//! Core for niri-settings: config model, safe writes, history, checks and
//! imports. This crate has no UI code and is the only crate that touches the
//! config or runs `niri`.

pub mod doc;
pub mod history;
pub mod import;
pub mod journal;
pub mod lint;
pub mod location;
pub mod niri;
pub mod presets;
pub mod schema;
pub mod schemes;
pub mod session;
pub mod value;
pub mod watch;

pub use doc::{LoadError, Managed, Tree};
pub use history::{History, HistoryEntry, HistoryFile};
pub use journal::Recovery;
pub use schema::{Control, OPTIONS, OptionInfo, PAGES, Page, ToggleStyle, Unit, option};
pub use schemes::{SCHEMES, Scheme, scheme};
pub use session::{
    Applied, ApplyError, Resolved, Session, SessionError, SessionOptions, Source, TextChange,
};
pub use value::Value;
pub use watch::ConfigWatcher;

pub use location::{Location, OpenError, locate, make_personal_copy};
