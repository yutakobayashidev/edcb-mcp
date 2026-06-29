use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, FixedOffset, Utc};

use crate::client::EdcbClient;
use crate::error::{EdcbError, Result};
use crate::recording::{
    rec_file_set_lists, rec_mode_value, record_settings_from_rec_setting as decode_record_settings,
    recording_mode_from_rec_mode, service_mode_value, service_recording_modes, suspend_mode_value,
};
use crate::types::{
    AutoAddData, BroadcastType, Channel, ChannelType, ContentData, DuplicateTitleCheckScope,
    EventInfo, EventKey, ProgramGenreRange, ProgramSearchQuery, RecSettingData, RecordSettings,
    RecordSettingsPatch, RecordSettingsPresets, RecordingAvailability, ReservationCondition,
    ReservationStatus, ReserveData, SearchDateInfo, SearchKeyInfo, ServiceInfo, ServiceKey,
    TimeTable, TimeTableChannel, TimeTableDateRange, TimeTableProgram, TimeTableProgramReservation,
    TimeTableQuery, TimeTableSubchannel,
};
use crate::util::{convert_bytes_to_string, datetime_to_file_time, parse_ch_set5};

const EPG_SERVICE_ALL_MASK: i64 = 0x0000_ffff_ffff_ffff;
const EPG_LOOKUP_TIME_BEGIN: i64 = 1;
const EPG_LOOKUP_TIME_END: i64 = i64::MAX;
const INDEPENDENT_SUBCHANNEL_SECONDS_PER_DAY: i64 = 8 * 60 * 60;

struct TimetablePrograms {
    by_service: HashMap<ServiceKey, Vec<TimeTableProgram>>,
    earliest: Option<DateTime<FixedOffset>>,
    latest: Option<DateTime<FixedOffset>>,
}

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
        key_disabled: !query.is_enabled,
        and_key: query.keyword.clone(),
        not_key: query.exclude_keyword.clone(),
        case_sensitive: query.case_sensitive,
        reg_exp_flag: query.regex,
        title_only_flag: query.title_only,
        content_list: query
            .genre_ranges
            .iter()
            .map(program_genre_to_content_data)
            .collect(),
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
        not_contet_flag: query.exclude_genre_ranges,
        chk_rec_end: query.duplicate_title_check_scope != DuplicateTitleCheckScope::None,
        chk_rec_day: query.duplicate_title_check_period_days,
        chk_rec_no_service: query.duplicate_title_check_scope
            == DuplicateTitleCheckScope::AllChannels,
        chk_duration_min: query.duration_min.unwrap_or_default(),
        chk_duration_max: query.duration_max.unwrap_or_default(),
        ..SearchKeyInfo::default()
    })
}

pub fn program_search_query_from_search_key(key: &SearchKeyInfo) -> Result<ProgramSearchQuery> {
    let service_ranges = key
        .service_list
        .iter()
        .map(|value| service_key_from_search_id(*value))
        .collect::<Result<Vec<_>>>()?;
    let genre_ranges = key
        .content_list
        .iter()
        .map(content_data_to_program_genre)
        .collect::<Result<Vec<_>>>()?;
    Ok(ProgramSearchQuery {
        is_enabled: !key.key_disabled,
        keyword: key.and_key.clone(),
        exclude_keyword: key.not_key.clone(),
        title_only: key.title_only_flag,
        case_sensitive: key.case_sensitive,
        regex: key.reg_exp_flag,
        fuzzy: key.aimai_flag,
        service_ranges: Some(service_ranges),
        genre_ranges,
        exclude_genre_ranges: key.not_contet_flag,
        date_ranges: key.date_list.clone(),
        exclude_date_ranges: key.not_date_flag,
        duration_min: (key.chk_duration_min > 0).then_some(key.chk_duration_min),
        duration_max: (key.chk_duration_max > 0).then_some(key.chk_duration_max),
        broadcast_type: match key.free_ca_flag {
            0 => BroadcastType::All,
            1 => BroadcastType::FreeOnly,
            2 => BroadcastType::PaidOnly,
            value => {
                return Err(EdcbError::InvalidInput(format!(
                    "unsupported EDCB free_ca_flag: {value}"
                )));
            }
        },
        duplicate_title_check_scope: if !key.chk_rec_end {
            DuplicateTitleCheckScope::None
        } else if key.chk_rec_no_service {
            DuplicateTitleCheckScope::AllChannels
        } else {
            DuplicateTitleCheckScope::SameChannelOnly
        },
        duplicate_title_check_period_days: key.chk_rec_day,
    })
}

fn program_genre_to_content_data(genre: &ProgramGenreRange) -> ContentData {
    ContentData {
        content_nibble: (u16::from(genre.major) << 8) | u16::from(genre.middle),
        user_nibble: genre.user_nibble.unwrap_or_default(),
    }
}

fn content_data_to_program_genre(content: &ContentData) -> Result<ProgramGenreRange> {
    Ok(ProgramGenreRange {
        major: u8::try_from((content.content_nibble >> 8) & 0x00ff)
            .expect("content major nibble is masked to u8"),
        middle: u8::try_from(content.content_nibble & 0x00ff)
            .expect("content middle nibble is masked to u8"),
        user_nibble: (content.user_nibble != 0).then_some(content.user_nibble),
    })
}

fn service_key_from_search_id(value: i64) -> Result<ServiceKey> {
    let value = u64::try_from(value).map_err(|_| {
        EdcbError::InvalidInput(format!(
            "EDCB service search id must be non-negative: {value}"
        ))
    })?;
    Ok(ServiceKey {
        onid: u16::try_from((value >> 32) & 0xffff).expect("masked ONID fits in u16"),
        tsid: u16::try_from((value >> 16) & 0xffff).expect("masked TSID fits in u16"),
        sid: u16::try_from(value & 0xffff).expect("masked SID fits in u16"),
    })
}

pub async fn get_timetable(client: &EdcbClient, query: &TimeTableQuery) -> Result<TimeTable> {
    validate_timetable_query(query)?;
    let services = resolve_timetable_services(client, query).await?;
    let service_time_list = timetable_lookup_filter(&services, query);
    let service_events = if service_time_list.is_empty() {
        Vec::new()
    } else {
        client.enum_pg_info_ex(&service_time_list).await?
    };
    let reserves = client.enum_reserve().await.unwrap_or_default();
    Ok(build_timetable(&services, service_events, &reserves, query))
}

fn validate_timetable_query(query: &TimeTableQuery) -> Result<()> {
    if let (Some(start), Some(end)) = (query.start_time, query.end_time)
        && end <= start
    {
        return Err(EdcbError::InvalidInput(
            "timetable end_time must be later than start_time".to_string(),
        ));
    }
    Ok(())
}

async fn resolve_timetable_services(
    client: &EdcbClient,
    query: &TimeTableQuery,
) -> Result<Vec<ServiceInfo>> {
    let mut services = client.enum_service().await?;
    if let Some(channel_type) = query.channel_type {
        services.retain(|service| service_channel_type(service) == Some(channel_type));
    }
    if query.services.is_empty() {
        services.sort_by(timetable_service_sort_key);
        return Ok(services);
    }

    let mut selected = Vec::new();
    for key in &query.services {
        if let Some(service) = services.iter().find(|service| service_key(service) == *key) {
            selected.push(service.clone());
        }
    }
    Ok(selected)
}

fn timetable_lookup_filter(services: &[ServiceInfo], query: &TimeTableQuery) -> Vec<i64> {
    let start = query
        .start_time
        .map(datetime_to_file_time)
        .unwrap_or(EPG_LOOKUP_TIME_BEGIN);
    let end = query
        .end_time
        .map(datetime_to_file_time)
        .unwrap_or(EPG_LOOKUP_TIME_END);
    services
        .iter()
        .flat_map(|service| {
            let key = service_key(service).to_search_id();
            [0, key, start, end]
        })
        .collect()
}

fn build_timetable(
    services: &[ServiceInfo],
    service_events: Vec<crate::types::ServiceEventInfo>,
    reserves: &[ReserveData],
    query: &TimeTableQuery,
) -> TimeTable {
    let now = Utc::now().fixed_offset();
    let events_by_service = events_by_service(service_events);
    let programs = build_timetable_programs(services, &events_by_service, reserves, query, now);

    TimeTable {
        channels: build_timetable_channels(services, &programs.by_service),
        date_range: timetable_date_range(query, programs.earliest, programs.latest),
    }
}

fn build_timetable_channels(
    services: &[ServiceInfo],
    programs_by_service: &HashMap<ServiceKey, Vec<TimeTableProgram>>,
) -> Vec<TimeTableChannel> {
    let parent_by_service = parent_services(services);
    let independent_subchannels = independent_subchannels(services, programs_by_service);
    let mut channels = Vec::new();

    for service in services {
        let key = service_key(service);
        let parent = parent_by_service.get(&key).copied().unwrap_or(key);
        if parent != key && !independent_subchannels.contains(&key) {
            continue;
        }

        let subchannels = services
            .iter()
            .filter(|candidate| {
                let candidate_key = service_key(candidate);
                parent_by_service.get(&candidate_key).copied() == Some(key)
                    && candidate_key != key
                    && !independent_subchannels.contains(&candidate_key)
            })
            .filter_map(|candidate| {
                let candidate_key = service_key(candidate);
                let programs = programs_by_service
                    .get(&candidate_key)
                    .cloned()
                    .unwrap_or_default();
                if programs.is_empty() {
                    None
                } else {
                    Some(TimeTableSubchannel {
                        service: candidate.clone(),
                        programs,
                    })
                }
            })
            .collect::<Vec<_>>();

        channels.push(TimeTableChannel {
            service: service.clone(),
            programs: programs_by_service.get(&key).cloned().unwrap_or_default(),
            subchannels: if subchannels.is_empty() {
                None
            } else {
                Some(subchannels)
            },
        });
    }
    channels
}

fn build_timetable_programs(
    services: &[ServiceInfo],
    events_by_service: &HashMap<ServiceKey, Vec<EventInfo>>,
    reserves: &[ReserveData],
    query: &TimeTableQuery,
    now: DateTime<FixedOffset>,
) -> TimetablePrograms {
    let mut by_service = HashMap::new();
    let mut earliest = None;
    let mut latest = None;

    for service in services {
        let key = service_key(service);
        let mut programs = events_by_service
            .get(&key)
            .into_iter()
            .flatten()
            .filter(|event| event_overlaps_query(event, query))
            .map(|event| {
                include_event_range(event, &mut earliest, &mut latest);
                TimeTableProgram {
                    event: event.clone(),
                    reservation: matching_reservation(event, reserves, now),
                }
            })
            .collect::<Vec<_>>();
        programs.sort_by_key(|program| program.event.start_time);
        by_service.insert(key, programs);
    }

    TimetablePrograms {
        by_service,
        earliest,
        latest,
    }
}

fn include_event_range(
    event: &EventInfo,
    earliest: &mut Option<DateTime<FixedOffset>>,
    latest: &mut Option<DateTime<FixedOffset>>,
) {
    if let Some(start) = event.start_time {
        *earliest = Some(earliest.map_or(start, |value| value.min(start)));
    }
    if let Some(end) = event_end_time(event) {
        *latest = Some(latest.map_or(end, |value| value.max(end)));
    }
}

fn timetable_date_range(
    query: &TimeTableQuery,
    earliest: Option<DateTime<FixedOffset>>,
    latest: Option<DateTime<FixedOffset>>,
) -> TimeTableDateRange {
    let fallback = query
        .start_time
        .or(query.end_time)
        .unwrap_or_else(|| Utc::now().fixed_offset());
    TimeTableDateRange {
        earliest: earliest.unwrap_or(fallback),
        latest: latest.or(query.end_time).unwrap_or(fallback),
    }
}

fn events_by_service(
    service_events: Vec<crate::types::ServiceEventInfo>,
) -> HashMap<ServiceKey, Vec<EventInfo>> {
    service_events
        .into_iter()
        .map(|item| (service_key(&item.service_info), item.event_list))
        .collect()
}

fn event_overlaps_query(event: &EventInfo, query: &TimeTableQuery) -> bool {
    let Some(start) = event.start_time else {
        return true;
    };
    let end = event_end_time(event).unwrap_or(start);
    if let Some(query_start) = query.start_time
        && end <= query_start
    {
        return false;
    }
    if let Some(query_end) = query.end_time
        && start >= query_end
    {
        return false;
    }
    true
}

fn matching_reservation(
    event: &EventInfo,
    reserves: &[ReserveData],
    now: DateTime<FixedOffset>,
) -> Option<TimeTableProgramReservation> {
    reserves
        .iter()
        .find(|reserve| {
            reserve.onid == event.onid
                && reserve.tsid == event.tsid
                && reserve.sid == event.sid
                && reserve.eid == event.eid
        })
        .or_else(|| {
            reserves
                .iter()
                .find(|reserve| reservation_overlaps_event(reserve, event))
        })
        .map(|reserve| reservation_metadata(reserve, now))
}

fn reservation_overlaps_event(reserve: &ReserveData, event: &EventInfo) -> bool {
    if reserve.onid != event.onid || reserve.tsid != event.tsid || reserve.sid != event.sid {
        return false;
    }
    let Some(event_start) = event.start_time else {
        return false;
    };
    let event_end = event_end_time(event).unwrap_or(event_start);
    let reserve_end =
        reserve.start_time + ChronoDuration::seconds(i64::from(reserve.duration_second));
    reserve.start_time < event_end && reserve_end > event_start
}

fn reservation_metadata(
    reserve: &ReserveData,
    now: DateTime<FixedOffset>,
) -> TimeTableProgramReservation {
    let reserve_end =
        reserve.start_time + ChronoDuration::seconds(i64::from(reserve.duration_second));
    let status = if reserve.rec_setting.rec_mode >= 5 {
        ReservationStatus::Disabled
    } else if reserve.start_time <= now && reserve_end > now {
        ReservationStatus::Recording
    } else {
        ReservationStatus::Reserved
    };
    TimeTableProgramReservation {
        id: reserve.reserve_id,
        status,
        recording_availability: match reserve.overlap_mode {
            1 => RecordingAvailability::Partial,
            2 => RecordingAvailability::Unavailable,
            _ => RecordingAvailability::Full,
        },
    }
}

fn independent_subchannels(
    services: &[ServiceInfo],
    programs_by_service: &HashMap<ServiceKey, Vec<TimeTableProgram>>,
) -> HashSet<ServiceKey> {
    let parent_by_service = parent_services(services);
    let mut seconds_by_day: BTreeMap<(ServiceKey, i32, u32), i64> = BTreeMap::new();

    for service in services {
        let key = service_key(service);
        if parent_by_service.get(&key).copied().unwrap_or(key) == key {
            continue;
        }
        for program in programs_by_service.get(&key).into_iter().flatten() {
            let Some(start) = program.event.start_time else {
                continue;
            };
            let broadcast_day = start - ChronoDuration::hours(4);
            let duration = i64::from(program.event.duration_sec.unwrap_or_default().max(0));
            *seconds_by_day
                .entry((key, broadcast_day.year(), broadcast_day.ordinal()))
                .or_default() += duration;
        }
    }

    seconds_by_day
        .into_iter()
        .filter_map(|((key, _, _), seconds)| {
            (seconds >= INDEPENDENT_SUBCHANNEL_SECONDS_PER_DAY).then_some(key)
        })
        .collect()
}

fn parent_services(services: &[ServiceInfo]) -> HashMap<ServiceKey, ServiceKey> {
    let mut sorted = services.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| timetable_service_sort_key(left, right));

    let mut parents = HashMap::new();
    let mut parent_by_stream = HashMap::new();
    for service in sorted
        .iter()
        .filter(|service| service.service_type != 192)
        .chain(sorted.iter().filter(|service| service.service_type == 192))
    {
        parent_by_stream
            .entry((service.onid, service.tsid))
            .or_insert_with(|| service_key(service));
    }
    for service in services {
        let parent = parent_by_stream
            .get(&(service.onid, service.tsid))
            .copied()
            .unwrap_or_else(|| service_key(service));
        parents.insert(service_key(service), parent);
    }
    parents
}

fn service_channel_type(service: &ServiceInfo) -> Option<ChannelType> {
    match service.onid {
        4 => Some(ChannelType::Bs),
        6 | 7 => Some(ChannelType::Cs),
        10 => Some(ChannelType::Sky),
        11 => Some(ChannelType::Bs4k),
        0x7000..=0x7fff => Some(ChannelType::Gr),
        _ => Some(ChannelType::Catv),
    }
}

fn timetable_service_sort_key(left: &ServiceInfo, right: &ServiceInfo) -> std::cmp::Ordering {
    (
        left.remote_control_key_id,
        left.onid,
        left.tsid,
        left.sid,
        left.service_name.as_str(),
    )
        .cmp(&(
            right.remote_control_key_id,
            right.onid,
            right.tsid,
            right.sid,
            right.service_name.as_str(),
        ))
}

fn service_key(service: &ServiceInfo) -> ServiceKey {
    ServiceKey {
        onid: service.onid,
        tsid: service.tsid,
        sid: service.sid,
    }
}

fn event_end_time(event: &EventInfo) -> Option<DateTime<FixedOffset>> {
    let start = event.start_time?;
    let duration = i64::from(event.duration_sec.unwrap_or_default().max(0));
    Some(start + ChronoDuration::seconds(duration))
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
    if query.duplicate_title_check_period_days > 9999 {
        return Err(EdcbError::InvalidInput(format!(
            "duplicate_title_check_period_days must be in 0..=9999: {}",
            query.duplicate_title_check_period_days
        )));
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

pub fn record_settings_from_rec_setting(rec_setting: &RecSettingData) -> Result<RecordSettings> {
    decode_record_settings(rec_setting)
}

pub async fn get_recording_defaults(client: &EdcbClient) -> Result<RecordSettings> {
    let reserve = client.get_default_reserve().await?;
    record_settings_from_rec_setting(&reserve.rec_setting)
}

pub async fn get_recording_presets(client: &EdcbClient) -> Result<RecordSettingsPresets> {
    let files = client.file_copy2(&["EpgTimerSrv.ini".to_string()]).await?;
    let file = files
        .into_iter()
        .find(|file| file.name.eq_ignore_ascii_case("EpgTimerSrv.ini"))
        .ok_or_else(|| EdcbError::InvalidInput("EpgTimerSrv.ini was not returned".to_string()))?;
    if file.data.is_empty() {
        return Err(EdcbError::InvalidInput(
            "EpgTimerSrv.ini is empty".to_string(),
        ));
    }
    let ini = convert_bytes_to_string(&file.data, "cp932");
    crate::recording::parse_recording_presets_ini(&ini)
}

pub async fn list_channels(client: &EdcbClient) -> Result<Vec<Channel>> {
    let chset = match client.file_copy("ChSet5.txt").await {
        Ok(data) if !data.is_empty() => parse_ch_set5(&convert_bytes_to_string(&data, "cp932")),
        _ => crate::channels::chset_from_services(client.enum_service().await?),
    };
    let epg_services = client.enum_service().await.unwrap_or_default();
    Ok(crate::channels::channels_from_sources(chset, &epg_services))
}

pub async fn list_reservation_conditions(client: &EdcbClient) -> Result<Vec<ReservationCondition>> {
    client
        .enum_auto_add()
        .await?
        .iter()
        .map(auto_add_data_to_reservation_condition)
        .collect()
}

pub async fn get_reservation_condition(
    client: &EdcbClient,
    condition_id: i32,
) -> Result<ReservationCondition> {
    let data = get_auto_add_data(client, condition_id).await?;
    auto_add_data_to_reservation_condition(&data)
}

pub async fn create_reservation_condition(
    client: &EdcbClient,
    query: &ProgramSearchQuery,
    options: &RecordSettingsPatch,
) -> Result<ReservationCondition> {
    let default = client.get_default_reserve().await?;
    let mut rec_setting = default.rec_setting;
    apply_record_settings_patch(&mut rec_setting, options)?;
    let data = AutoAddData {
        data_id: 0,
        search_info: reservation_condition_search_key(client, query).await?,
        rec_setting,
        add_count: 0,
    };
    client.add_auto_add(&data).await?;
    auto_add_data_to_reservation_condition(&data)
}

pub async fn update_reservation_condition(
    client: &EdcbClient,
    condition_id: i32,
    query: Option<&ProgramSearchQuery>,
    options: &RecordSettingsPatch,
) -> Result<ReservationCondition> {
    if query.is_none() && options.is_empty() {
        return Err(EdcbError::InvalidInput(
            "reservation condition update requires a search condition or at least one recording option"
                .to_string(),
        ));
    }

    let mut data = get_auto_add_data(client, condition_id).await?;
    if let Some(query) = query {
        data.search_info = reservation_condition_search_key(client, query).await?;
    }
    apply_record_settings_patch(&mut data.rec_setting, options)?;
    client.change_auto_add(&data).await?;
    get_reservation_condition(client, condition_id).await
}

pub async fn delete_reservation_condition(
    client: &EdcbClient,
    condition_id: i32,
) -> Result<ReservationCondition> {
    let data = get_auto_add_data(client, condition_id).await?;
    let condition = auto_add_data_to_reservation_condition(&data)?;
    client.delete_auto_add(condition_id).await?;
    Ok(condition)
}

pub fn auto_add_data_to_reservation_condition(data: &AutoAddData) -> Result<ReservationCondition> {
    Ok(ReservationCondition {
        id: data.data_id,
        reservation_count: data.add_count,
        program_search_condition: program_search_query_from_search_key(&data.search_info)?,
        record_settings: data.rec_setting.clone(),
    })
}

async fn get_auto_add_data(client: &EdcbClient, condition_id: i32) -> Result<AutoAddData> {
    client
        .enum_auto_add()
        .await?
        .into_iter()
        .find(|data| data.data_id == condition_id)
        .ok_or_else(|| {
            EdcbError::InvalidInput(format!("reservation condition not found: {condition_id}"))
        })
}

async fn reservation_condition_search_key(
    client: &EdcbClient,
    query: &ProgramSearchQuery,
) -> Result<SearchKeyInfo> {
    if query.service_ranges.is_some() {
        return program_search_query_to_search_key(query);
    }
    let mut query = query.clone();
    query.service_ranges = Some(default_search_services(client).await?);
    program_search_query_to_search_key(&query)
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

fn event_lookup_filter(service: Option<ServiceKey>) -> [i64; 4] {
    let (mask, key) = service
        .map(|service| (0, service.to_search_id()))
        .unwrap_or((EPG_SERVICE_ALL_MASK, EPG_SERVICE_ALL_MASK));
    [mask, key, EPG_LOOKUP_TIME_BEGIN, EPG_LOOKUP_TIME_END]
}
