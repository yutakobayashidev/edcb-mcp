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

pub use client::{ConnectionConfig, EdcbClient};
pub use error::{EdcbError, Result};
pub use types::{
    BroadcastType, ChannelType, DuplicateTitleCheckScope, EventKey, PluginKind, PostRecordingMode,
    ProgramGenreRange, ProgramSearchQuery, RecordSettingsPatch, RecordingAvailability,
    RecordingFolder, RecordingMode, ReservationCondition, ReservationStatus, SearchDateInfo,
    SearchKeyInfo, ServiceKey, ServiceRecordingMode, TimeTable, TimeTableChannel,
    TimeTableDateRange, TimeTableProgram, TimeTableProgramReservation, TimeTableQuery,
    TimeTableSubchannel,
};
