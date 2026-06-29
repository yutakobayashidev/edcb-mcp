use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use edcb_tools::{EdcbClient, EdcbError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

async fn spawn_single_response_server(
    response_status: i32,
    response_body: Vec<u8>,
) -> (SocketAddr, JoinHandle<()>) {
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
        let mut header = [0_u8; 8];
        socket
            .read_exact(&mut header)
            .await
            .expect("mock EDCB server should read request header");
        let payload_len = i32::from_le_bytes(
            header[4..8]
                .try_into()
                .expect("request header length field is exactly four bytes"),
        );
        let payload_len =
            usize::try_from(payload_len).expect("request payload length must be non-negative");
        let mut payload = vec![0_u8; payload_len];
        socket
            .read_exact(&mut payload)
            .await
            .expect("mock EDCB server should read request payload");

        assert_eq!(
            i32::from_le_bytes(
                header[0..4]
                    .try_into()
                    .expect("request command field is exactly four bytes"),
            ),
            1021
        );
        assert!(payload.is_empty());

        socket
            .write_i32_le(response_status)
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
    });

    (addr, handle)
}

#[tokio::test]
async fn enum_service_sends_command_and_decodes_response() {
    let response_body = edcb_tools::test_support::encode_service_list_for_test();
    let (addr, server) = spawn_single_response_server(1, response_body).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let services = client.enum_service().await.unwrap();
    server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(services.len(), 1);
    assert_eq!(services[0].service_name, "Test Service");
    assert_eq!(services[0].onid, 1);
    assert_eq!(services[0].remote_control_key_id, 7);
}

#[tokio::test]
async fn command_status_failure_is_reported() {
    let (addr, server) = spawn_single_response_server(203, Vec::new()).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let error = client.enum_service().await.unwrap_err();
    server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert!(matches!(error, EdcbError::CommandStatus(203)));
}

#[tokio::test]
async fn io_failures_are_reported() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_millis(50));

    let error = client.enum_service().await.unwrap_err();

    assert!(matches!(&error, EdcbError::Io(_)) || matches!(&error, EdcbError::Timeout));
    if let EdcbError::Io(source) = &error {
        assert!(
            matches!(
                source.kind(),
                io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::TimedOut
            ),
            "unexpected io kind: {:?}",
            source.kind()
        );
    }
}
