use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use chrono::Timelike;
use edcb_tools::{
    BroadcastType, EdcbClient, EventKey, PostRecordingMode, ProgramSearchQuery,
    RecordSettingsPatch, RecordingFolder, RecordingMode, SearchDateInfo, SearchKeyInfo, ServiceKey,
    ServiceRecordingMode,
    flows::{
        apply_record_settings_patch, build_reservation_from_event, create_reservation_with_options,
        delete_reservation, preview_reservation, program_search_query_to_search_key,
        search_programs, update_reservation,
    },
    test_support::{
        encode_event_list_for_test, encode_reserve_for_test, encode_search_keys_for_test,
        encode_service_event_list_for_test, read_request_frame_for_test, reserve_fixture_for_test,
        service_event_fixture_for_test,
    },
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

async fn spawn_single_command_server(
    expected_command: i32,
    response_body: Vec<u8>,
) -> (SocketAddr, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock EDCB server should bind to a local port");
    let addr = listener
        .local_addr()
        .expect("mock EDCB server should expose its local address");

    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("mock EDCB server should accept one client connection");
        let (command, payload) = read_request_frame_for_test(&mut socket).await;
        assert_eq!(command, expected_command);

        socket
            .write_i32_le(1)
            .await
            .expect("mock EDCB server should write response status");
        socket
            .write_i32_le(
                i32::try_from(response_body.len())
                    .expect("response body length should fit in an EDCB frame"),
            )
            .await
            .expect("mock EDCB server should write response length");
        socket
            .write_all(&response_body)
            .await
            .expect("mock EDCB server should write response body");

        payload
    });

    (addr, handle)
}

async fn spawn_two_command_server(
    first_command: i32,
    first_response_body: Vec<u8>,
    second_command: i32,
    second_response_body: Vec<u8>,
) -> (SocketAddr, JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock EDCB server should bind to a local port");
    let addr = listener
        .local_addr()
        .expect("mock EDCB server should expose its local address");

    let handle = tokio::spawn(async move {
        let mut payloads = Vec::new();
        for (expected_command, response_body) in [
            (first_command, first_response_body),
            (second_command, second_response_body),
        ] {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("mock EDCB server should accept a client connection");
            let (command, payload) = read_request_frame_for_test(&mut socket).await;
            assert_eq!(command, expected_command);
            socket
                .write_i32_le(1)
                .await
                .expect("mock EDCB server should write response status");
            socket
                .write_i32_le(
                    i32::try_from(response_body.len())
                        .expect("response body length should fit in an EDCB frame"),
                )
                .await
                .expect("mock EDCB server should write response length");
            socket
                .write_all(&response_body)
                .await
                .expect("mock EDCB server should write response body");
            payloads.push(payload);
        }
        payloads
    });

    (addr, handle)
}

async fn spawn_command_sequence_server(
    commands: Vec<(i32, Vec<u8>)>,
) -> (SocketAddr, JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock EDCB server should bind to a local port");
    let addr = listener
        .local_addr()
        .expect("mock EDCB server should expose its local address");

    let handle = tokio::spawn(async move {
        let mut payloads = Vec::new();
        for (expected_command, response_body) in commands {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("mock EDCB server should accept a client connection");
            let (command, payload) = read_request_frame_for_test(&mut socket).await;
            assert_eq!(command, expected_command);
            socket
                .write_i32_le(1)
                .await
                .expect("mock EDCB server should write response status");
            socket
                .write_i32_le(
                    i32::try_from(response_body.len())
                        .expect("response body length should fit in an EDCB frame"),
                )
                .await
                .expect("mock EDCB server should write response length");
            socket
                .write_all(&response_body)
                .await
                .expect("mock EDCB server should write response body");
            payloads.push(payload);
        }
        payloads
    });

    (addr, handle)
}

#[test]
fn parses_service_and_event_keys() {
    let service = ServiceKey::from_str("32736:32736:1024")
        .expect("service key should parse from onid:tsid:sid");
    assert_eq!(service.onid, 32736);
    assert_eq!(service.tsid, 32736);
    assert_eq!(service.sid, 1024);
    assert_eq!(service.to_search_id(), 140602194789376);

    let event =
        EventKey::from_str("32736:32736:1024:4208").expect("event key should parse with eid");
    assert_eq!(event.service, service);
    assert_eq!(event.eid, 4208);

    assert!(ServiceKey::from_str("32736:32736").is_err());
    assert!(EventKey::from_str("32736:32736:1024:nope").is_err());
}

#[test]
fn applies_record_settings_patch_to_edcb_rec_setting() {
    let mut rec_setting = reserve_fixture_for_test().rec_setting;
    let patch = RecordSettingsPatch {
        is_enabled: Some(false),
        priority: Some(4),
        recording_mode: Some(RecordingMode::SpecifiedServiceWithoutDecoding),
        recording_start_margin: Some(60),
        recording_end_margin: Some(120),
        caption_recording_mode: Some(ServiceRecordingMode::Enable),
        data_broadcasting_recording_mode: Some(ServiceRecordingMode::Disable),
        post_recording_mode: Some(PostRecordingMode::StandbyAndReboot),
        post_recording_bat_file_path: Some("after.bat".to_string()),
        recording_folders: Some(vec![RecordingFolder {
            recording_folder_path: "/recorded".to_string(),
            recording_file_name_template: Some("$title$".to_string()),
            is_oneseg_separate_recording_folder: false,
        }]),
        is_event_relay_follow_enabled: Some(false),
        is_exact_recording_enabled: Some(true),
        is_oneseg_separate_output_enabled: Some(true),
        is_sequential_recording_in_single_file_enabled: Some(true),
        forced_tuner_id: Some(7),
    };

    apply_record_settings_patch(&mut rec_setting, &patch)
        .expect("valid recording settings patch should apply");

    assert_eq!(rec_setting.rec_mode, 7);
    assert_eq!(rec_setting.priority, 4);
    assert!(!rec_setting.tuijyuu_flag);
    assert_eq!(rec_setting.service_mode, 0x0000_0001 | 0x0000_0010);
    assert!(rec_setting.pittari_flag);
    assert_eq!(rec_setting.bat_file_path, "after.bat");
    assert_eq!(rec_setting.rec_folder_list.len(), 1);
    assert_eq!(rec_setting.rec_folder_list[0].rec_folder, "/recorded");
    assert_eq!(
        rec_setting.rec_folder_list[0].rec_name_plug_in,
        "RecName_Macro.dll?$title$"
    );
    assert_eq!(rec_setting.suspend_mode, 1);
    assert!(rec_setting.reboot_flag);
    assert_eq!(rec_setting.start_margin, Some(60));
    assert_eq!(rec_setting.end_margin, Some(120));
    assert!(rec_setting.continue_rec_flag);
    assert_eq!(rec_setting.partial_rec_flag, 1);
    assert_eq!(rec_setting.tuner_id, 7);
}

#[test]
fn rejects_invalid_record_settings_patch_values() {
    let mut rec_setting = reserve_fixture_for_test().rec_setting;
    let error = apply_record_settings_patch(
        &mut rec_setting,
        &RecordSettingsPatch {
            priority: Some(6),
            ..RecordSettingsPatch::default()
        },
    )
    .expect_err("priority outside 1..=5 should be rejected");
    assert!(error.to_string().contains("priority"));

    let error = apply_record_settings_patch(
        &mut rec_setting,
        &RecordSettingsPatch {
            recording_start_margin: Some(30),
            ..RecordSettingsPatch::default()
        },
    )
    .expect_err("one-sided margins should be rejected");
    assert!(error.to_string().contains("margin"));

    let error = apply_record_settings_patch(
        &mut rec_setting,
        &RecordSettingsPatch {
            caption_recording_mode: Some(ServiceRecordingMode::Enable),
            ..RecordSettingsPatch::default()
        },
    )
    .expect_err("caption/data modes should be explicit together");
    assert!(error.to_string().contains("caption"));
}

#[tokio::test]
async fn search_pg_sends_search_key_info_and_decodes_events() {
    let (service, event) = service_event_fixture_for_test();
    let key = SearchKeyInfo {
        and_key: "Program".to_string(),
        not_key: "Sports".to_string(),
        title_only_flag: true,
        case_sensitive: true,
        reg_exp_flag: true,
        aimai_flag: true,
        service_list: vec![
            ServiceKey {
                onid: service.onid,
                tsid: service.tsid,
                sid: service.sid,
            }
            .to_search_id(),
        ],
        date_list: vec![SearchDateInfo {
            start_day_of_week: 1,
            start_hour: 19,
            start_min: 0,
            end_day_of_week: 1,
            end_hour: 23,
            end_min: 0,
        }],
        not_date_flag: true,
        free_ca_flag: 1,
        chk_duration_min: 30,
        chk_duration_max: 120,
        ..SearchKeyInfo::default()
    };
    let (addr, server) =
        spawn_single_command_server(1025, encode_event_list_for_test(&event)).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let programs = client
        .search_pg(std::slice::from_ref(&key))
        .await
        .expect("SearchPg should decode event list");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].eid, event.eid);
    assert_eq!(payload, encode_search_keys_for_test(&[key]));
}

#[test]
fn program_search_query_maps_to_search_key_info() {
    let service = ServiceKey {
        onid: 1,
        tsid: 2,
        sid: 3,
    };
    let query = ProgramSearchQuery {
        keyword: "Program".to_string(),
        exclude_keyword: "Sports".to_string(),
        title_only: true,
        case_sensitive: true,
        regex: true,
        fuzzy: true,
        service_ranges: Some(vec![service]),
        date_ranges: vec![SearchDateInfo {
            start_day_of_week: 1,
            start_hour: 19,
            start_min: 0,
            end_day_of_week: 1,
            end_hour: 23,
            end_min: 0,
        }],
        exclude_date_ranges: true,
        duration_min: Some(30),
        duration_max: Some(120),
        broadcast_type: BroadcastType::FreeOnly,
    };

    let key = program_search_query_to_search_key(&query)
        .expect("valid program search query should map to SearchKeyInfo");

    assert_eq!(key.and_key, "Program");
    assert_eq!(key.not_key, "Sports");
    assert!(key.title_only_flag);
    assert!(key.case_sensitive);
    assert!(key.reg_exp_flag);
    assert!(key.aimai_flag);
    assert_eq!(key.service_list, vec![service.to_search_id()]);
    assert_eq!(key.date_list, query.date_ranges);
    assert!(key.not_date_flag);
    assert_eq!(key.chk_duration_min, 30);
    assert_eq!(key.chk_duration_max, 120);
    assert_eq!(key.free_ca_flag, 1);
}

#[test]
fn program_search_query_rejects_invalid_duration_range() {
    let error = program_search_query_to_search_key(&ProgramSearchQuery {
        duration_min: Some(120),
        duration_max: Some(30),
        ..ProgramSearchQuery::default()
    })
    .expect_err("reversed duration range should fail");

    assert!(error.to_string().contains("duration_min"));
}

#[tokio::test]
async fn search_programs_uses_search_pg_for_specific_service() {
    let (service, event) = service_event_fixture_for_test();
    let service_key = ServiceKey {
        onid: service.onid,
        tsid: service.tsid,
        sid: service.sid,
    };
    let query = ProgramSearchQuery {
        keyword: "Program".to_string(),
        title_only: true,
        service_ranges: Some(vec![service_key]),
        ..ProgramSearchQuery::default()
    };
    let (addr, server) =
        spawn_single_command_server(1025, encode_event_list_for_test(&event)).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let programs = search_programs(&client, &query)
        .await
        .expect("program search should use SearchPg");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");
    let expected_key =
        program_search_query_to_search_key(&query).expect("test query should map to SearchKeyInfo");

    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].eid, event.eid);
    assert_eq!(payload, encode_search_keys_for_test(&[expected_key]));
}

#[tokio::test]
async fn search_programs_without_service_uses_enum_service_defaults() {
    let (service, event) = service_event_fixture_for_test();
    let (addr, server) = spawn_two_command_server(
        1021,
        edcb_tools::test_support::encode_service_list_for_test(),
        1025,
        encode_event_list_for_test(&event),
    )
    .await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let programs = search_programs(
        &client,
        &ProgramSearchQuery {
            keyword: "Program".to_string(),
            title_only: true,
            ..ProgramSearchQuery::default()
        },
    )
    .await
    .expect("program search should populate default service ranges");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");
    let expected_key = program_search_query_to_search_key(&ProgramSearchQuery {
        keyword: "Program".to_string(),
        title_only: true,
        service_ranges: Some(vec![ServiceKey {
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
        }]),
        ..ProgramSearchQuery::default()
    })
    .expect("test query should map to SearchKeyInfo");

    assert_eq!(programs.len(), 1);
    assert_eq!(payloads[1], encode_search_keys_for_test(&[expected_key]));
}

#[tokio::test]
async fn preview_reservation_looks_up_event_with_service_and_time_filter() {
    let (service, event) = service_event_fixture_for_test();
    let event_key = EventKey {
        service: ServiceKey {
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
        },
        eid: event.eid,
    };
    let (addr, server) = spawn_two_command_server(
        1029,
        encode_service_event_list_for_test(&service, &event),
        2012,
        encode_reserve_for_test(&reserve_fixture_for_test()),
    )
    .await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let reserve = preview_reservation(&client, event_key)
        .await
        .expect("reservation preview should build from EPG event and default settings");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");
    let enum_pg_payload = &payloads[0];

    assert_eq!(reserve.title, "Test Program");
    assert_eq!(read_i32_at(enum_pg_payload, 0), 40);
    assert_eq!(read_i32_at(enum_pg_payload, 4), 4);
    assert_eq!(read_i64_at(enum_pg_payload, 8), 0);
    assert_eq!(
        read_i64_at(enum_pg_payload, 16),
        event_key.service.to_search_id()
    );
    assert_eq!(read_i64_at(enum_pg_payload, 24), 1);
    assert_eq!(read_i64_at(enum_pg_payload, 32), i64::MAX);
    assert_eq!(&payloads[1][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[1][2..6], &0x7fff_ffff_i32.to_le_bytes());
}

#[test]
fn builds_reservation_from_default_settings_and_event() {
    let default = reserve_fixture_for_test();
    let (service, event) = service_event_fixture_for_test();

    let reserve = build_reservation_from_event(&default, &service, &event)
        .expect("event with time and duration should build a reservation");

    assert_eq!(reserve.title, "Test Program");
    assert_eq!(reserve.station_name, "Test Service");
    assert_eq!(reserve.onid, 1);
    assert_eq!(reserve.tsid, 2);
    assert_eq!(reserve.sid, 3);
    assert_eq!(reserve.eid, 4);
    assert_eq!(reserve.reserve_id, 0);
    assert_eq!(reserve.overlap_mode, 0);
    assert!(reserve.rec_file_name_list.is_empty());
    assert_eq!(reserve.rec_setting, default.rec_setting);
    assert_eq!(reserve.start_time.hour(), 10);
}

#[tokio::test]
async fn get_default_reserve_sends_get_reserve2_sentinel_id() {
    let response_body = encode_reserve_for_test(&reserve_fixture_for_test());
    let (addr, server) = spawn_single_command_server(2012, response_body).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let reserve = client
        .get_default_reserve()
        .await
        .expect("default reserve should decode");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(&payload[0..2], &5_u16.to_le_bytes());
    assert_eq!(&payload[2..6], &0x7fff_ffff_i32.to_le_bytes());
    assert_eq!(reserve.title, "Default Reserve");
}

#[tokio::test]
async fn add_reserve_sends_versioned_reserve_vector() {
    let (addr, server) = spawn_single_command_server(2013, 5_u16.to_le_bytes().to_vec()).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));
    let reserve = reserve_fixture_for_test();

    client
        .add_reserve(&reserve)
        .await
        .expect("add reserve should report command success");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(&payload[0..2], &5_u16.to_le_bytes());
    assert_eq!(&payload[6..10], &1_i32.to_le_bytes());
    let title_bytes: Vec<_> = "Default Reserve"
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    assert!(
        payload
            .windows(title_bytes.len())
            .any(|window| window == title_bytes)
    );
}

#[tokio::test]
async fn create_reservation_with_options_applies_recording_options() {
    let (service, event) = service_event_fixture_for_test();
    let event_key = EventKey {
        service: ServiceKey {
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
        },
        eid: event.eid,
    };
    let (addr, server) = spawn_command_sequence_server(vec![
        (1029, encode_service_event_list_for_test(&service, &event)),
        (2012, encode_reserve_for_test(&reserve_fixture_for_test())),
        (2013, 5_u16.to_le_bytes().to_vec()),
    ])
    .await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let reserve = create_reservation_with_options(
        &client,
        event_key,
        &RecordSettingsPatch {
            priority: Some(5),
            recording_start_margin: Some(10),
            recording_end_margin: Some(20),
            ..RecordSettingsPatch::default()
        },
    )
    .await
    .expect("reservation creation should apply recording options");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(reserve.rec_setting.priority, 5);
    assert_eq!(reserve.rec_setting.start_margin, Some(10));
    assert_eq!(reserve.rec_setting.end_margin, Some(20));
    assert_eq!(&payloads[2][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[2][6..10], &1_i32.to_le_bytes());
}

#[tokio::test]
async fn update_reservation_changes_existing_record_settings() {
    let mut existing = reserve_fixture_for_test();
    existing.reserve_id = 518;
    let mut updated = existing.clone();
    updated.rec_setting.priority = 5;
    let (addr, server) = spawn_command_sequence_server(vec![
        (2012, encode_reserve_for_test(&existing)),
        (2015, 5_u16.to_le_bytes().to_vec()),
        (2012, encode_reserve_for_test(&updated)),
    ])
    .await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let reserve = update_reservation(
        &client,
        existing.reserve_id,
        &RecordSettingsPatch {
            priority: Some(5),
            ..RecordSettingsPatch::default()
        },
    )
    .await
    .expect("reservation update should change existing record settings");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(reserve.rec_setting.priority, 5);
    assert_eq!(&payloads[0][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[0][2..6], &existing.reserve_id.to_le_bytes());
    assert_eq!(&payloads[1][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[1][6..10], &1_i32.to_le_bytes());
    assert_eq!(&payloads[2][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[2][2..6], &existing.reserve_id.to_le_bytes());
}

#[tokio::test]
async fn delete_reservation_fetches_existing_reserve_then_sends_delete() {
    let reserve = reserve_fixture_for_test();
    let (addr, server) =
        spawn_two_command_server(2012, encode_reserve_for_test(&reserve), 1014, Vec::new()).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let deleted = delete_reservation(&client, reserve.reserve_id)
        .await
        .expect("reservation delete should return the deleted reservation");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(deleted.reserve_id, reserve.reserve_id);
    assert_eq!(&payloads[0][0..2], &5_u16.to_le_bytes());
    assert_eq!(
        &payloads[0][2..6],
        &reserve.reserve_id.to_le_bytes(),
        "flow should fetch the existing reservation before deleting it"
    );
    assert_eq!(read_i32_at(&payloads[1], 0), 12);
    assert_eq!(read_i32_at(&payloads[1], 4), 1);
    assert_eq!(read_i32_at(&payloads[1], 8), reserve.reserve_id);
}

#[test]
fn reservation_builder_rejects_events_without_time() {
    let default = reserve_fixture_for_test();
    let (service, mut event) = service_event_fixture_for_test();
    event.start_time = None;

    let error = build_reservation_from_event(&default, &service, &event)
        .expect_err("event without start time should be rejected");

    assert!(error.to_string().contains("start_time"));
}

fn read_i32_at(payload: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        payload[offset..offset + 4]
            .try_into()
            .expect("test payload should contain a full i32 field"),
    )
}

fn read_i64_at(payload: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(
        payload[offset..offset + 8]
            .try_into()
            .expect("test payload should contain a full i64 field"),
    )
}
