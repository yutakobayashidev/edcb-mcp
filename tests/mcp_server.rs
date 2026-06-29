use std::net::SocketAddr;
use std::time::Duration;

use edcb_tools::mcp::{EdcbMcpServer, PluginKindParam, ServerConfig};
use rmcp::ServiceExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

async fn spawn_service_list_server() -> (SocketAddr, JoinHandle<()>) {
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

        let response_body = edcb_tools::test_support::encode_service_list_for_test();
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
    });

    (addr, handle)
}

#[test]
fn config_uses_cli_then_env_then_defaults() {
    let config = ServerConfig::from_args_and_env(
        ["edcb-mcp", "--host", "192.0.2.10", "--port", "5510"],
        [
            ("EDCB_HOST", "127.0.0.2"),
            ("EDCB_PORT", "4511"),
            ("EDCB_TIMEOUT_SECONDS", "3"),
        ],
    )
    .expect("config should parse");

    assert_eq!(config.host, "192.0.2.10");
    assert_eq!(config.port, 5510);
    assert_eq!(config.timeout, Duration::from_secs(3));

    let default_config =
        ServerConfig::from_args_and_env(["edcb-mcp"], std::iter::empty::<(&str, &str)>())
            .expect("default config should parse");
    assert_eq!(default_config.host, "127.0.0.1");
    assert_eq!(default_config.port, 4510);
    assert_eq!(default_config.timeout, Duration::from_secs(15));
}

#[test]
fn invalid_config_reports_the_bad_field() {
    let error = ServerConfig::from_args_and_env(
        ["edcb-mcp", "--port", "nope"],
        std::iter::empty::<(&str, &str)>(),
    )
    .expect_err("invalid port should fail");

    assert!(error.contains("port"));
}

#[test]
fn plugin_kind_param_parses_supported_values() {
    assert_eq!(
        PluginKindParam {
            kind: "write".to_string()
        }
        .try_into_plugin_kind()
        .expect("write plugin kind should parse") as u16,
        2
    );
    assert_eq!(
        PluginKindParam {
            kind: "rec_name".to_string()
        }
        .try_into_plugin_kind()
        .expect("rec_name plugin kind should parse") as u16,
        1
    );
    assert!(
        PluginKindParam {
            kind: "other".to_string()
        }
        .try_into_plugin_kind()
        .is_err()
    );
}

#[test]
fn mcp_server_exposes_v1_tools() {
    let server = EdcbMcpServer::new(ServerConfig::default());
    let tool_names: Vec<_> = server.tool_names();

    assert_eq!(
        tool_names,
        vec![
            "create_reservation",
            "delete_reservation",
            "get_notify_status",
            "get_recorded_info",
            "get_reservation",
            "list_plugins",
            "list_recorded",
            "list_reserves",
            "list_services",
            "list_tuner_processes",
            "list_tuner_reserves",
            "preview_reservation",
            "search_programs",
            "update_reservation",
        ]
    );
}

#[tokio::test]
async fn mcp_service_lists_tools_over_transport() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_handle = tokio::spawn(async move {
        let service = EdcbMcpServer::new(ServerConfig::default())
            .serve(server_transport)
            .await
            .expect("MCP server should start over duplex transport");
        service
            .waiting()
            .await
            .expect("MCP server should shut down cleanly");
    });

    let client =
        ().serve(client_transport)
            .await
            .expect("MCP client should start over duplex transport");
    let tools = client
        .list_all_tools()
        .await
        .expect("MCP client should list tools");
    let names: Vec<_> = tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();

    assert!(names.contains(&"list_services".to_string()));
    assert!(names.contains(&"get_notify_status".to_string()));

    client.cancel().await.expect("MCP client should cancel");
    server_handle
        .await
        .expect("MCP server task should complete without panicking");
}

#[tokio::test]
async fn list_services_tool_returns_structured_service_info() {
    let (addr, server_task) = spawn_service_list_server().await;
    let server = EdcbMcpServer::new(ServerConfig {
        host: addr.ip().to_string(),
        port: addr.port(),
        timeout: Duration::from_secs(1),
    });

    let services = server
        .list_services()
        .await
        .expect("list_services tool should call EDCB successfully");
    server_task
        .await
        .expect("mock EDCB server task should complete without panicking");

    let services = services
        .structured_content
        .expect("list_services should return structured content");
    assert_eq!(services[0]["service_name"], "Test Service");
    assert_eq!(services[0]["remote_control_key_id"], 7);
}
