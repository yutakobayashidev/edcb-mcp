use crate::client::EdcbClient;
use crate::error::{EdcbError, Result};
use crate::types::{
    BroadcastType, EventInfo, EventKey, PostRecordingMode, ProgramSearchQuery, RecFileSetInfo,
    RecSettingData, RecordSettingsPatch, RecordingFolder, RecordingMode, ReserveData,
    SearchDateInfo, SearchKeyInfo, ServiceInfo, ServiceKey, ServiceRecordingMode,
};

const EPG_SERVICE_ALL_MASK: i64 = 0x0000_ffff_ffff_ffff;
const EPG_LOOKUP_TIME_BEGIN: i64 = 1;
const EPG_LOOKUP_TIME_END: i64 = i64::MAX;

pub async fn search_programs(
    client: &EdcbClient,
    query: &ProgramSearchQuery,
) -> Result<Vec<EventInfo>> {
    let key = if query.service_ranges.is_none() {
        let mut query = query.clone();
        query.service_ranges = Some(default_search_services(client).await?);
        program_search_query_to_search_key(&query)?
    } else {
        program_search_query_to_search_key(query)?
    };
    client.search_pg(&[key]).await
}

pub fn program_search_query_to_search_key(query: &ProgramSearchQuery) -> Result<SearchKeyInfo> {
    validate_program_search_query(query)?;
    Ok(SearchKeyInfo {
        and_key: query.keyword.clone(),
        not_key: query.exclude_keyword.clone(),
        case_sensitive: query.case_sensitive,
        reg_exp_flag: query.regex,
        title_only_flag: query.title_only,
        date_list: query.date_ranges.clone(),
        service_list: query
            .service_ranges
            .as_ref()
            .into_iter()
            .flatten()
            .map(|service| service.to_search_id())
            .collect(),
        aimai_flag: query.fuzzy,
        not_date_flag: query.exclude_date_ranges,
        free_ca_flag: match query.broadcast_type {
            BroadcastType::All => 0,
            BroadcastType::FreeOnly => 1,
            BroadcastType::PaidOnly => 2,
        },
        chk_duration_min: query.duration_min.unwrap_or_default(),
        chk_duration_max: query.duration_max.unwrap_or_default(),
        ..SearchKeyInfo::default()
    })
}

fn validate_program_search_query(query: &ProgramSearchQuery) -> Result<()> {
    if let (Some(min), Some(max)) = (query.duration_min, query.duration_max)
        && min > max
    {
        return Err(EdcbError::InvalidInput(
            "program search duration_min must be less than or equal to duration_max".to_string(),
        ));
    }
    for value in [query.duration_min, query.duration_max]
        .into_iter()
        .flatten()
    {
        if value > 9999 {
            return Err(EdcbError::InvalidInput(format!(
                "program search duration must be in 0..=9999 minutes: {value}"
            )));
        }
    }
    for date in &query.date_ranges {
        validate_search_date(date)?;
    }
    Ok(())
}

fn validate_search_date(date: &SearchDateInfo) -> Result<()> {
    if date.start_day_of_week > 6 || date.end_day_of_week > 6 {
        return Err(EdcbError::InvalidInput(
            "program search date day_of_week must be in 0..=6".to_string(),
        ));
    }
    if date.start_hour > 23 || date.end_hour > 23 {
        return Err(EdcbError::InvalidInput(
            "program search date hour must be in 0..=23".to_string(),
        ));
    }
    if date.start_min > 59 || date.end_min > 59 {
        return Err(EdcbError::InvalidInput(
            "program search date minute must be in 0..=59".to_string(),
        ));
    }
    Ok(())
}

async fn default_search_services(client: &EdcbClient) -> Result<Vec<ServiceKey>> {
    Ok(client
        .enum_service()
        .await?
        .into_iter()
        .map(|service| ServiceKey {
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
        })
        .collect())
}

pub async fn get_reservation(client: &EdcbClient, reserve_id: i32) -> Result<ReserveData> {
    client.get_reserve(reserve_id).await
}

pub async fn delete_reservation(client: &EdcbClient, reserve_id: i32) -> Result<ReserveData> {
    let reserve = get_reservation(client, reserve_id).await?;
    client.delete_reserve(reserve_id).await?;
    Ok(reserve)
}

pub async fn preview_reservation(client: &EdcbClient, event_key: EventKey) -> Result<ReserveData> {
    let (service, event) = find_event(client, event_key).await?;
    let default = client.get_default_reserve().await?;
    build_reservation_from_event(&default, &service, &event)
}

pub async fn preview_reservation_with_options(
    client: &EdcbClient,
    event_key: EventKey,
    options: &RecordSettingsPatch,
) -> Result<ReserveData> {
    let mut reserve = preview_reservation(client, event_key).await?;
    apply_record_settings_patch(&mut reserve.rec_setting, options)?;
    Ok(reserve)
}

pub async fn create_reservation(client: &EdcbClient, event_key: EventKey) -> Result<ReserveData> {
    create_reservation_with_options(client, event_key, &RecordSettingsPatch::default()).await
}

pub async fn create_reservation_with_options(
    client: &EdcbClient,
    event_key: EventKey,
    options: &RecordSettingsPatch,
) -> Result<ReserveData> {
    let reserve = preview_reservation_with_options(client, event_key, options).await?;
    client.add_reserve(&reserve).await?;
    Ok(reserve)
}

pub async fn update_reservation(
    client: &EdcbClient,
    reserve_id: i32,
    options: &RecordSettingsPatch,
) -> Result<ReserveData> {
    if options.is_empty() {
        return Err(EdcbError::InvalidInput(
            "reservation update requires at least one option".to_string(),
        ));
    }
    let mut reserve = get_reservation(client, reserve_id).await?;
    apply_record_settings_patch(&mut reserve.rec_setting, options)?;
    client.change_reserve(&reserve).await?;
    get_reservation(client, reserve_id).await
}

pub fn build_reservation_from_event(
    default: &ReserveData,
    service: &ServiceInfo,
    event: &EventInfo,
) -> Result<ReserveData> {
    let start_time = event
        .start_time
        .ok_or_else(|| EdcbError::InvalidInput("event start_time is missing".to_string()))?;
    let duration_second = event
        .duration_sec
        .ok_or_else(|| EdcbError::InvalidInput("event duration_sec is missing".to_string()))?;
    let duration_second = u32::try_from(duration_second).map_err(|_| {
        EdcbError::InvalidInput(format!(
            "event duration_sec must be non-negative: {duration_second}"
        ))
    })?;
    let title = event
        .short_info
        .as_ref()
        .map(|info| info.event_name.trim())
        .filter(|title| !title.is_empty())
        .unwrap_or(default.title.as_str())
        .to_string();

    let mut reserve = default.clone();
    reserve.title = title;
    reserve.start_time = start_time;
    reserve.duration_second = duration_second;
    reserve.station_name = service.service_name.clone();
    reserve.onid = event.onid;
    reserve.tsid = event.tsid;
    reserve.sid = event.sid;
    reserve.eid = event.eid;
    reserve.comment.clear();
    reserve.reserve_id = 0;
    reserve.overlap_mode = 0;
    reserve.start_time_epg = start_time;
    reserve.rec_file_name_list.clear();
    Ok(reserve)
}

pub fn apply_record_settings_patch(
    rec_setting: &mut RecSettingData,
    patch: &RecordSettingsPatch,
) -> Result<()> {
    if let Some(priority) = patch.priority {
        if !(1..=5).contains(&priority) {
            return Err(EdcbError::InvalidInput(format!(
                "priority must be in 1..=5: {priority}"
            )));
        }
        rec_setting.priority = priority;
    }

    let enabled = patch.is_enabled.unwrap_or(rec_setting.rec_mode <= 4);
    let recording_mode = patch
        .recording_mode
        .unwrap_or(recording_mode_from_rec_mode(rec_setting.rec_mode)?);
    if patch.is_enabled.is_some() || patch.recording_mode.is_some() {
        rec_setting.rec_mode = rec_mode_value(enabled, recording_mode);
    }

    if patch.recording_start_margin.is_some() || patch.recording_end_margin.is_some() {
        match (patch.recording_start_margin, patch.recording_end_margin) {
            (Some(start), Some(end)) => {
                rec_setting.start_margin = Some(start);
                rec_setting.end_margin = Some(end);
            }
            _ => {
                return Err(EdcbError::InvalidInput(
                    "recording margins must include both start and end".to_string(),
                ));
            }
        }
    }

    if patch.caption_recording_mode.is_some() || patch.data_broadcasting_recording_mode.is_some() {
        let (current_caption, current_data) = service_recording_modes(rec_setting.service_mode);
        let caption = patch.caption_recording_mode.unwrap_or(current_caption);
        let data = patch
            .data_broadcasting_recording_mode
            .unwrap_or(current_data);
        rec_setting.service_mode = service_mode_value(caption, data)?;
    }

    if let Some(mode) = patch.post_recording_mode {
        let (suspend_mode, reboot_flag) = suspend_mode_value(mode);
        rec_setting.suspend_mode = suspend_mode;
        rec_setting.reboot_flag = reboot_flag;
    }
    if let Some(path) = &patch.post_recording_bat_file_path {
        rec_setting.bat_file_path.clone_from(path);
    }
    if let Some(folders) = &patch.recording_folders {
        let (rec_folders, partial_folders) = rec_file_set_lists(folders);
        rec_setting.rec_folder_list = rec_folders;
        rec_setting.partial_rec_folder = partial_folders;
    }
    if let Some(value) = patch.is_event_relay_follow_enabled {
        rec_setting.tuijyuu_flag = value;
    }
    if let Some(value) = patch.is_exact_recording_enabled {
        rec_setting.pittari_flag = value;
    }
    if let Some(value) = patch.is_oneseg_separate_output_enabled {
        rec_setting.partial_rec_flag = u8::from(value);
    }
    if let Some(value) = patch.is_sequential_recording_in_single_file_enabled {
        rec_setting.continue_rec_flag = value;
    }
    if let Some(value) = patch.forced_tuner_id {
        rec_setting.tuner_id = value;
    }
    Ok(())
}

async fn find_event(client: &EdcbClient, event_key: EventKey) -> Result<(ServiceInfo, EventInfo)> {
    let services = client
        .enum_pg_info_ex(&event_lookup_filter(Some(event_key.service)))
        .await?;
    for service in services {
        for event in service.event_list {
            if event.eid == event_key.eid
                && event.onid == event_key.service.onid
                && event.tsid == event_key.service.tsid
                && event.sid == event_key.service.sid
            {
                return Ok((service.service_info, event));
            }
        }
    }
    Err(EdcbError::InvalidInput(format!(
        "event not found: {}:{}:{}:{}",
        event_key.service.onid, event_key.service.tsid, event_key.service.sid, event_key.eid
    )))
}

fn recording_mode_from_rec_mode(rec_mode: u8) -> Result<RecordingMode> {
    match rec_mode {
        0 | 9 => Ok(RecordingMode::AllServices),
        1 | 5 => Ok(RecordingMode::SpecifiedService),
        2 | 6 => Ok(RecordingMode::AllServicesWithoutDecoding),
        3 | 7 => Ok(RecordingMode::SpecifiedServiceWithoutDecoding),
        4 | 8 => Ok(RecordingMode::View),
        _ => Err(EdcbError::InvalidInput(format!(
            "unsupported EDCB rec_mode: {rec_mode}"
        ))),
    }
}

fn rec_mode_value(enabled: bool, mode: RecordingMode) -> u8 {
    match (enabled, mode) {
        (true, RecordingMode::AllServices) => 0,
        (true, RecordingMode::SpecifiedService) => 1,
        (true, RecordingMode::AllServicesWithoutDecoding) => 2,
        (true, RecordingMode::SpecifiedServiceWithoutDecoding) => 3,
        (true, RecordingMode::View) => 4,
        (false, RecordingMode::AllServices) => 9,
        (false, RecordingMode::SpecifiedService) => 5,
        (false, RecordingMode::AllServicesWithoutDecoding) => 6,
        (false, RecordingMode::SpecifiedServiceWithoutDecoding) => 7,
        (false, RecordingMode::View) => 8,
    }
}

fn service_recording_modes(service_mode: u32) -> (ServiceRecordingMode, ServiceRecordingMode) {
    if service_mode & 0x0000_0001 == 0 {
        return (ServiceRecordingMode::Default, ServiceRecordingMode::Default);
    }
    let caption = if service_mode & 0x0000_0010 != 0 {
        ServiceRecordingMode::Enable
    } else {
        ServiceRecordingMode::Disable
    };
    let data = if service_mode & 0x0000_0020 != 0 {
        ServiceRecordingMode::Enable
    } else {
        ServiceRecordingMode::Disable
    };
    (caption, data)
}

fn service_mode_value(caption: ServiceRecordingMode, data: ServiceRecordingMode) -> Result<u32> {
    let caption_default = caption == ServiceRecordingMode::Default;
    let data_default = data == ServiceRecordingMode::Default;
    if caption_default != data_default {
        return Err(EdcbError::InvalidInput(
            "caption and data recording modes must both be Default or both be explicit".to_string(),
        ));
    }
    if caption_default {
        return Ok(0);
    }
    let mut service_mode = 0x0000_0001;
    if caption == ServiceRecordingMode::Enable {
        service_mode |= 0x0000_0010;
    }
    if data == ServiceRecordingMode::Enable {
        service_mode |= 0x0000_0020;
    }
    Ok(service_mode)
}

fn suspend_mode_value(mode: PostRecordingMode) -> (u8, bool) {
    match mode {
        PostRecordingMode::Default => (0, false),
        PostRecordingMode::Standby => (1, false),
        PostRecordingMode::StandbyAndReboot => (1, true),
        PostRecordingMode::Suspend => (2, false),
        PostRecordingMode::SuspendAndReboot => (2, true),
        PostRecordingMode::Shutdown => (3, false),
        PostRecordingMode::Nothing => (4, false),
    }
}

fn rec_file_set_lists(folders: &[RecordingFolder]) -> (Vec<RecFileSetInfo>, Vec<RecFileSetInfo>) {
    let mut normal = Vec::new();
    let mut partial = Vec::new();
    for folder in folders {
        let info = RecFileSetInfo {
            rec_folder: folder.recording_folder_path.clone(),
            write_plug_in: "Write_Default.dll".to_string(),
            rec_name_plug_in: folder
                .recording_file_name_template
                .as_ref()
                .filter(|template| !template.is_empty())
                .map(|template| format!("RecName_Macro.dll?{template}"))
                .unwrap_or_else(|| "RecName_Macro.dll".to_string()),
        };
        if folder.is_oneseg_separate_recording_folder {
            partial.push(info);
        } else {
            normal.push(info);
        }
    }
    (normal, partial)
}

fn event_lookup_filter(service: Option<ServiceKey>) -> [i64; 4] {
    let (mask, key) = service
        .map(|service| (0, service.to_search_id()))
        .unwrap_or((EPG_SERVICE_ALL_MASK, EPG_SERVICE_ALL_MASK));
    [mask, key, EPG_LOOKUP_TIME_BEGIN, EPG_LOOKUP_TIME_END]
}
