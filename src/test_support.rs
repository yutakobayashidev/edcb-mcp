use chrono::TimeZone;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::codec::{
    Writer, jst, write_auto_add_data, write_reserve_data, write_search_key_info, write_service_info,
};
use crate::types::{
    AutoAddData, EventInfo, FileData, RecSettingData, ReserveData, SearchKeyInfo, ServiceInfo,
    ShortEventInfo,
};

#[doc(hidden)]
pub fn encode_service_list_for_test() -> Vec<u8> {
    let service = ServiceInfo {
        onid: 1,
        tsid: 2,
        sid: 3,
        service_type: 1,
        partial_reception_flag: 0,
        service_provider_name: "Provider".to_string(),
        service_name: "Test Service".to_string(),
        network_name: "Network".to_string(),
        ts_name: "TS".to_string(),
        remote_control_key_id: 7,
    };

    let mut writer = Writer::new();
    writer.write_vector(&[service], write_service_info);
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_services_for_test(services: &[ServiceInfo]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_vector(services, write_service_info);
    writer.into_inner()
}

#[doc(hidden)]
pub fn reserve_fixture_for_test() -> ReserveData {
    ReserveData {
        title: "Default Reserve".to_string(),
        start_time: jst()
            .with_ymd_and_hms(2026, 6, 29, 9, 0, 0)
            .single()
            .expect("test reservation start time should be valid"),
        duration_second: 1800,
        station_name: "Default Station".to_string(),
        onid: 10,
        tsid: 20,
        sid: 30,
        eid: 40,
        comment: "default comment".to_string(),
        reserve_id: 123,
        overlap_mode: 1,
        start_time_epg: jst()
            .with_ymd_and_hms(2026, 6, 29, 9, 0, 0)
            .single()
            .expect("test reservation EPG start time should be valid"),
        rec_setting: RecSettingData {
            rec_mode: 1,
            priority: 3,
            tuijyuu_flag: true,
            service_mode: 0,
            pittari_flag: false,
            bat_file_path: String::new(),
            rec_folder_list: Vec::new(),
            suspend_mode: 0,
            reboot_flag: false,
            start_margin: None,
            end_margin: None,
            continue_rec_flag: false,
            partial_rec_flag: 0,
            tuner_id: 0,
            partial_rec_folder: Vec::new(),
        },
        rec_file_name_list: vec!["existing.ts".to_string()],
    }
}

#[doc(hidden)]
pub fn encode_reserve_for_test(reserve: &ReserveData) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_u16(5);
    write_reserve_data(&mut writer, reserve);
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_reserve_list_for_test(reserves: &[ReserveData]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_u16(5);
    writer.write_vector(reserves, write_reserve_data);
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_auto_add_list_for_test(items: &[AutoAddData]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_u16(5);
    writer.write_vector(items, write_auto_add_data);
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_file_list_for_test(files: &[FileData]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_u16(5);
    writer.write_vector(files, |writer, file| {
        writer.write_struct(|writer| {
            writer.write_string(&file.name);
            writer.write_i32(
                i32::try_from(file.data.len())
                    .expect("test file data length should fit in an EDCB frame"),
            );
            writer.write_i32(0);
            writer.write_bytes(&file.data);
        });
    });
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_service_event_list_for_test(service: &ServiceInfo, event: &EventInfo) -> Vec<u8> {
    encode_service_event_lists_for_test(&[(service.clone(), vec![event.clone()])])
}

#[doc(hidden)]
pub fn encode_service_event_lists_for_test(
    service_events: &[(ServiceInfo, Vec<EventInfo>)],
) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_vector(service_events, |writer, (service, events)| {
        writer.write_struct(|writer| {
            write_service_info(writer, service);
            writer.write_vector(events, |writer, event| {
                write_event_info_for_test(writer, event)
            });
        });
    });
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_event_list_for_test(event: &EventInfo) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_vector(std::slice::from_ref(event), |writer, event| {
        write_event_info_for_test(writer, event)
    });
    writer.into_inner()
}

#[doc(hidden)]
pub fn encode_search_keys_for_test(keys: &[SearchKeyInfo]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_vector(keys, write_search_key_info);
    writer.into_inner()
}

fn write_event_info_for_test(writer: &mut Writer, value: &EventInfo) {
    writer.write_struct(|writer| {
        writer.write_u16(value.onid);
        writer.write_u16(value.tsid);
        writer.write_u16(value.sid);
        writer.write_u16(value.eid);
        writer.write_u8(1);
        writer.write_system_time(value.start_time.expect("test event should have start time"));
        writer.write_u8(1);
        writer.write_i32(value.duration_sec.expect("test event should have duration"));
        let short_info = value
            .short_info
            .as_ref()
            .expect("test event should have short info");
        writer.write_struct(|writer| {
            writer.write_string(&short_info.event_name);
            writer.write_string(&short_info.text_char);
        });
        writer.write_i32(4);
        writer.write_i32(4);
        writer.write_i32(4);
        writer.write_i32(4);
        writer.write_i32(4);
        writer.write_i32(4);
        writer.write_u8(value.free_ca_flag);
    });
}

#[doc(hidden)]
pub fn service_event_fixture_for_test() -> (ServiceInfo, EventInfo) {
    let service = ServiceInfo {
        onid: 1,
        tsid: 2,
        sid: 3,
        service_type: 1,
        partial_reception_flag: 0,
        service_provider_name: "Provider".to_string(),
        service_name: "Test Service".to_string(),
        network_name: "Network".to_string(),
        ts_name: "TS".to_string(),
        remote_control_key_id: 7,
    };
    let event = EventInfo {
        onid: 1,
        tsid: 2,
        sid: 3,
        eid: 4,
        free_ca_flag: 0,
        start_time: Some(
            jst()
                .with_ymd_and_hms(2026, 6, 29, 10, 0, 0)
                .single()
                .expect("test event start time should be valid"),
        ),
        duration_sec: Some(3600),
        short_info: Some(ShortEventInfo {
            event_name: "Test Program".to_string(),
            text_char: "Description".to_string(),
        }),
        ext_info: None,
        content_info: None,
        component_info: None,
        audio_info: None,
        event_group_info: None,
        event_relay_info: None,
    };
    (service, event)
}

#[doc(hidden)]
pub async fn read_request_frame_for_test<R>(reader: &mut R) -> (i32, Vec<u8>)
where
    R: AsyncRead + Unpin,
{
    let mut header = [0_u8; 8];
    reader
        .read_exact(&mut header)
        .await
        .expect("mock EDCB server should read request header");
    let command = i32::from_le_bytes(
        header[0..4]
            .try_into()
            .expect("request command field is exactly four bytes"),
    );
    let payload_len = i32::from_le_bytes(
        header[4..8]
            .try_into()
            .expect("request payload length field is exactly four bytes"),
    );
    let payload_len =
        usize::try_from(payload_len).expect("request payload length must be non-negative");
    let mut payload = vec![0_u8; payload_len];
    reader
        .read_exact(&mut payload)
        .await
        .expect("mock EDCB server should read request payload");
    (command, payload)
}
