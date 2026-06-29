use std::collections::BTreeMap;
use std::time::Duration;

use crate::{EdcbClient, EventKey, PluginKind, ProgramSearchQuery, ServiceKey, flows};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 4510,
            timeout: Duration::from_secs(15),
        }
    }
}

impl ServerConfig {
    pub fn from_env_args() -> Result<Self, String> {
        Self::from_args_and_env(std::env::args(), std::env::vars())
    }

    pub fn from_args_and_env<A, S, E, K, V>(args: A, env: E) -> Result<Self, String>
    where
        A: IntoIterator<Item = S>,
        S: AsRef<str>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let env: BTreeMap<String, String> = env
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string()))
            .collect();

        let mut config = Self::default();
        if let Some(host) = env.get("EDCB_HOST") {
            config.host.clone_from(host);
        }
        if let Some(port) = env.get("EDCB_PORT") {
            config.port = parse_port(port)?;
        }
        if let Some(timeout) = env.get("EDCB_TIMEOUT_SECONDS") {
            config.timeout = Duration::from_secs(parse_timeout(timeout)?);
        }

        let args: Vec<String> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string())
            .collect();
        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--host" => {
                    index += 1;
                    config.host = args
                        .get(index)
                        .ok_or_else(|| "--host requires a value".to_string())?
                        .clone();
                }
                "--port" => {
                    index += 1;
                    config.port = parse_port(
                        args.get(index)
                            .ok_or_else(|| "--port requires a value".to_string())?,
                    )?;
                }
                "--timeout-seconds" => {
                    index += 1;
                    config.timeout = Duration::from_secs(parse_timeout(
                        args.get(index)
                            .ok_or_else(|| "--timeout-seconds requires a value".to_string())?,
                    )?);
                }
                unknown => return Err(format!("unknown argument {unknown}")),
            }
            index += 1;
        }

        Ok(config)
    }
}

fn parse_port(value: &str) -> Result<u16, String> {
    value
        .parse()
        .map_err(|_| format!("port must be a number in 0..=65535: {value}"))
}

fn parse_timeout(value: &str) -> Result<u64, String> {
    let timeout = value
        .parse::<u64>()
        .map_err(|_| format!("timeout must be a positive integer: {value}"))?;
    if timeout == 0 {
        Err("timeout must be greater than zero".to_string())
    } else {
        Ok(timeout)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecordedInfoParam {
    pub info_id: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PluginKindParam {
    pub kind: String,
}

impl PluginKindParam {
    pub fn try_into_plugin_kind(&self) -> Result<PluginKind, String> {
        match self.kind.as_str() {
            "write" => Ok(PluginKind::Write),
            "rec_name" => Ok(PluginKind::RecName),
            value => Err(format!(
                "unsupported plugin kind {value}; expected \"write\" or \"rec_name\""
            )),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsParam {
    pub keyword: String,
    #[serde(default)]
    pub title_only: bool,
    pub service: Option<String>,
}

impl SearchProgramsParam {
    fn try_into_query(&self) -> Result<ProgramSearchQuery, String> {
        let service = self
            .service
            .as_deref()
            .map(|value| value.parse::<ServiceKey>())
            .transpose()?;
        Ok(ProgramSearchQuery {
            keyword: self.keyword.clone(),
            title_only: self.title_only,
            service,
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReservationEventParam {
    pub event: String,
}

impl ReservationEventParam {
    fn try_into_event_key(&self) -> Result<EventKey, String> {
        self.event.parse()
    }
}

#[derive(Debug, Clone)]
pub struct EdcbMcpServer {
    config: ServerConfig,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EdcbMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("EDCB CtrlCmd MCP server")
    }
}

#[tool_router(router = tool_router)]
impl EdcbMcpServer {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        names.sort();
        names
    }

    #[tool(name = "list_services", description = "List EDCB services")]
    pub async fn list_services(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_service().await)
    }

    #[tool(name = "list_reserves", description = "List EDCB reserves")]
    pub async fn list_reserves(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_reserve().await)
    }

    #[tool(name = "list_recorded", description = "List EDCB recorded file info")]
    pub async fn list_recorded(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_rec_info_basic().await)
    }

    #[tool(
        name = "get_recorded_info",
        description = "Get one EDCB recorded file info item"
    )]
    pub async fn get_recorded_info(
        &self,
        Parameters(params): Parameters<RecordedInfoParam>,
    ) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().get_rec_info(params.info_id).await)
    }

    #[tool(
        name = "search_programs",
        description = "Search EDCB programs by keyword, optionally title-only or scoped to one service"
    )]
    pub async fn search_programs(
        &self,
        Parameters(params): Parameters<SearchProgramsParam>,
    ) -> Result<CallToolResult, String> {
        let query = params.try_into_query()?;
        let client = self.client();
        to_call_tool_result(flows::search_programs(&client, &query).await)
    }

    #[tool(
        name = "preview_reservation",
        description = "Build an EDCB reservation from an event without sending a mutation command"
    )]
    pub async fn preview_reservation(
        &self,
        Parameters(params): Parameters<ReservationEventParam>,
    ) -> Result<CallToolResult, String> {
        let event_key = params.try_into_event_key()?;
        let client = self.client();
        to_call_tool_result(flows::preview_reservation(&client, event_key).await)
    }

    #[tool(
        name = "create_reservation",
        description = "Create an EDCB reservation from an event using the server default reservation settings"
    )]
    pub async fn create_reservation(
        &self,
        Parameters(params): Parameters<ReservationEventParam>,
    ) -> Result<CallToolResult, String> {
        let event_key = params.try_into_event_key()?;
        let client = self.client();
        to_call_tool_result(flows::create_reservation(&client, event_key).await)
    }

    #[tool(
        name = "list_tuner_reserves",
        description = "List EDCB tuner reserve state"
    )]
    pub async fn list_tuner_reserves(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_tuner_reserve().await)
    }

    #[tool(
        name = "list_tuner_processes",
        description = "List EDCB tuner process state"
    )]
    pub async fn list_tuner_processes(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_tuner_process().await)
    }

    #[tool(name = "list_plugins", description = "List EDCB plugins by kind")]
    pub async fn list_plugins(
        &self,
        Parameters(params): Parameters<PluginKindParam>,
    ) -> Result<CallToolResult, String> {
        let kind = params.try_into_plugin_kind()?;
        to_call_tool_result(self.client().enum_plugin(kind).await)
    }

    #[tool(
        name = "get_notify_status",
        description = "Get current EDCB notify status"
    )]
    pub async fn get_notify_status(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().get_notify_srv_status().await)
    }

    fn client(&self) -> EdcbClient {
        let mut client = EdcbClient::new(self.config.host.clone(), self.config.port);
        client.set_timeout(self.config.timeout);
        client
    }
}

fn to_call_tool_result<T: Serialize>(result: crate::Result<T>) -> Result<CallToolResult, String> {
    let value = result.map_err(|error| error.to_string())?;
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|error| format!("failed to serialize tool response: {error}"))
}
