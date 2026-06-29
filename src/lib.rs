pub mod cli;
mod client;
mod error;
mod flows;
pub mod mcp;
#[doc(hidden)]
pub mod test_support;
mod types;
mod util;

mod channels;
mod codec;
mod recording;

pub use client::{ConnectionConfig, EdcbClient};
pub use error::{EdcbError, Result};
pub use flows::{
    apply_record_settings_patch, auto_add_data_to_reservation_condition,
    build_reservation_from_event, create_reservation, create_reservation_condition,
    create_reservation_with_options, delete_reservation, delete_reservation_condition,
    get_recording_defaults, get_recording_presets, get_reservation, get_reservation_condition,
    get_timetable, list_channels, list_reservation_conditions, preview_reservation,
    preview_reservation_with_options, program_search_query_from_search_key,
    program_search_query_to_search_key, record_settings_from_rec_setting, search_programs,
    update_reservation, update_reservation_condition,
};
pub use types::{
    AudioComponentInfo, AudioComponentInfoData, AutoAddData, BestEffortStatus, BroadcastType,
    ChSet5Item, Channel, ChannelList, ChannelType, ComponentInfo, ContentData, ContentInfo,
    DuplicateTitleCheckScope, EventData, EventGroupInfo, EventInfo, EventKey, ExtendedEventInfo,
    FileData, ManualAutoAddData, NotifySrvInfo, NwPlayTimeShiftInfo, PluginKind, PostRecordingMode,
    ProgramGenreRange, ProgramSearchQuery, RecFileInfo, RecFileSetInfo, RecSettingData,
    RecordSettings, RecordSettingsGlobalDefaults, RecordSettingsPatch, RecordSettingsPreset,
    RecordSettingsPresets, RecordingAvailability, RecordingFolder, RecordingMode,
    ReservationCondition, ReservationStatus, ReserveData, SearchDateInfo, SearchKeyInfo,
    ServiceEventInfo, ServiceInfo, ServiceKey, ServiceRecordingMode, ShortEventInfo, TimeTable,
    TimeTableChannel, TimeTableDateRange, TimeTableProgram, TimeTableProgramReservation,
    TimeTableQuery, TimeTableSubchannel, TunerProcessStatusInfo, TunerReserveInfo,
};
pub use util::{
    convert_bytes_to_string, datetime_to_file_time, get_logo_file_name_from_directory_index,
    get_logo_id_from_logo_data_ini, parse_ch_set5, parse_program_extended_text,
};
