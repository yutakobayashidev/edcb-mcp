use chrono::{DateTime, FixedOffset};
use serde::Serialize;

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
pub struct SearchDateInfo {
    pub start_day_of_week: u8,
    pub start_hour: u16,
    pub start_min: u16,
    pub end_day_of_week: u8,
    pub end_hour: u16,
    pub end_min: u16,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AutoAddData {
    pub data_id: i32,
    pub search_info: SearchKeyInfo,
    pub rec_setting: RecSettingData,
    pub add_count: i32,
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
