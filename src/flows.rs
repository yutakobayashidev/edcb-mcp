use crate::client::EdcbClient;
use crate::error::{EdcbError, Result};
use crate::types::{EventInfo, EventKey, ProgramSearchQuery, ReserveData, ServiceInfo, ServiceKey};

const EPG_SERVICE_ALL_MASK: i64 = 0x0000_ffff_ffff_ffff;
const EPG_LOOKUP_TIME_BEGIN: i64 = 1;
const EPG_LOOKUP_TIME_END: i64 = i64::MAX;

pub async fn search_programs(
    client: &EdcbClient,
    query: &ProgramSearchQuery,
) -> Result<Vec<EventInfo>> {
    let service_events = client
        .enum_pg_info_ex(&event_lookup_filter(query.service))
        .await?;
    Ok(service_events
        .into_iter()
        .flat_map(|service| service.event_list)
        .filter(|event| event_matches_query(event, query))
        .collect())
}

pub async fn preview_reservation(client: &EdcbClient, event_key: EventKey) -> Result<ReserveData> {
    let (service, event) = find_event(client, event_key).await?;
    let default = client.get_default_reserve().await?;
    build_reservation_from_event(&default, &service, &event)
}

pub async fn create_reservation(client: &EdcbClient, event_key: EventKey) -> Result<ReserveData> {
    let reserve = preview_reservation(client, event_key).await?;
    client.add_reserve(&reserve).await?;
    Ok(reserve)
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

fn event_matches_query(event: &EventInfo, query: &ProgramSearchQuery) -> bool {
    if query.keyword.is_empty() {
        return true;
    }
    let title_match = event
        .short_info
        .as_ref()
        .is_some_and(|info| info.event_name.contains(&query.keyword));
    let detail_match = event
        .short_info
        .as_ref()
        .is_some_and(|info| info.text_char.contains(&query.keyword))
        || event
            .ext_info
            .as_ref()
            .is_some_and(|info| info.text_char.contains(&query.keyword));
    title_match || (!query.title_only && detail_match)
}
