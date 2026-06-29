use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time;

use crate::codec::{
    Reader, Writer, read_auto_add_data, read_event_info, read_file_data, read_manual_auto_add_data,
    read_notify_srv_info, read_nw_play_time_shift_info, read_rec_file_info, read_reserve_data,
    read_service_event_info, read_service_info, read_tuner_process_status_info,
    read_tuner_reserve_info, write_auto_add_data, write_i64_vector, write_reserve_data,
    write_search_key_info, write_string_vector,
};
use crate::error::{EdcbError, Result};
use crate::types::*;

const CMD_SUCCESS: i32 = 1;
const CMD_VER: u16 = 5;
const CMD_EPG_SRV_DEL_RESERVE: i32 = 1014;
const CMD_EPG_SRV_ENUM_TUNER_RESERVE: i32 = 1016;
const CMD_EPG_SRV_DEL_RECINFO: i32 = 1018;
const CMD_EPG_SRV_CHG_PATH_RECINFO: i32 = 1019;
const CMD_EPG_SRV_ENUM_SERVICE: i32 = 1021;
const CMD_EPG_SRV_SEARCH_PG: i32 = 1025;
const CMD_EPG_SRV_ENUM_PG_INFO_EX: i32 = 1029;
const CMD_EPG_SRV_ENUM_PG_ARC: i32 = 1030;
const CMD_EPG_SRV_DEL_AUTO_ADD: i32 = 1033;
const CMD_EPG_SRV_DEL_MANU_ADD: i32 = 1043;
const CMD_EPG_SRV_EPG_CAP_NOW: i32 = 1053;
const CMD_EPG_SRV_FILE_COPY: i32 = 1060;
const CMD_EPG_SRV_ENUM_PLUGIN: i32 = 1061;
const CMD_EPG_SRV_ENUM_TUNER_PROCESS: i32 = 1066;
const CMD_EPG_SRV_NWPLAY_CLOSE: i32 = 1081;
const CMD_EPG_SRV_NWPLAY_TF_OPEN: i32 = 1087;
const CMD_EPG_SRV_GET_NETWORK_PATH: i32 = 1299;
const CMD_EPG_SRV_ENUM_RESERVE2: i32 = 2011;
const CMD_EPG_SRV_GET_RESERVE2: i32 = 2012;
const CMD_EPG_SRV_ADD_RESERVE2: i32 = 2013;
const CMD_EPG_SRV_CHG_RESERVE2: i32 = 2015;
const CMD_EPG_SRV_CHG_PROTECT_RECINFO2: i32 = 2019;
const CMD_EPG_SRV_ENUM_RECINFO_BASIC2: i32 = 2020;
const CMD_EPG_SRV_GET_RECINFO2: i32 = 2024;
const CMD_EPG_SRV_FILE_COPY2: i32 = 2060;
const CMD_EPG_SRV_ENUM_AUTO_ADD2: i32 = 2131;
const CMD_EPG_SRV_ADD_AUTO_ADD2: i32 = 2132;
const CMD_EPG_SRV_CHG_AUTO_ADD2: i32 = 2134;
const CMD_EPG_SRV_ENUM_MANU_ADD2: i32 = 2141;
const CMD_EPG_SRV_ADD_MANU_ADD2: i32 = 2142;
const CMD_EPG_SRV_CHG_MANU_ADD2: i32 = 2144;
const CMD_EPG_SRV_GET_STATUS_NOTIFY2: i32 = 2200;

const DEFAULT_RESERVE_ID: i32 = 0x7fff_ffff;

#[derive(Debug, Clone)]
pub struct EdcbClient {
    config: ConnectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 4510,
            timeout: Duration::from_secs(15),
        }
    }
}

impl ConnectionConfig {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Self::default()
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl EdcbClient {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config }
    }

    pub async fn enum_service(&self) -> Result<Vec<ServiceInfo>> {
        let body = self.send_cmd(CMD_EPG_SRV_ENUM_SERVICE, |_| {}).await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(read_service_info)
    }

    pub async fn enum_pg_info_ex(
        &self,
        service_time_list: &[i64],
    ) -> Result<Vec<ServiceEventInfo>> {
        let body = self
            .send_cmd(CMD_EPG_SRV_ENUM_PG_INFO_EX, |writer| {
                write_i64_vector(writer, service_time_list)
            })
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(read_service_event_info)
    }

    pub async fn enum_pg_arc(&self, service_time_list: &[i64]) -> Result<Vec<ServiceEventInfo>> {
        let body = self
            .send_cmd(CMD_EPG_SRV_ENUM_PG_ARC, |writer| {
                write_i64_vector(writer, service_time_list)
            })
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(read_service_event_info)
    }

    pub async fn file_copy(&self, name: &str) -> Result<Vec<u8>> {
        self.send_cmd(CMD_EPG_SRV_FILE_COPY, |writer| writer.write_string(name))
            .await
    }

    pub async fn file_copy2(&self, names: &[String]) -> Result<Vec<FileData>> {
        let body = self
            .send_cmd2(CMD_EPG_SRV_FILE_COPY2, |writer| {
                write_string_vector(writer, names)
            })
            .await?;
        read_versioned(&body, |reader| reader.read_vector(read_file_data))
    }

    pub async fn enum_reserve(&self) -> Result<Vec<ReserveData>> {
        let body = self.send_cmd2(CMD_EPG_SRV_ENUM_RESERVE2, |_| {}).await?;
        read_versioned(&body, |reader| reader.read_vector(read_reserve_data))
    }

    pub async fn get_reserve(&self, reserve_id: i32) -> Result<ReserveData> {
        let body = self
            .send_cmd2(CMD_EPG_SRV_GET_RESERVE2, |writer| {
                writer.write_i32(reserve_id)
            })
            .await?;
        read_versioned(&body, read_reserve_data)
    }

    pub async fn get_default_reserve(&self) -> Result<ReserveData> {
        self.get_reserve(DEFAULT_RESERVE_ID).await
    }

    pub async fn add_reserve(&self, reserve: &ReserveData) -> Result<()> {
        self.add_reserves(std::slice::from_ref(reserve)).await
    }

    pub async fn add_reserves(&self, reserves: &[ReserveData]) -> Result<()> {
        self.send_cmd2(CMD_EPG_SRV_ADD_RESERVE2, |writer| {
            writer.write_vector(reserves, write_reserve_data)
        })
        .await?;
        Ok(())
    }

    pub async fn change_reserve(&self, reserve: &ReserveData) -> Result<()> {
        self.change_reserves(std::slice::from_ref(reserve)).await
    }

    pub async fn change_reserves(&self, reserves: &[ReserveData]) -> Result<()> {
        self.send_cmd2(CMD_EPG_SRV_CHG_RESERVE2, |writer| {
            writer.write_vector(reserves, write_reserve_data)
        })
        .await?;
        Ok(())
    }

    pub async fn delete_reserve(&self, reserve_id: i32) -> Result<()> {
        self.delete_reserves(&[reserve_id]).await
    }

    pub async fn delete_reserves(&self, reserve_ids: &[i32]) -> Result<()> {
        self.send_cmd(CMD_EPG_SRV_DEL_RESERVE, |writer| {
            writer.write_vector(reserve_ids, |writer, reserve_id| {
                writer.write_i32(*reserve_id)
            })
        })
        .await?;
        Ok(())
    }

    pub async fn enum_rec_info_basic(&self) -> Result<Vec<RecFileInfo>> {
        let body = self
            .send_cmd2(CMD_EPG_SRV_ENUM_RECINFO_BASIC2, |_| {})
            .await?;
        read_versioned(&body, |reader| reader.read_vector(read_rec_file_info))
    }

    pub async fn get_rec_info(&self, info_id: i32) -> Result<RecFileInfo> {
        let body = self
            .send_cmd2(CMD_EPG_SRV_GET_RECINFO2, |writer| writer.write_i32(info_id))
            .await?;
        read_versioned(&body, read_rec_file_info)
    }

    pub async fn get_rec_file_network_path(&self, path: &str) -> Result<String> {
        let body = self
            .send_cmd(CMD_EPG_SRV_GET_NETWORK_PATH, |writer| {
                writer.write_string(path)
            })
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_string()
    }

    pub async fn get_rec_file_path(&self, reserve_id: i32) -> Result<String> {
        let body = self
            .send_cmd(CMD_EPG_SRV_NWPLAY_TF_OPEN, |writer| {
                writer.write_i32(reserve_id)
            })
            .await?;
        let mut reader = Reader::new(&body);
        let info = read_nw_play_time_shift_info(&mut reader)?;
        let _ = self
            .send_cmd(CMD_EPG_SRV_NWPLAY_CLOSE, |writer| {
                writer.write_i32(info.ctrl_id)
            })
            .await;
        Ok(info.file_path)
    }

    pub async fn enum_tuner_reserve(&self) -> Result<Vec<TunerReserveInfo>> {
        let body = self
            .send_cmd(CMD_EPG_SRV_ENUM_TUNER_RESERVE, |_| {})
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(read_tuner_reserve_info)
    }

    pub async fn enum_tuner_process(&self) -> Result<Vec<TunerProcessStatusInfo>> {
        let body = self
            .send_cmd(CMD_EPG_SRV_ENUM_TUNER_PROCESS, |_| {})
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(read_tuner_process_status_info)
    }

    pub async fn enum_plugin(&self, kind: PluginKind) -> Result<Vec<String>> {
        let body = self
            .send_cmd(CMD_EPG_SRV_ENUM_PLUGIN, |writer| {
                writer.write_u16(kind as u16)
            })
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(|reader| reader.read_string())
    }

    pub async fn search_pg(&self, keys: &[SearchKeyInfo]) -> Result<Vec<EventInfo>> {
        let body = self
            .send_cmd(CMD_EPG_SRV_SEARCH_PG, |writer| {
                writer.write_vector(keys, write_search_key_info)
            })
            .await?;
        let mut reader = Reader::new(&body);
        reader.read_vector(read_event_info)
    }

    pub async fn enum_auto_add(&self) -> Result<Vec<AutoAddData>> {
        let body = self.send_cmd2(CMD_EPG_SRV_ENUM_AUTO_ADD2, |_| {}).await?;
        read_versioned(&body, |reader| reader.read_vector(read_auto_add_data))
    }

    pub async fn add_auto_add(&self, data: &AutoAddData) -> Result<()> {
        self.add_auto_adds(std::slice::from_ref(data)).await
    }

    pub async fn add_auto_adds(&self, data_list: &[AutoAddData]) -> Result<()> {
        self.send_cmd2(CMD_EPG_SRV_ADD_AUTO_ADD2, |writer| {
            writer.write_vector(data_list, write_auto_add_data)
        })
        .await?;
        Ok(())
    }

    pub async fn change_auto_add(&self, data: &AutoAddData) -> Result<()> {
        self.change_auto_adds(std::slice::from_ref(data)).await
    }

    pub async fn change_auto_adds(&self, data_list: &[AutoAddData]) -> Result<()> {
        self.send_cmd2(CMD_EPG_SRV_CHG_AUTO_ADD2, |writer| {
            writer.write_vector(data_list, write_auto_add_data)
        })
        .await?;
        Ok(())
    }

    pub async fn delete_auto_add(&self, data_id: i32) -> Result<()> {
        self.delete_auto_adds(&[data_id]).await
    }

    pub async fn delete_auto_adds(&self, data_ids: &[i32]) -> Result<()> {
        self.send_cmd(CMD_EPG_SRV_DEL_AUTO_ADD, |writer| {
            writer.write_vector(data_ids, |writer, data_id| writer.write_i32(*data_id))
        })
        .await?;
        Ok(())
    }

    pub async fn enum_manual_add(&self) -> Result<Vec<ManualAutoAddData>> {
        let body = self.send_cmd2(CMD_EPG_SRV_ENUM_MANU_ADD2, |_| {}).await?;
        read_versioned(&body, |reader| {
            reader.read_vector(read_manual_auto_add_data)
        })
    }

    pub async fn get_notify_srv_info(&self, target_count: u32) -> Result<NotifySrvInfo> {
        let body = self
            .send_cmd2(CMD_EPG_SRV_GET_STATUS_NOTIFY2, |writer| {
                writer.write_u32(target_count)
            })
            .await?;
        read_versioned(&body, read_notify_srv_info)
    }

    pub async fn get_notify_srv_status(&self) -> Result<NotifySrvInfo> {
        self.get_notify_srv_info(0).await
    }

    async fn send_cmd(&self, cmd: i32, write_payload: impl FnOnce(&mut Writer)) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.write_i32(cmd);
        writer.write_i32(0);
        write_payload(&mut writer);
        let payload_len = i32::try_from(writer.as_slice().len() - 8)
            .expect("EDCB request payload length fits in i32");
        writer.write_i32_at(4, payload_len);
        self.send_and_receive(writer.into_inner()).await
    }

    async fn send_cmd2(
        &self,
        cmd: i32,
        write_payload: impl FnOnce(&mut Writer),
    ) -> Result<Vec<u8>> {
        self.send_cmd(cmd, |writer| {
            writer.write_u16(CMD_VER);
            write_payload(writer);
        })
        .await
    }

    async fn send_and_receive(&self, request: Vec<u8>) -> Result<Vec<u8>> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let mut stream = time::timeout(self.config.timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| EdcbError::Timeout)??;

        time::timeout(self.config.timeout, stream.write_all(&request))
            .await
            .map_err(|_| EdcbError::Timeout)??;

        let mut header = [0_u8; 8];
        time::timeout(self.config.timeout, stream.read_exact(&mut header))
            .await
            .map_err(|_| EdcbError::Timeout)??;

        let status = i32::from_le_bytes(header[0..4].try_into().expect("header field length"));
        let size = i32::from_le_bytes(header[4..8].try_into().expect("header field length"));
        let size = usize::try_from(size)
            .map_err(|_| EdcbError::Decode("negative response body size".to_string()))?;
        let mut body = vec![0_u8; size];
        time::timeout(self.config.timeout, stream.read_exact(&mut body))
            .await
            .map_err(|_| EdcbError::Timeout)??;

        if status == CMD_SUCCESS {
            Ok(body)
        } else {
            Err(EdcbError::CommandStatus(status))
        }
    }
}

fn read_versioned<T>(body: &[u8], read: impl FnOnce(&mut Reader<'_>) -> Result<T>) -> Result<T> {
    let mut reader = Reader::new(body);
    let version = reader.read_u16()?;
    if version < CMD_VER {
        return Err(EdcbError::Decode(format!(
            "unsupported EDCB response version {version}"
        )));
    }
    read(&mut reader)
}

const _: () = {
    let _ = CMD_EPG_SRV_DEL_RESERVE;
    let _ = CMD_EPG_SRV_DEL_RECINFO;
    let _ = CMD_EPG_SRV_CHG_PATH_RECINFO;
    let _ = CMD_EPG_SRV_EPG_CAP_NOW;
    let _ = CMD_EPG_SRV_GET_RESERVE2;
    let _ = CMD_EPG_SRV_ADD_RESERVE2;
    let _ = CMD_EPG_SRV_CHG_RESERVE2;
    let _ = CMD_EPG_SRV_CHG_PROTECT_RECINFO2;
    let _ = CMD_EPG_SRV_ADD_MANU_ADD2;
    let _ = CMD_EPG_SRV_CHG_MANU_ADD2;
    let _ = CMD_EPG_SRV_DEL_MANU_ADD;
};
