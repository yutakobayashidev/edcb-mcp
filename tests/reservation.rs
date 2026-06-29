use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use chrono::Timelike;
use edcb_mcp::{
    EdcbClient, EventKey, ProgramSearchQuery, ServiceKey,
    flows::{build_reservation_from_event, preview_reservation, search_programs},
    test_support::{
        encode_reserve_for_test, encode_service_event_list_for_test, read_request_frame_for_test,
        reserve_fixture_for_test, service_event_fixture_for_test,
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

#[tokio::test]
async fn search_programs_filters_enum_pg_info_ex_results() {
    let (service, event) = service_event_fixture_for_test();
    let query = ProgramSearchQuery {
        keyword: "Program".to_string(),
        title_only: true,
        service: Some(ServiceKey {
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
        }),
    };
    let (addr, server) =
        spawn_single_command_server(1029, encode_service_event_list_for_test(&service, &event))
            .await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let programs = search_programs(&client, &query)
        .await
        .expect("program search should filter EPG events");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].eid, event.eid);
    assert_eq!(read_i32_at(&payload, 0), 40);
    assert_eq!(read_i32_at(&payload, 4), 4);
    assert_eq!(read_i64_at(&payload, 8), 0);
    assert_eq!(
        read_i64_at(&payload, 16),
        query
            .service
            .expect("test query has service")
            .to_search_id()
    );
    assert_eq!(read_i64_at(&payload, 24), 1);
    assert_eq!(read_i64_at(&payload, 32), i64::MAX);
}

#[tokio::test]
async fn search_programs_without_service_uses_all_service_filter() {
    let (service, event) = service_event_fixture_for_test();
    let (addr, server) =
        spawn_single_command_server(1029, encode_service_event_list_for_test(&service, &event))
            .await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let programs = search_programs(
        &client,
        &ProgramSearchQuery {
            keyword: "Program".to_string(),
            title_only: true,
            service: None,
        },
    )
    .await
    .expect("program search should support all-service search");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(programs.len(), 1);
    assert_eq!(read_i32_at(&payload, 0), 40);
    assert_eq!(read_i32_at(&payload, 4), 4);
    assert_eq!(read_i64_at(&payload, 8), 0x0000_ffff_ffff_ffff);
    assert_eq!(read_i64_at(&payload, 16), 0x0000_ffff_ffff_ffff);
    assert_eq!(read_i64_at(&payload, 24), 1);
    assert_eq!(read_i64_at(&payload, 32), i64::MAX);
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
