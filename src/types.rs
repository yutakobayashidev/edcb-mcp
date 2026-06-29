use std::str::FromStr;

use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ServiceKey {
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
}

impl ServiceKey {
    pub fn to_search_id(self) -> i64 {
        (i64::from(self.onid) << 32) | (i64::from(self.tsid) << 16) | i64::from(self.sid)
    }
}

impl FromStr for ServiceKey {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = value.split(':').collect();
        if parts.len() != 3 {
            return Err(format!("service key must be onid:tsid:sid, got {value}"));
        }
        Ok(Self {
            onid: parse_key_part(parts[0], "onid")?,
            tsid: parse_key_part(parts[1], "tsid")?,
            sid: parse_key_part(parts[2], "sid")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ChannelType {
    #[serde(rename = "GR")]
    Gr,
    #[serde(rename = "BS")]
    Bs,
    #[serde(rename = "CS")]
    Cs,
    #[serde(rename = "CATV")]
    Catv,
    #[serde(rename = "SKY")]
    Sky,
    #[serde(rename = "BS4K")]
    Bs4k,
}

impl FromStr for ChannelType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "gr" => Ok(Self::Gr),
            "bs" => Ok(Self::Bs),
            "cs" => Ok(Self::Cs),
            "catv" => Ok(Self::Catv),
            "sky" => Ok(Self::Sky),
            "bs4k" => Ok(Self::Bs4k),
            _ => Err(format!(
                "channel type must be gr, bs, cs, catv, sky, or bs4k: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EventKey {
    pub service: ServiceKey,
    pub eid: u16,
}

impl FromStr for EventKey {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = value.split(':').collect();
        if parts.len() != 4 {
            return Err(format!("event key must be onid:tsid:sid:eid, got {value}"));
        }
        Ok(Self {
            service: ServiceKey {
                onid: parse_key_part(parts[0], "onid")?,
                tsid: parse_key_part(parts[1], "tsid")?,
                sid: parse_key_part(parts[2], "sid")?,
            },
            eid: parse_key_part(parts[3], "eid")?,
        })
    }
}

fn parse_key_part(value: &str, name: &str) -> Result<u16, String> {
    value
        .parse()
        .map_err(|_| format!("{name} must be a number in 0..=65535: {value}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChSet5Item {
    pub service_name: String,
    pub network_name: String,
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub service_type: u8,
    pub partial_flag: bool,
    pub epg_cap_flag: bool,
    pub search_flag: bool,
    pub remocon_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceInfo {
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub service_type: u8,
    pub partial_reception_flag: u8,
    pub service_provider_name: String,
    pub service_name: String,
    pub network_name: String,
    pub ts_name: String,
    pub remote_control_key_id: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileData {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecFileSetInfo {
    pub rec_folder: String,
    pub write_plug_in: String,
    pub rec_name_plug_in: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecSettingData {
    pub rec_mode: u8,
    pub priority: u8,
    pub tuijyuu_flag: bool,
    pub service_mode: u32,
    pub pittari_flag: bool,
    pub bat_file_path: String,
    pub rec_folder_list: Vec<RecFileSetInfo>,
    pub suspend_mode: u8,
    pub reboot_flag: bool,
    pub start_margin: Option<i32>,
    pub end_margin: Option<i32>,
    pub continue_rec_flag: bool,
    pub partial_rec_flag: u8,
    pub tuner_id: u32,
    pub partial_rec_folder: Vec<RecFileSetInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum RecordingMode {
    AllServices,
    AllServicesWithoutDecoding,
    SpecifiedService,
    SpecifiedServiceWithoutDecoding,
    View,
}

impl FromStr for RecordingMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "all" | "allservices" => Ok(Self::AllServices),
            "allwithoutdecoding" | "allserviceswithoutdecoding" => {
                Ok(Self::AllServicesWithoutDecoding)
            }
            "specified" | "specifiedservice" => Ok(Self::SpecifiedService),
            "specifiedwithoutdecoding" | "specifiedservicewithoutdecoding" => {
                Ok(Self::SpecifiedServiceWithoutDecoding)
            }
            "view" => Ok(Self::View),
            _ => Err(format!(
                "recording mode must be all, all-without-decoding, specified, specified-without-decoding, or view: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ServiceRecordingMode {
    Default,
    Enable,
    Disable,
}

impl FromStr for ServiceRecordingMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "default" => Ok(Self::Default),
            "enable" | "enabled" => Ok(Self::Enable),
            "disable" | "disabled" => Ok(Self::Disable),
            _ => Err(format!(
                "service recording mode must be default, enable, or disable: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum PostRecordingMode {
    Default,
    Nothing,
    Standby,
    StandbyAndReboot,
    Suspend,
    SuspendAndReboot,
    Shutdown,
}

impl FromStr for PostRecordingMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "default" => Ok(Self::Default),
            "nothing" => Ok(Self::Nothing),
            "standby" => Ok(Self::Standby),
            "standbyandreboot" => Ok(Self::StandbyAndReboot),
            "suspend" => Ok(Self::Suspend),
            "suspendandreboot" => Ok(Self::SuspendAndReboot),
            "shutdown" => Ok(Self::Shutdown),
            _ => Err(format!(
                "post-recording mode must be default, nothing, standby, standby-and-reboot, suspend, suspend-and-reboot, or shutdown: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingFolder {
    pub recording_folder_path: String,
    pub recording_file_name_template: Option<String>,
    #[serde(default)]
    pub is_oneseg_separate_recording_folder: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordSettingsPatch {
    pub is_enabled: Option<bool>,
    pub priority: Option<u8>,
    pub recording_folders: Option<Vec<RecordingFolder>>,
    pub recording_start_margin: Option<i32>,
    pub recording_end_margin: Option<i32>,
    pub recording_mode: Option<RecordingMode>,
    pub caption_recording_mode: Option<ServiceRecordingMode>,
    pub data_broadcasting_recording_mode: Option<ServiceRecordingMode>,
    pub post_recording_mode: Option<PostRecordingMode>,
    pub post_recording_bat_file_path: Option<String>,
    pub is_event_relay_follow_enabled: Option<bool>,
    pub is_exact_recording_enabled: Option<bool>,
    pub is_oneseg_separate_output_enabled: Option<bool>,
    pub is_sequential_recording_in_single_file_enabled: Option<bool>,
    pub forced_tuner_id: Option<u32>,
}

impl RecordSettingsPatch {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RecordSettings {
    pub is_enabled: bool,
    pub priority: u8,
    pub recording_folders: Vec<RecordingFolder>,
    pub recording_start_margin: Option<i32>,
    pub recording_end_margin: Option<i32>,
    pub recording_mode: RecordingMode,
    pub caption_recording_mode: ServiceRecordingMode,
    pub data_broadcasting_recording_mode: ServiceRecordingMode,
    pub post_recording_mode: PostRecordingMode,
    pub post_recording_bat_file_path: Option<String>,
    pub is_event_relay_follow_enabled: bool,
    pub is_exact_recording_enabled: bool,
    pub is_oneseg_separate_output_enabled: bool,
    pub is_sequential_recording_in_single_file_enabled: bool,
    pub forced_tuner_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RecordSettingsGlobalDefaults {
    pub recording_start_margin: i32,
    pub recording_end_margin: i32,
    pub caption_recording_mode: ServiceRecordingMode,
    pub data_broadcasting_recording_mode: ServiceRecordingMode,
    pub post_recording_mode: PostRecordingMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RecordSettingsPreset {
    pub id: i32,
    pub name: String,
    pub record_settings: RecordSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RecordSettingsPresets {
    pub global_defaults: RecordSettingsGlobalDefaults,
    pub presets: Vec<RecordSettingsPreset>,
}

fn normalize_option(value: &str) -> String {
    value
        .chars()
        .filter(|value| *value != '-' && *value != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    RecName = 1,
    Write = 2,
}

impl FromStr for PluginKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "write" => Ok(Self::Write),
            "recname" => Ok(Self::RecName),
            _ => Err(format!("plugin kind must be write or rec_name: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReserveData {
    pub title: String,
    pub start_time: DateTime<FixedOffset>,
    pub duration_second: u32,
    pub station_name: String,
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub eid: u16,
    pub comment: String,
    pub reserve_id: i32,
    pub overlap_mode: u8,
    pub start_time_epg: DateTime<FixedOffset>,
    pub rec_setting: RecSettingData,
    pub rec_file_name_list: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecFileInfo {
    pub id: i32,
    pub rec_file_path: String,
    pub title: String,
    pub start_time: DateTime<FixedOffset>,
    pub duration_sec: u32,
    pub service_name: String,
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub eid: u16,
    pub drops: i64,
    pub scrambles: i64,
    pub rec_status: i32,
    pub start_time_epg: DateTime<FixedOffset>,
    pub comment: String,
    pub program_info: String,
    pub err_info: String,
    pub protect_flag: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TunerReserveInfo {
    pub tuner_id: u32,
    pub tuner_name: String,
    pub reserve_list: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TunerProcessStatusInfo {
    pub tuner_id: u32,
    pub process_id: i32,
    pub drop: i64,
    pub scramble: i64,
    pub signal_lv: f32,
    pub space: i32,
    pub ch: i32,
    pub onid: i32,
    pub tsid: i32,
    pub rec_flag: bool,
    pub epg_cap_flag: bool,
    pub extra_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShortEventInfo {
    pub event_name: String,
    pub text_char: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtendedEventInfo {
    pub text_char: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContentData {
    pub content_nibble: u16,
    pub user_nibble: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContentInfo {
    pub nibble_list: Vec<ContentData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentInfo {
    pub stream_content: u8,
    pub component_type: u8,
    pub component_tag: u8,
    pub text_char: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioComponentInfoData {
    pub stream_content: u8,
    pub component_type: u8,
    pub component_tag: u8,
    pub stream_type: u8,
    pub simulcast_group_tag: u8,
    pub es_multi_lingual_flag: u8,
    pub main_component_flag: u8,
    pub quality_indicator: u8,
    pub sampling_rate: u8,
    pub text_char: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioComponentInfo {
    pub component_list: Vec<AudioComponentInfoData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventData {
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub eid: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventGroupInfo {
    pub group_type: u8,
    pub event_data_list: Vec<EventData>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventInfo {
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub eid: u16,
    pub free_ca_flag: u8,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub duration_sec: Option<i32>,
    pub short_info: Option<ShortEventInfo>,
    pub ext_info: Option<ExtendedEventInfo>,
    pub content_info: Option<ContentInfo>,
    pub component_info: Option<ComponentInfo>,
    pub audio_info: Option<AudioComponentInfo>,
    pub event_group_info: Option<EventGroupInfo>,
    pub event_relay_info: Option<EventGroupInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceEventInfo {
    pub service_info: ServiceInfo,
    pub event_list: Vec<EventInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct TimeTableQuery {
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
    pub channel_type: Option<ChannelType>,
    pub services: Vec<ServiceKey>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeTable {
    pub channels: Vec<TimeTableChannel>,
    pub date_range: TimeTableDateRange,
    pub reservation_metadata_status: BestEffortStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum BestEffortStatus {
    Ok,
    Unavailable { message: String },
}

impl BestEffortStatus {
    pub fn unavailable(error: impl ToString) -> Self {
        Self::Unavailable {
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeTableDateRange {
    pub earliest: DateTime<FixedOffset>,
    pub latest: DateTime<FixedOffset>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeTableChannel {
    pub service: ServiceInfo,
    pub programs: Vec<TimeTableProgram>,
    pub subchannels: Option<Vec<TimeTableSubchannel>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeTableSubchannel {
    pub service: ServiceInfo,
    pub programs: Vec<TimeTableProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Channel {
    pub id: String,
    pub display_channel_id: String,
    pub service_key: ServiceKey,
    pub network_id: u16,
    pub transport_stream_id: u16,
    pub service_id: u16,
    pub remocon_id: u16,
    pub channel_number: String,
    pub channel_type: ChannelType,
    pub name: String,
    pub is_subchannel: bool,
    pub is_radiochannel: bool,
    pub is_watchable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelList {
    pub channels: Vec<Channel>,
    pub epg_service_status: BestEffortStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeTableProgram {
    pub event: EventInfo,
    pub reservation: Option<TimeTableProgramReservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimeTableProgramReservation {
    pub id: i32,
    pub status: ReservationStatus,
    pub recording_availability: RecordingAvailability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum ReservationStatus {
    Reserved,
    Recording,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum RecordingAvailability {
    Full,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SearchDateInfo {
    pub start_day_of_week: u8,
    pub start_hour: u16,
    pub start_min: u16,
    pub end_day_of_week: u8,
    pub end_hour: u16,
    pub end_min: u16,
}

impl SearchDateInfo {
    pub fn validate(&self) -> Result<(), String> {
        if self.start_day_of_week > 6 || self.end_day_of_week > 6 {
            return Err("program search date range day_of_week must be in 0..=6".to_string());
        }
        if self.start_hour > 23 || self.end_hour > 23 {
            return Err("program search date range hour must be in 0..=23".to_string());
        }
        if self.start_min > 59 || self.end_min > 59 {
            return Err("program search date range minute must be in 0..=59".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SearchKeyInfo {
    pub and_key: String,
    pub not_key: String,
    pub key_disabled: bool,
    pub case_sensitive: bool,
    pub reg_exp_flag: bool,
    pub title_only_flag: bool,
    pub content_list: Vec<ContentData>,
    pub date_list: Vec<SearchDateInfo>,
    pub service_list: Vec<i64>,
    pub video_list: Vec<u16>,
    pub audio_list: Vec<u16>,
    pub aimai_flag: bool,
    pub not_contet_flag: bool,
    pub not_date_flag: bool,
    pub free_ca_flag: u8,
    pub chk_rec_end: bool,
    pub chk_rec_day: u16,
    pub chk_rec_no_service: bool,
    pub chk_duration_min: u16,
    pub chk_duration_max: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum BroadcastType {
    #[default]
    All,
    FreeOnly,
    PaidOnly,
}

impl FromStr for BroadcastType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "all" => Ok(Self::All),
            "free" | "freeonly" => Ok(Self::FreeOnly),
            "paid" | "paidonly" => Ok(Self::PaidOnly),
            _ => Err(format!(
                "broadcast type must be all, free, free-only, paid, or paid-only: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramGenreRange {
    pub major: u8,
    pub middle: u8,
    pub user_nibble: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum DuplicateTitleCheckScope {
    #[default]
    None,
    SameChannelOnly,
    AllChannels,
}

impl FromStr for DuplicateTitleCheckScope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_option(value).as_str() {
            "none" => Ok(Self::None),
            "samechannel" | "samechannelonly" => Ok(Self::SameChannelOnly),
            "allchannels" => Ok(Self::AllChannels),
            _ => Err(format!(
                "duplicate title check must be none, same-channel, or all-channels: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramSearchQuery {
    pub is_enabled: bool,
    pub keyword: String,
    pub exclude_keyword: String,
    pub title_only: bool,
    pub case_sensitive: bool,
    pub regex: bool,
    pub fuzzy: bool,
    pub service_ranges: Option<Vec<ServiceKey>>,
    pub genre_ranges: Vec<ProgramGenreRange>,
    pub exclude_genre_ranges: bool,
    pub date_ranges: Vec<SearchDateInfo>,
    pub exclude_date_ranges: bool,
    pub duration_min: Option<u16>,
    pub duration_max: Option<u16>,
    pub broadcast_type: BroadcastType,
    pub duplicate_title_check_scope: DuplicateTitleCheckScope,
    pub duplicate_title_check_period_days: u16,
}

impl Default for ProgramSearchQuery {
    fn default() -> Self {
        Self {
            is_enabled: true,
            keyword: String::new(),
            exclude_keyword: String::new(),
            title_only: false,
            case_sensitive: false,
            regex: false,
            fuzzy: false,
            service_ranges: None,
            genre_ranges: Vec::new(),
            exclude_genre_ranges: false,
            date_ranges: Vec::new(),
            exclude_date_ranges: false,
            duration_min: None,
            duration_max: None,
            broadcast_type: BroadcastType::All,
            duplicate_title_check_scope: DuplicateTitleCheckScope::None,
            duplicate_title_check_period_days: 6,
        }
    }
}

impl ProgramSearchQuery {
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(min), Some(max)) = (self.duration_min, self.duration_max)
            && min > max
        {
            return Err(
                "program search duration_min must be less than or equal to duration_max"
                    .to_string(),
            );
        }
        for value in [self.duration_min, self.duration_max].into_iter().flatten() {
            if value > 9999 {
                return Err(format!(
                    "program search duration must be in 0..=9999 minutes: {value}"
                ));
            }
        }
        for date in &self.date_ranges {
            date.validate()?;
        }
        if self.duplicate_title_check_period_days > 9999 {
            return Err(format!(
                "duplicate_title_check_period_days must be in 0..=9999: {}",
                self.duplicate_title_check_period_days
            ));
        }
        Ok(())
    }
}

impl TimeTableQuery {
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(start), Some(end)) = (self.start_time, self.end_time)
            && end <= start
        {
            return Err("timetable end_time must be later than start_time".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AutoAddData {
    pub data_id: i32,
    pub search_info: SearchKeyInfo,
    pub rec_setting: RecSettingData,
    pub add_count: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReservationCondition {
    pub id: i32,
    pub reservation_count: i32,
    pub program_search_condition: ProgramSearchQuery,
    pub record_settings: RecSettingData,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManualAutoAddData {
    pub data_id: i32,
    pub day_of_week_flag: u8,
    pub start_time: u32,
    pub duration_second: u32,
    pub title: String,
    pub station_name: String,
    pub onid: u16,
    pub tsid: u16,
    pub sid: u16,
    pub rec_setting: RecSettingData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NwPlayTimeShiftInfo {
    pub ctrl_id: i32,
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotifySrvInfo {
    pub notify_id: u32,
    pub time: DateTime<FixedOffset>,
    pub param1: u32,
    pub param2: u32,
    pub count: u32,
    pub param4: String,
    pub param5: String,
    pub param6: String,
}
