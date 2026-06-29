use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Timelike};
use edcb_tools::{
    BroadcastType, ConnectionConfig, DuplicateTitleCheckScope, EdcbClient, EventKey,
    PostRecordingMode, ProgramGenreRange, ProgramSearchQuery, RecordSettingsPatch,
    RecordingAvailability, RecordingFolder, RecordingMode, SearchDateInfo, SearchKeyInfo,
    ServiceKey, ServiceRecordingMode, TimeTableQuery,
    flows::{
        apply_record_settings_patch, build_reservation_from_event, create_reservation_condition,
        create_reservation_with_options, delete_reservation, delete_reservation_condition,
        get_timetable, preview_reservation, program_search_query_from_search_key,
        program_search_query_to_search_key, search_programs, update_reservation,
        update_reservation_condition,
    },
    test_support::{
        encode_auto_add_list_for_test, encode_event_list_for_test, encode_file_list_for_test,
        encode_reserve_for_test, encode_reserve_list_for_test, encode_search_keys_for_test,
        encode_service_event_list_for_test, encode_service_event_lists_for_test,
        encode_services_for_test, read_request_frame_for_test, reserve_fixture_for_test,
        service_event_fixture_for_test,
    },
    types::{AutoAddData, FileData, RecFileSetInfo, RecSettingData, ServiceInfo},
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

fn test_client(addr: SocketAddr) -> EdcbClient {
    EdcbClient::new(
        ConnectionConfig::new(addr.ip().to_string(), addr.port())
            .with_timeout(Duration::from_secs(1)),
    )
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
fn record_settings_from_rec_setting_decodes_full_settings() {
    let rec_setting = RecSettingData {
        rec_mode: 6,
        priority: 4,
        tuijyuu_flag: false,
        service_mode: 0x0000_0001 | 0x0000_0010 | 0x0000_0020,
        pittari_flag: true,
        bat_file_path: "after.bat".to_string(),
        rec_folder_list: vec![RecFileSetInfo {
            rec_folder: "/recorded".to_string(),
            write_plug_in: "Write_Default.dll".to_string(),
            rec_name_plug_in: "RecName_Macro.dll?$title$.ts".to_string(),
        }],
        suspend_mode: 2,
        reboot_flag: true,
        start_margin: Some(15),
        end_margin: Some(25),
        continue_rec_flag: true,
        partial_rec_flag: 1,
        tuner_id: 9,
        partial_rec_folder: vec![RecFileSetInfo {
            rec_folder: "/oneseg".to_string(),
            write_plug_in: "Write_Default.dll".to_string(),
            rec_name_plug_in: "RecName_Macro.dll".to_string(),
        }],
    };

    let settings = edcb_tools::flows::record_settings_from_rec_setting(&rec_setting)
        .expect("valid EDCB recording settings should decode");

    assert!(!settings.is_enabled);
    assert_eq!(settings.priority, 4);
    assert_eq!(
        settings.recording_mode,
        RecordingMode::AllServicesWithoutDecoding
    );
    assert_eq!(settings.recording_start_margin, Some(15));
    assert_eq!(settings.recording_end_margin, Some(25));
    assert_eq!(
        settings.caption_recording_mode,
        ServiceRecordingMode::Enable
    );
    assert_eq!(
        settings.data_broadcasting_recording_mode,
        ServiceRecordingMode::Enable
    );
    assert_eq!(
        settings.post_recording_mode,
        PostRecordingMode::SuspendAndReboot
    );
    assert_eq!(
        settings.post_recording_bat_file_path.as_deref(),
        Some("after.bat")
    );
    assert!(!settings.is_event_relay_follow_enabled);
    assert!(settings.is_exact_recording_enabled);
    assert!(settings.is_oneseg_separate_output_enabled);
    assert!(settings.is_sequential_recording_in_single_file_enabled);
    assert_eq!(settings.forced_tuner_id, Some(9));
    assert_eq!(settings.recording_folders.len(), 2);
    assert_eq!(
        settings.recording_folders[0]
            .recording_file_name_template
            .as_deref(),
        Some("$title$.ts")
    );
    assert!(settings.recording_folders[1].is_oneseg_separate_recording_folder);
    assert_eq!(
        settings.recording_folders[1].recording_file_name_template,
        None
    );
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
    let client = test_client(addr);

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
        genre_ranges: vec![
            ProgramGenreRange {
                major: 0,
                middle: 1,
                user_nibble: None,
            },
            ProgramGenreRange {
                major: 14,
                middle: 0,
                user_nibble: Some(0x1234),
            },
        ],
        exclude_genre_ranges: true,
        is_enabled: false,
        duplicate_title_check_scope: DuplicateTitleCheckScope::AllChannels,
        duplicate_title_check_period_days: 6,
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
    assert_eq!(key.content_list.len(), 2);
    assert_eq!(key.content_list[0].content_nibble, 0x0001);
    assert_eq!(key.content_list[0].user_nibble, 0);
    assert_eq!(key.content_list[1].content_nibble, 0x0e00);
    assert_eq!(key.content_list[1].user_nibble, 0x1234);
    assert!(key.not_contet_flag);
    assert!(key.key_disabled);
    assert!(key.chk_rec_end);
    assert!(key.chk_rec_no_service);
    assert_eq!(key.chk_rec_day, 6);
}

#[test]
fn search_key_info_maps_back_to_program_search_query() {
    let key = SearchKeyInfo {
        and_key: "Program".to_string(),
        not_key: "Sports".to_string(),
        key_disabled: true,
        content_list: vec![edcb_tools::types::ContentData {
            content_nibble: 0x0e00,
            user_nibble: 0x1234,
        }],
        service_list: vec![
            ServiceKey {
                onid: 1,
                tsid: 2,
                sid: 3,
            }
            .to_search_id(),
        ],
        not_contet_flag: true,
        chk_rec_end: true,
        chk_rec_day: 6,
        chk_rec_no_service: false,
        ..SearchKeyInfo::default()
    };

    let query = program_search_query_from_search_key(&key)
        .expect("SearchKeyInfo should map back to a program search query");

    assert_eq!(query.keyword, "Program");
    assert_eq!(query.exclude_keyword, "Sports");
    assert!(!query.is_enabled);
    assert_eq!(
        query.service_ranges,
        Some(vec![ServiceKey {
            onid: 1,
            tsid: 2,
            sid: 3,
        }])
    );
    assert_eq!(
        query.genre_ranges,
        vec![ProgramGenreRange {
            major: 14,
            middle: 0,
            user_nibble: Some(0x1234),
        }]
    );
    assert!(query.exclude_genre_ranges);
    assert_eq!(
        query.duplicate_title_check_scope,
        DuplicateTitleCheckScope::SameChannelOnly
    );
    assert_eq!(query.duplicate_title_check_period_days, 6);
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
    let client = test_client(addr);

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
async fn timetable_groups_programs_and_attaches_reservations() {
    let (service, event) = service_event_fixture_for_test();
    let service_key = ServiceKey {
        onid: service.onid,
        tsid: service.tsid,
        sid: service.sid,
    };
    let mut reserve = reserve_fixture_for_test();
    reserve.reserve_id = 77;
    reserve.onid = event.onid;
    reserve.tsid = event.tsid;
    reserve.sid = event.sid;
    reserve.eid = event.eid;
    reserve.start_time = event.start_time.expect("test event should have start time");
    reserve.duration_second =
        u32::try_from(event.duration_sec.expect("test event should have duration"))
            .expect("test event duration should be non-negative");
    reserve.overlap_mode = 1;
    let (addr, server) = spawn_command_sequence_server(vec![
        (
            1021,
            encode_services_for_test(std::slice::from_ref(&service)),
        ),
        (
            1029,
            encode_service_event_lists_for_test(&[(service.clone(), vec![event.clone()])]),
        ),
        (2011, encode_reserve_list_for_test(&[reserve])),
    ])
    .await;
    let client = test_client(addr);

    let timetable = get_timetable(
        &client,
        &TimeTableQuery {
            services: vec![service_key],
            ..TimeTableQuery::default()
        },
    )
    .await
    .expect("timetable should be built from EDCB EPG and reservations");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");
    let enum_pg_payload = &payloads[1];

    assert_eq!(read_i64_at(enum_pg_payload, 8), 0);
    assert_eq!(read_i64_at(enum_pg_payload, 16), service_key.to_search_id());
    assert_eq!(timetable.channels.len(), 1);
    assert_eq!(timetable.channels[0].service.sid, service.sid);
    assert_eq!(timetable.channels[0].programs.len(), 1);
    assert_eq!(timetable.channels[0].programs[0].event.eid, event.eid);
    let reservation = timetable.channels[0].programs[0]
        .reservation
        .as_ref()
        .expect("matching reservation should attach");
    assert_eq!(reservation.id, 77);
    assert_eq!(
        reservation.recording_availability,
        RecordingAvailability::Partial
    );
    assert_eq!(
        timetable.date_range.earliest,
        event.start_time.expect("test event should have start time")
    );
}

#[tokio::test]
async fn timetable_groups_short_subchannels_under_main_channel() {
    let (mut main_service, mut main_event) = service_event_fixture_for_test();
    main_service.sid = 3;
    main_service.service_name = "Main".to_string();
    main_event.sid = main_service.sid;
    let mut sub_service = main_service.clone();
    sub_service.sid = 4;
    sub_service.service_name = "Sub".to_string();
    let mut sub_event = main_event.clone();
    sub_event.sid = sub_service.sid;
    sub_event.eid = 5;
    sub_event.start_time = Some(
        main_event
            .start_time
            .expect("test event should have start time")
            + ChronoDuration::hours(1),
    );
    let (addr, server) = spawn_command_sequence_server(vec![
        (
            1021,
            encode_services_for_test(&[main_service.clone(), sub_service.clone()]),
        ),
        (
            1029,
            encode_service_event_lists_for_test(&[
                (main_service.clone(), vec![main_event.clone()]),
                (sub_service.clone(), vec![sub_event.clone()]),
            ]),
        ),
        (2011, encode_reserve_list_for_test(&[])),
    ])
    .await;
    let client = test_client(addr);

    let timetable = get_timetable(&client, &TimeTableQuery::default())
        .await
        .expect("timetable should group subchannels");
    server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(timetable.channels.len(), 1);
    assert_eq!(timetable.channels[0].service.sid, main_service.sid);
    assert_eq!(timetable.channels[0].programs.len(), 1);
    let subchannels = timetable.channels[0]
        .subchannels
        .as_ref()
        .expect("short subchannel should be nested");
    assert_eq!(subchannels.len(), 1);
    assert_eq!(subchannels[0].service.sid, sub_service.sid);
    assert_eq!(subchannels[0].programs[0].event.eid, sub_event.eid);
}

#[tokio::test]
async fn timetable_attaches_reservation_by_time_overlap_when_event_id_differs() {
    let (service, event) = service_event_fixture_for_test();
    let mut reserve = reserve_fixture_for_test();
    reserve.reserve_id = 78;
    reserve.onid = event.onid;
    reserve.tsid = event.tsid;
    reserve.sid = event.sid;
    reserve.eid = event.eid + 1;
    reserve.start_time =
        event.start_time.expect("test event should have start time") + ChronoDuration::minutes(10);
    reserve.duration_second = 600;

    let (addr, server) = spawn_command_sequence_server(vec![
        (
            1021,
            encode_services_for_test(std::slice::from_ref(&service)),
        ),
        (
            1029,
            encode_service_event_lists_for_test(&[(service.clone(), vec![event.clone()])]),
        ),
        (2011, encode_reserve_list_for_test(&[reserve])),
    ])
    .await;
    let client = test_client(addr);

    let timetable = get_timetable(&client, &TimeTableQuery::default())
        .await
        .expect("timetable should attach overlapping reservation metadata");
    server
        .await
        .expect("mock EDCB server task should complete without panicking");

    let reservation = timetable.channels[0].programs[0]
        .reservation
        .as_ref()
        .expect("overlapping reservation should attach even when event id differs");
    assert_eq!(reservation.id, 78);
}

#[tokio::test]
async fn timetable_keeps_long_subchannels_as_independent_channels() {
    let (mut main_service, mut main_event) = service_event_fixture_for_test();
    main_service.sid = 3;
    main_service.service_name = "Main".to_string();
    main_event.sid = main_service.sid;
    let mut sub_service = main_service.clone();
    sub_service.sid = 4;
    sub_service.service_name = "Long Sub".to_string();
    let mut sub_event = main_event.clone();
    sub_event.sid = sub_service.sid;
    sub_event.eid = 6;
    sub_event.duration_sec = Some(8 * 60 * 60);
    let (addr, server) = spawn_command_sequence_server(vec![
        (
            1021,
            encode_services_for_test(&[main_service.clone(), sub_service.clone()]),
        ),
        (
            1029,
            encode_service_event_lists_for_test(&[
                (main_service.clone(), vec![main_event.clone()]),
                (sub_service.clone(), vec![sub_event.clone()]),
            ]),
        ),
        (2011, encode_reserve_list_for_test(&[])),
    ])
    .await;
    let client = test_client(addr);

    let timetable = get_timetable(&client, &TimeTableQuery::default())
        .await
        .expect("timetable should keep long subchannels independent");
    server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(timetable.channels.len(), 2);
    assert_eq!(timetable.channels[0].service.sid, main_service.sid);
    assert_eq!(timetable.channels[0].subchannels, None);
    assert_eq!(timetable.channels[1].service.sid, sub_service.sid);
    assert_eq!(timetable.channels[1].programs[0].event.eid, sub_event.eid);
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
    let client = test_client(addr);

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
async fn auto_add_mutations_send_expected_protocol_shapes() {
    let data = AutoAddData {
        data_id: 55,
        search_info: SearchKeyInfo {
            and_key: "auto".to_string(),
            chk_rec_end: true,
            chk_rec_day: 6,
            chk_rec_no_service: true,
            ..SearchKeyInfo::default()
        },
        rec_setting: reserve_fixture_for_test().rec_setting,
        add_count: 2,
    };
    let (addr, server) = spawn_command_sequence_server(vec![
        (2132, Vec::new()),
        (2134, Vec::new()),
        (1033, Vec::new()),
    ])
    .await;
    let client = test_client(addr);

    client
        .add_auto_add(&data)
        .await
        .expect("AutoAdd add should succeed");
    client
        .change_auto_add(&data)
        .await
        .expect("AutoAdd change should succeed");
    client
        .delete_auto_add(data.data_id)
        .await
        .expect("AutoAdd delete should succeed");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    for payload in [&payloads[0], &payloads[1]] {
        assert_eq!(&payload[0..2], &5_u16.to_le_bytes());
        assert_eq!(read_i32_at(payload, 6), 1);
        assert_eq!(read_i32_at(payload, 14), data.data_id);
        let (chk_rec_end, chk_rec_day) = read_search_key_recording_check(payload, 18);
        assert!(chk_rec_end);
        assert_eq!(chk_rec_day, 40006);
    }
    assert_eq!(read_i32_at(&payloads[2], 0), 12);
    assert_eq!(read_i32_at(&payloads[2], 4), 1);
    assert_eq!(read_i32_at(&payloads[2], 8), data.data_id);
}

#[tokio::test]
async fn create_reservation_condition_uses_default_record_settings_and_adds_auto_add() {
    let query = ProgramSearchQuery {
        keyword: "ニュース".to_string(),
        service_ranges: Some(vec![ServiceKey {
            onid: 1,
            tsid: 2,
            sid: 3,
        }]),
        genre_ranges: vec![ProgramGenreRange {
            major: 0,
            middle: 1,
            user_nibble: None,
        }],
        ..ProgramSearchQuery::default()
    };
    let options = RecordSettingsPatch {
        priority: Some(4),
        ..RecordSettingsPatch::default()
    };
    let (addr, server) = spawn_two_command_server(
        2012,
        encode_reserve_for_test(&reserve_fixture_for_test()),
        2132,
        Vec::new(),
    )
    .await;
    let client = test_client(addr);

    let condition = create_reservation_condition(&client, &query, &options)
        .await
        .expect("reservation condition create should add AutoAdd data");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(condition.id, 0);
    assert_eq!(condition.program_search_condition.keyword, "ニュース");
    assert_eq!(condition.record_settings.priority, 4);
    assert_eq!(&payloads[0][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[0][2..6], &0x7fff_ffff_i32.to_le_bytes());
    assert_eq!(&payloads[1][0..2], &5_u16.to_le_bytes());
    assert_eq!(read_i32_at(&payloads[1], 6), 1);
    assert_eq!(read_i32_at(&payloads[1], 14), 0);
}

#[tokio::test]
async fn update_reservation_condition_merges_existing_auto_add_and_returns_updated_condition() {
    let existing = AutoAddData {
        data_id: 77,
        search_info: program_search_query_to_search_key(&ProgramSearchQuery {
            keyword: "old".to_string(),
            ..ProgramSearchQuery::default()
        })
        .expect("test query should map to SearchKeyInfo"),
        rec_setting: reserve_fixture_for_test().rec_setting,
        add_count: 3,
    };
    let updated_query = ProgramSearchQuery {
        keyword: "new".to_string(),
        service_ranges: Some(vec![ServiceKey {
            onid: 1,
            tsid: 2,
            sid: 3,
        }]),
        ..ProgramSearchQuery::default()
    };
    let mut updated = existing.clone();
    updated.search_info =
        program_search_query_to_search_key(&updated_query).expect("test query should map");
    updated.rec_setting.priority = 5;

    let (addr, server) = spawn_command_sequence_server(vec![
        (
            2131,
            encode_auto_add_list_for_test(std::slice::from_ref(&existing)),
        ),
        (2134, Vec::new()),
        (
            2131,
            encode_auto_add_list_for_test(std::slice::from_ref(&updated)),
        ),
    ])
    .await;
    let client = test_client(addr);

    let condition = update_reservation_condition(
        &client,
        existing.data_id,
        Some(&updated_query),
        &RecordSettingsPatch {
            priority: Some(5),
            ..RecordSettingsPatch::default()
        },
    )
    .await
    .expect("reservation condition update should change AutoAdd data");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(condition.id, existing.data_id);
    assert_eq!(condition.program_search_condition.keyword, "new");
    assert_eq!(condition.record_settings.priority, 5);
    assert_eq!(&payloads[1][0..2], &5_u16.to_le_bytes());
    assert_eq!(read_i32_at(&payloads[1], 14), existing.data_id);
}

#[tokio::test]
async fn delete_reservation_condition_fetches_existing_auto_add_then_deletes_by_id() {
    let existing = AutoAddData {
        data_id: 77,
        search_info: program_search_query_to_search_key(&ProgramSearchQuery {
            keyword: "old".to_string(),
            ..ProgramSearchQuery::default()
        })
        .expect("test query should map to SearchKeyInfo"),
        rec_setting: reserve_fixture_for_test().rec_setting,
        add_count: 3,
    };
    let (addr, server) = spawn_two_command_server(
        2131,
        encode_auto_add_list_for_test(std::slice::from_ref(&existing)),
        1033,
        Vec::new(),
    )
    .await;
    let client = test_client(addr);

    let condition = delete_reservation_condition(&client, existing.data_id)
        .await
        .expect("reservation condition delete should return deleted condition");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(condition.id, existing.data_id);
    assert_eq!(condition.reservation_count, 3);
    assert_eq!(read_i32_at(&payloads[1], 0), 12);
    assert_eq!(read_i32_at(&payloads[1], 4), 1);
    assert_eq!(read_i32_at(&payloads[1], 8), existing.data_id);
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
    let client = test_client(addr);

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
    let client = test_client(addr);

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
async fn get_recording_defaults_fetches_default_reserve_settings() {
    let mut reserve = reserve_fixture_for_test();
    reserve.rec_setting.priority = 5;
    reserve.rec_setting.start_margin = Some(10);
    reserve.rec_setting.end_margin = Some(20);
    let (addr, server) = spawn_single_command_server(2012, encode_reserve_for_test(&reserve)).await;
    let client = test_client(addr);

    let settings = edcb_tools::flows::get_recording_defaults(&client)
        .await
        .expect("recording defaults should decode from EDCB default reserve");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(&payload[0..2], &5_u16.to_le_bytes());
    assert_eq!(&payload[2..6], &0x7fff_ffff_i32.to_le_bytes());
    assert_eq!(settings.priority, 5);
    assert_eq!(settings.recording_start_margin, Some(10));
    assert_eq!(settings.recording_end_margin, Some(20));
}

#[tokio::test]
async fn get_recording_presets_reads_epg_timer_srv_ini() {
    let ini = r#"
[SET]
StartMargin=10
EndMargin=20
Caption=0
Data=1
RecEndMode=1
Reboot=1
PresetID=1,bad,

[REC_DEF]
SetName=Default
RecMode=5
NoRecMode=1
Priority=3
TuijyuuFlag=0
ServiceMode=49
PittariFlag=1
BatFilePath=after.bat
SuspendMode=4
RebootFlag=0
UseMargineFlag=1
StartMargine=15
EndMargine=25
ContinueRec=1
PartialRec=1
TunerID=7

[REC_DEF_FOLDER]
Count=1
0=/recorded
RecNamePlugIn0=RecName_Macro.dll?$title$.ts

[REC_DEF_FOLDER_1SEG]
Count=1
0=/oneseg
RecNamePlugIn0=RecName_Macro.dll?$title$_1seg.ts

[REC_DEF1]
SetName=Custom
RecMode=2
Priority=5
"#;
    let files = [FileData {
        name: "EpgTimerSrv.ini".to_string(),
        data: ini.as_bytes().to_vec(),
    }];
    let (addr, server) = spawn_single_command_server(2060, encode_file_list_for_test(&files)).await;
    let client = test_client(addr);

    let presets = edcb_tools::flows::get_recording_presets(&client)
        .await
        .expect("recording presets should parse from EpgTimerSrv.ini");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(&payload[0..2], &5_u16.to_le_bytes());
    assert_eq!(presets.global_defaults.recording_start_margin, 10);
    assert_eq!(presets.global_defaults.recording_end_margin, 20);
    assert_eq!(
        presets.global_defaults.caption_recording_mode,
        ServiceRecordingMode::Disable
    );
    assert_eq!(
        presets.global_defaults.data_broadcasting_recording_mode,
        ServiceRecordingMode::Enable
    );
    assert_eq!(
        presets.global_defaults.post_recording_mode,
        PostRecordingMode::StandbyAndReboot
    );
    assert_eq!(presets.presets.len(), 2);
    assert_eq!(presets.presets[0].id, 0);
    assert_eq!(presets.presets[0].name, "Default");
    assert!(!presets.presets[0].record_settings.is_enabled);
    assert_eq!(presets.presets[0].record_settings.priority, 3);
    assert_eq!(
        presets.presets[0].record_settings.recording_mode,
        RecordingMode::SpecifiedService
    );
    assert_eq!(
        presets.presets[0]
            .record_settings
            .recording_folders
            .iter()
            .filter(|folder| folder.is_oneseg_separate_recording_folder)
            .count(),
        1
    );
    assert_eq!(presets.presets[1].id, 1);
    assert_eq!(presets.presets[1].name, "Custom");
    assert_eq!(presets.presets[1].record_settings.priority, 5);
}

#[tokio::test]
async fn get_recording_presets_empty_ini_error_mentions_defaults_fallback() {
    let files = [FileData {
        name: "EpgTimerSrv.ini".to_string(),
        data: Vec::new(),
    }];
    let (addr, server) = spawn_single_command_server(2060, encode_file_list_for_test(&files)).await;
    let client = test_client(addr);

    let error = edcb_tools::flows::get_recording_presets(&client)
        .await
        .expect_err("empty EpgTimerSrv.ini should be reported as a useful error");
    server
        .await
        .expect("mock EDCB server task should complete without panicking");

    let message = error.to_string();
    assert!(message.contains("EpgTimerSrv.ini is empty"));
    assert!(message.contains("recording defaults"));
}

#[tokio::test]
async fn list_channels_builds_snapshot_from_chset5_and_enum_service() {
    let chset5 = "\
Main TV\tNetwork\t32736\t32736\t1024\t1\t0\t1\t1\t0\n\
Sub TV\tNetwork\t32736\t32736\t1025\t1\t0\t1\t1\t0\n\
Radio\tNetwork\t4\t16625\t101\t2\t0\t1\t1\t0\n\
Data\tNetwork\t32736\t32736\t1400\t192\t0\t1\t1\t0\n";
    let services = [
        ServiceInfo {
            onid: 32736,
            tsid: 32736,
            sid: 1024,
            service_type: 1,
            partial_reception_flag: 0,
            service_provider_name: "Provider".to_string(),
            service_name: "Main TV".to_string(),
            network_name: "Network".to_string(),
            ts_name: "TS".to_string(),
            remote_control_key_id: 1,
        },
        ServiceInfo {
            onid: 32736,
            tsid: 32736,
            sid: 1025,
            service_type: 1,
            partial_reception_flag: 0,
            service_provider_name: "Provider".to_string(),
            service_name: "Sub TV".to_string(),
            network_name: "Network".to_string(),
            ts_name: "TS".to_string(),
            remote_control_key_id: 1,
        },
    ];
    let (addr, server) = spawn_command_sequence_server(vec![
        (1060, chset5.as_bytes().to_vec()),
        (1021, encode_services_for_test(&services)),
    ])
    .await;
    let client = test_client(addr);

    let channels = edcb_tools::flows::list_channels(&client)
        .await
        .expect("channels should build from current EDCB service data");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");
    let chset_name_bytes: Vec<_> = "ChSet5.txt"
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    assert!(
        payloads[0]
            .windows(chset_name_bytes.len())
            .any(|window| window == chset_name_bytes),
        "first request should be FileCopy for ChSet5.txt"
    );
    assert_eq!(channels.len(), 3);
    assert_eq!(channels[0].id, "NID32736-SID1024");
    assert_eq!(channels[0].display_channel_id, "gr011");
    assert_eq!(channels[0].channel_number, "011");
    assert_eq!(channels[0].remocon_id, 1);
    assert_eq!(channels[0].service_key.sid, 1024);
    assert!(!channels[0].is_subchannel);
    assert_eq!(channels[1].display_channel_id, "gr012");
    assert!(channels[1].is_subchannel);
    assert_eq!(channels[2].display_channel_id, "bs101");
    assert!(channels[2].is_radiochannel);
}

#[tokio::test]
async fn add_reserve_sends_versioned_reserve_vector() {
    let (addr, server) = spawn_single_command_server(2013, 5_u16.to_le_bytes().to_vec()).await;
    let client = test_client(addr);
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
        (2011, encode_reserve_list_for_test(&[])),
        (2013, 5_u16.to_le_bytes().to_vec()),
        (2011, encode_reserve_list_for_test(&[])),
    ])
    .await;
    let client = test_client(addr);

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
    assert_eq!(&payloads[3][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[3][6..10], &1_i32.to_le_bytes());
}

#[tokio::test]
async fn create_reservation_returns_resolved_reserve_id_after_add() {
    let (service, event) = service_event_fixture_for_test();
    let event_key = EventKey {
        service: ServiceKey {
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
        },
        eid: event.eid,
    };
    let existing = reserve_fixture_for_test();
    let mut created = build_reservation_from_event(&existing, &service, &event)
        .expect("test event should build reservation");
    created.reserve_id = 519;
    let (addr, server) = spawn_command_sequence_server(vec![
        (1029, encode_service_event_list_for_test(&service, &event)),
        (2012, encode_reserve_for_test(&existing)),
        (
            2011,
            encode_reserve_list_for_test(std::slice::from_ref(&existing)),
        ),
        (2013, 5_u16.to_le_bytes().to_vec()),
        (
            2011,
            encode_reserve_list_for_test(&[existing.clone(), created.clone()]),
        ),
    ])
    .await;
    let client = test_client(addr);

    let reserve = create_reservation_with_options(
        &client,
        event_key,
        &RecordSettingsPatch {
            priority: Some(5),
            ..RecordSettingsPatch::default()
        },
    )
    .await
    .expect("reservation creation should return the resolved created reserve");
    let payloads = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(reserve.reserve_id, 519);
    assert_eq!(&payloads[2][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[3][0..2], &5_u16.to_le_bytes());
    assert_eq!(&payloads[4][0..2], &5_u16.to_le_bytes());
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
    let client = test_client(addr);

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
    let client = test_client(addr);

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

fn read_u16_at(payload: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        payload[offset..offset + 2]
            .try_into()
            .expect("test payload should contain a full u16 field"),
    )
}

fn read_search_key_recording_check(payload: &[u8], search_key_offset: usize) -> (bool, u16) {
    let mut offset = search_key_offset + 4;
    offset = string_end_at(payload, offset);
    offset = string_end_at(payload, offset);
    offset += 8;
    for _ in 0..5 {
        offset = vector_end_at(payload, offset);
    }
    offset += 4;
    (payload[offset] != 0, read_u16_at(payload, offset + 1))
}

fn string_end_at(payload: &[u8], offset: usize) -> usize {
    let size = usize::try_from(read_i32_at(payload, offset))
        .expect("test string size should be non-negative");
    offset + size
}

fn vector_end_at(payload: &[u8], offset: usize) -> usize {
    let size = usize::try_from(read_i32_at(payload, offset))
        .expect("test vector size should be non-negative");
    offset + size
}
