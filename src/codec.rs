use chrono::{DateTime, FixedOffset, TimeZone};

use crate::error::{EdcbError, Result};
use crate::types::*;

const JST_OFFSET_SECONDS: i32 = 9 * 3600;

pub(crate) fn jst() -> FixedOffset {
    FixedOffset::east_opt(JST_OFFSET_SECONDS).expect("JST offset is valid")
}

pub(crate) fn unix_epoch_jst() -> DateTime<FixedOffset> {
    jst()
        .with_ymd_and_hms(1970, 1, 1, 9, 0, 0)
        .single()
        .expect("JST UNIX epoch fallback is valid")
}

pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub(crate) fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    pub(crate) fn write_u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub(crate) fn write_u16(&mut self, value: u16) {
        self.buf.extend(value.to_le_bytes());
    }

    pub(crate) fn write_i32(&mut self, value: i32) {
        self.buf.extend(value.to_le_bytes());
    }

    pub(crate) fn write_u32(&mut self, value: u32) {
        self.buf.extend(value.to_le_bytes());
    }

    pub(crate) fn write_i64(&mut self, value: i64) {
        self.buf.extend(value.to_le_bytes());
    }

    pub(crate) fn write_i32_at(&mut self, pos: usize, value: i32) {
        self.buf[pos..pos + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn write_bytes(&mut self, value: &[u8]) {
        self.buf.extend(value);
    }

    pub(crate) fn write_string(&mut self, value: &str) {
        let encoded: Vec<u8> = value
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        let size = i32::try_from(6 + encoded.len()).expect("EDCB string fits in i32");
        self.write_i32(size);
        self.buf.extend(encoded);
        self.write_u16(0);
    }

    pub(crate) fn write_system_time(&mut self, value: DateTime<FixedOffset>) {
        use chrono::{Datelike, Timelike};

        self.write_u16(u16::try_from(value.year()).expect("SYSTEMTIME year fits in u16"));
        self.write_u16(u16::try_from(value.month()).expect("SYSTEMTIME month fits in u16"));
        self.write_u16(
            u16::try_from(value.weekday().num_days_from_sunday())
                .expect("SYSTEMTIME day of week fits in u16"),
        );
        self.write_u16(u16::try_from(value.day()).expect("SYSTEMTIME day fits in u16"));
        self.write_u16(u16::try_from(value.hour()).expect("SYSTEMTIME hour fits in u16"));
        self.write_u16(u16::try_from(value.minute()).expect("SYSTEMTIME minute fits in u16"));
        self.write_u16(u16::try_from(value.second()).expect("SYSTEMTIME second fits in u16"));
        self.write_u16(0);
    }

    pub(crate) fn write_struct(&mut self, write: impl FnOnce(&mut Writer)) {
        let pos = self.buf.len();
        self.write_i32(0);
        write(self);
        let size = i32::try_from(self.buf.len() - pos).expect("EDCB struct fits in i32");
        self.write_i32_at(pos, size);
    }

    pub(crate) fn write_vector<T>(&mut self, values: &[T], write: impl Fn(&mut Writer, &T)) {
        let pos = self.buf.len();
        self.write_i32(0);
        self.write_i32(i32::try_from(values.len()).expect("EDCB vector length fits in i32"));
        for value in values {
            write(self, value);
        }
        let size = i32::try_from(self.buf.len() - pos).expect("EDCB vector fits in i32");
        self.write_i32_at(pos, size);
    }
}

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.pos == self.buf.len()
    }

    fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.remaining() < len {
            return Err(EdcbError::Decode(format!(
                "buffer too short: need {len} bytes, remaining {}",
                self.remaining()
            )));
        }
        let start = self.pos;
        self.pos += len;
        Ok(&self.buf[start..start + len])
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.read_exact(2)?
                .try_into()
                .expect("slice length checked"),
        ))
    }

    pub(crate) fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(
            self.read_exact(4)?
                .try_into()
                .expect("slice length checked"),
        ))
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.read_exact(4)?
                .try_into()
                .expect("slice length checked"),
        ))
    }

    pub(crate) fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(
            self.read_exact(8)?
                .try_into()
                .expect("slice length checked"),
        ))
    }

    fn peek_i32(&self) -> Result<i32> {
        if self.remaining() < 4 {
            return Err(EdcbError::Decode(
                "buffer too short for i32 peek".to_string(),
            ));
        }
        Ok(i32::from_le_bytes(
            self.buf[self.pos..self.pos + 4]
                .try_into()
                .expect("slice length checked"),
        ))
    }

    pub(crate) fn read_string(&mut self) -> Result<String> {
        let size = self.read_i32()?;
        if size < 6 {
            return Err(EdcbError::Decode(format!("invalid string size {size}")));
        }
        let body_len = usize::try_from(size - 6)
            .map_err(|_| EdcbError::Decode(format!("invalid string body size {size}")))?;
        let total_len = usize::try_from(size - 4)
            .map_err(|_| EdcbError::Decode(format!("invalid string total size {size}")))?;
        if total_len < body_len || self.remaining() < total_len {
            return Err(EdcbError::Decode(format!("truncated string size {size}")));
        }
        if body_len % 2 != 0 {
            return Err(EdcbError::Decode(
                "UTF-16LE string has odd byte length".to_string(),
            ));
        }

        let raw = self.read_exact(body_len)?;
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let value = String::from_utf16(&units)
            .map_err(|err| EdcbError::Decode(format!("invalid UTF-16LE string: {err}")))?;
        self.read_exact(2)?;
        Ok(value)
    }

    pub(crate) fn read_system_time(&mut self) -> Result<DateTime<FixedOffset>> {
        let year = self.read_u16()? as i32;
        let month = self.read_u16()? as u32;
        self.read_u16()?;
        let day = self.read_u16()? as u32;
        let hour = self.read_u16()? as u32;
        let minute = self.read_u16()? as u32;
        let second = self.read_u16()? as u32;
        self.read_u16()?;

        Ok(jst()
            .with_ymd_and_hms(year, month, day, hour, minute, second)
            .single()
            .unwrap_or_else(unix_epoch_jst))
    }

    pub(crate) fn read_struct<T>(
        &mut self,
        read: impl FnOnce(&mut Reader<'_>) -> Result<T>,
    ) -> Result<T> {
        let size = self.read_i32()?;
        if size < 4 {
            return Err(EdcbError::Decode(format!("invalid struct size {size}")));
        }
        let body_len = usize::try_from(size - 4)
            .map_err(|_| EdcbError::Decode(format!("invalid struct size {size}")))?;
        let body = self.read_exact(body_len)?;
        let mut child = Reader::new(body);
        read(&mut child)
    }

    pub(crate) fn read_vector<T>(
        &mut self,
        read: impl Fn(&mut Reader<'_>) -> Result<T>,
    ) -> Result<Vec<T>> {
        let size = self.read_i32()?;
        let count = self.read_i32()?;
        if size < 8 || count < 0 {
            return Err(EdcbError::Decode(format!(
                "invalid vector size {size} or count {count}"
            )));
        }
        let body_len = usize::try_from(size - 8)
            .map_err(|_| EdcbError::Decode(format!("invalid vector size {size}")))?;
        let count = usize::try_from(count)
            .map_err(|_| EdcbError::Decode("negative vector count".to_string()))?;
        let body = self.read_exact(body_len)?;
        let mut child = Reader::new(body);
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read(&mut child)?);
        }
        Ok(values)
    }
}

pub(crate) fn write_i64_vector(writer: &mut Writer, values: &[i64]) {
    writer.write_vector(values, |writer, value| writer.write_i64(*value));
}

pub(crate) fn write_string_vector(writer: &mut Writer, values: &[String]) {
    writer.write_vector(values, |writer, value| writer.write_string(value));
}

pub(crate) fn write_search_key_info(writer: &mut Writer, value: &SearchKeyInfo) {
    write_search_key_info_inner(writer, value, false);
}

fn write_search_key_info2(writer: &mut Writer, value: &SearchKeyInfo) {
    write_search_key_info_inner(writer, value, true);
}

fn write_search_key_info_inner(
    writer: &mut Writer,
    value: &SearchKeyInfo,
    include_recording_checks: bool,
) {
    writer.write_struct(|writer| {
        let chk_duration = (u32::from(value.chk_duration_min) * 10000
            + u32::from(value.chk_duration_max))
            % 100000000;
        let mut and_key = String::new();
        if value.key_disabled {
            and_key.push_str("^!{999}");
        }
        if value.case_sensitive {
            and_key.push_str("C!{999}");
        }
        if chk_duration > 0 {
            and_key.push_str(&format!("D!{{1{chk_duration:08}}}"));
        }
        and_key.push_str(&value.and_key);

        writer.write_string(&and_key);
        writer.write_string(&value.not_key);
        writer.write_i32(if value.reg_exp_flag { 1 } else { 0 });
        writer.write_i32(if value.title_only_flag { 1 } else { 0 });
        writer.write_vector(&value.content_list, write_content_data);
        writer.write_vector(&value.date_list, write_search_date_info);
        writer.write_vector(&value.service_list, |writer, value| {
            writer.write_i64(*value)
        });
        writer.write_vector(&value.video_list, |writer, value| writer.write_u16(*value));
        writer.write_vector(&value.audio_list, |writer, value| writer.write_u16(*value));
        writer.write_u8(u8::from(value.aimai_flag));
        writer.write_u8(u8::from(value.not_contet_flag));
        writer.write_u8(u8::from(value.not_date_flag));
        writer.write_u8(value.free_ca_flag);
        if include_recording_checks {
            writer.write_u8(u8::from(value.chk_rec_end));
            let chk_rec_day = if value.chk_rec_no_service {
                40000 + value.chk_rec_day % 10000
            } else {
                value.chk_rec_day
            };
            writer.write_u16(chk_rec_day);
        }
    });
}

fn write_content_data(writer: &mut Writer, value: &ContentData) {
    writer.write_struct(|writer| {
        writer.write_u16(value.content_nibble.rotate_left(8));
        writer.write_u16(value.user_nibble.rotate_left(8));
    });
}

fn write_search_date_info(writer: &mut Writer, value: &SearchDateInfo) {
    writer.write_struct(|writer| {
        writer.write_u8(value.start_day_of_week);
        writer.write_u16(value.start_hour);
        writer.write_u16(value.start_min);
        writer.write_u8(value.end_day_of_week);
        writer.write_u16(value.end_hour);
        writer.write_u16(value.end_min);
    });
}

pub(crate) fn write_service_info(writer: &mut Writer, value: &ServiceInfo) {
    writer.write_struct(|writer| {
        writer.write_u16(value.onid);
        writer.write_u16(value.tsid);
        writer.write_u16(value.sid);
        writer.write_u8(value.service_type);
        writer.write_u8(value.partial_reception_flag);
        writer.write_string(&value.service_provider_name);
        writer.write_string(&value.service_name);
        writer.write_string(&value.network_name);
        writer.write_string(&value.ts_name);
        writer.write_u8(value.remote_control_key_id);
    });
}

pub(crate) fn read_file_data(reader: &mut Reader<'_>) -> Result<FileData> {
    reader.read_struct(|reader| {
        let name = reader.read_string()?;
        let data_size = reader.read_i32()?;
        reader.read_i32()?;
        let data_size = usize::try_from(data_size)
            .map_err(|_| EdcbError::Decode("negative file data size".to_string()))?;
        Ok(FileData {
            name,
            data: reader.read_exact(data_size)?.to_vec(),
        })
    })
}

fn read_rec_file_set_info(reader: &mut Reader<'_>) -> Result<RecFileSetInfo> {
    reader.read_struct(|reader| {
        let value = RecFileSetInfo {
            rec_folder: reader.read_string()?,
            write_plug_in: reader.read_string()?,
            rec_name_plug_in: reader.read_string()?,
        };
        reader.read_string()?;
        Ok(value)
    })
}

fn write_rec_file_set_info(writer: &mut Writer, value: &RecFileSetInfo) {
    writer.write_struct(|writer| {
        writer.write_string(&value.rec_folder);
        writer.write_string(&value.write_plug_in);
        writer.write_string(&value.rec_name_plug_in);
        writer.write_string("");
    });
}

fn read_rec_setting_data(reader: &mut Reader<'_>) -> Result<RecSettingData> {
    reader.read_struct(|reader| {
        let rec_mode = reader.read_u8()?;
        let priority = reader.read_u8()?;
        let tuijyuu_flag = reader.read_u8()? != 0;
        let service_mode = reader.read_u32()?;
        let pittari_flag = reader.read_u8()? != 0;
        let bat_file_path = reader.read_string()?;
        let rec_folder_list = reader.read_vector(read_rec_file_set_info)?;
        let suspend_mode = reader.read_u8()?;
        let reboot_flag = reader.read_u8()? != 0;
        let use_margin_flag = reader.read_u8()? != 0;
        let start_margin_value = reader.read_i32()?;
        let end_margin_value = reader.read_i32()?;
        Ok(RecSettingData {
            rec_mode,
            priority,
            tuijyuu_flag,
            service_mode,
            pittari_flag,
            bat_file_path,
            rec_folder_list,
            suspend_mode,
            reboot_flag,
            start_margin: use_margin_flag.then_some(start_margin_value),
            end_margin: use_margin_flag.then_some(end_margin_value),
            continue_rec_flag: reader.read_u8()? != 0,
            partial_rec_flag: reader.read_u8()?,
            tuner_id: reader.read_u32()?,
            partial_rec_folder: reader.read_vector(read_rec_file_set_info)?,
        })
    })
}

pub(crate) fn write_rec_setting_data(writer: &mut Writer, value: &RecSettingData) {
    writer.write_struct(|writer| {
        writer.write_u8(value.rec_mode);
        writer.write_u8(value.priority);
        writer.write_u8(u8::from(value.tuijyuu_flag));
        writer.write_u32(value.service_mode);
        writer.write_u8(u8::from(value.pittari_flag));
        writer.write_string(&value.bat_file_path);
        writer.write_vector(&value.rec_folder_list, write_rec_file_set_info);
        writer.write_u8(value.suspend_mode);
        writer.write_u8(u8::from(value.reboot_flag));
        let use_margin = value.start_margin.is_some() && value.end_margin.is_some();
        writer.write_u8(u8::from(use_margin));
        writer.write_i32(value.start_margin.unwrap_or_default());
        writer.write_i32(value.end_margin.unwrap_or_default());
        writer.write_u8(u8::from(value.continue_rec_flag));
        writer.write_u8(value.partial_rec_flag);
        writer.write_u32(value.tuner_id);
        writer.write_vector(&value.partial_rec_folder, write_rec_file_set_info);
    });
}

pub(crate) fn read_reserve_data(reader: &mut Reader<'_>) -> Result<ReserveData> {
    reader.read_struct(|reader| {
        let title = reader.read_string()?;
        let start_time = reader.read_system_time()?;
        let duration_second = reader.read_u32()?;
        let station_name = reader.read_string()?;
        let onid = reader.read_u16()?;
        let tsid = reader.read_u16()?;
        let sid = reader.read_u16()?;
        let eid = reader.read_u16()?;
        let comment = reader.read_string()?;
        let reserve_id = reader.read_i32()?;
        reader.read_u8()?;
        let overlap_mode = reader.read_u8()?;
        reader.read_string()?;
        let start_time_epg = reader.read_system_time()?;
        let rec_setting = read_rec_setting_data(reader)?;
        reader.read_i32()?;
        let rec_file_name_list = reader.read_vector(|reader| reader.read_string())?;
        reader.read_i32()?;

        Ok(ReserveData {
            title,
            start_time,
            duration_second,
            station_name,
            onid,
            tsid,
            sid,
            eid,
            comment,
            reserve_id,
            overlap_mode,
            start_time_epg,
            rec_setting,
            rec_file_name_list,
        })
    })
}

pub(crate) fn write_reserve_data(writer: &mut Writer, value: &ReserveData) {
    writer.write_struct(|writer| {
        writer.write_string(&value.title);
        writer.write_system_time(value.start_time);
        writer.write_u32(value.duration_second);
        writer.write_string(&value.station_name);
        writer.write_u16(value.onid);
        writer.write_u16(value.tsid);
        writer.write_u16(value.sid);
        writer.write_u16(value.eid);
        writer.write_string(&value.comment);
        writer.write_i32(value.reserve_id);
        writer.write_u8(0);
        writer.write_u8(value.overlap_mode);
        writer.write_string("");
        writer.write_system_time(value.start_time_epg);
        write_rec_setting_data(writer, &value.rec_setting);
        writer.write_i32(0);
        writer.write_vector(&value.rec_file_name_list, |writer, value| {
            writer.write_string(value)
        });
        writer.write_i32(0);
    });
}

pub(crate) fn read_rec_file_info(reader: &mut Reader<'_>) -> Result<RecFileInfo> {
    reader.read_struct(|reader| {
        Ok(RecFileInfo {
            id: reader.read_i32()?,
            rec_file_path: reader.read_string()?,
            title: reader.read_string()?,
            start_time: reader.read_system_time()?,
            duration_sec: reader.read_u32()?,
            service_name: reader.read_string()?,
            onid: reader.read_u16()?,
            tsid: reader.read_u16()?,
            sid: reader.read_u16()?,
            eid: reader.read_u16()?,
            drops: reader.read_i64()?,
            scrambles: reader.read_i64()?,
            rec_status: reader.read_i32()?,
            start_time_epg: reader.read_system_time()?,
            comment: reader.read_string()?,
            program_info: reader.read_string()?,
            err_info: reader.read_string()?,
            protect_flag: reader.read_u8()? != 0,
        })
    })
}

pub(crate) fn read_tuner_reserve_info(reader: &mut Reader<'_>) -> Result<TunerReserveInfo> {
    reader.read_struct(|reader| {
        Ok(TunerReserveInfo {
            tuner_id: reader.read_u32()?,
            tuner_name: reader.read_string()?,
            reserve_list: reader.read_vector(|reader| reader.read_i32())?,
        })
    })
}

pub(crate) fn read_tuner_process_status_info(
    reader: &mut Reader<'_>,
) -> Result<TunerProcessStatusInfo> {
    reader.read_struct(|reader| {
        Ok(TunerProcessStatusInfo {
            tuner_id: reader.read_u32()?,
            process_id: reader.read_i32()?,
            drop: reader.read_i64()?,
            scramble: reader.read_i64()?,
            signal_lv: f32::from_bits(reader.read_u32()?),
            space: reader.read_i32()?,
            ch: reader.read_i32()?,
            onid: reader.read_i32()?,
            tsid: reader.read_i32()?,
            rec_flag: reader.read_u8()? != 0,
            epg_cap_flag: reader.read_u8()? != 0,
            extra_flags: reader.read_u16()?,
        })
    })
}

pub(crate) fn read_service_event_info(reader: &mut Reader<'_>) -> Result<ServiceEventInfo> {
    reader.read_struct(|reader| {
        Ok(ServiceEventInfo {
            service_info: read_service_info(reader)?,
            event_list: reader.read_vector(read_event_info)?,
        })
    })
}

pub(crate) fn read_service_info(reader: &mut Reader<'_>) -> Result<ServiceInfo> {
    reader.read_struct(|reader| {
        Ok(ServiceInfo {
            onid: reader.read_u16()?,
            tsid: reader.read_u16()?,
            sid: reader.read_u16()?,
            service_type: reader.read_u8()?,
            partial_reception_flag: reader.read_u8()?,
            service_provider_name: reader.read_string()?,
            service_name: reader.read_string()?,
            network_name: reader.read_string()?,
            ts_name: reader.read_string()?,
            remote_control_key_id: reader.read_u8()?,
        })
    })
}

pub(crate) fn read_event_info(reader: &mut Reader<'_>) -> Result<EventInfo> {
    reader.read_struct(|reader| {
        let onid = reader.read_u16()?;
        let tsid = reader.read_u16()?;
        let sid = reader.read_u16()?;
        let eid = reader.read_u16()?;

        let start_time = if reader.read_u8()? != 0 {
            Some(reader.read_system_time()?)
        } else {
            reader.read_system_time()?;
            None
        };
        let duration_sec = if reader.read_u8()? != 0 {
            Some(reader.read_i32()?)
        } else {
            reader.read_i32()?;
            None
        };

        let short_info = read_optional_struct(reader, read_short_event_info)?;
        let ext_info = read_optional_struct(reader, read_extended_event_info)?;
        let content_info = read_optional_struct(reader, read_content_info)?;
        let component_info = read_optional_struct(reader, read_component_info)?;
        let audio_info = read_optional_struct(reader, read_audio_component_info)?;
        let event_group_info = read_optional_struct(reader, read_event_group_info)?;
        let event_relay_info = read_optional_struct(reader, read_event_group_info)?;
        let free_ca_flag = reader.read_u8()?;

        Ok(EventInfo {
            onid,
            tsid,
            sid,
            eid,
            free_ca_flag,
            start_time,
            duration_sec,
            short_info,
            ext_info,
            content_info,
            component_info,
            audio_info,
            event_group_info,
            event_relay_info,
        })
    })
}

fn read_optional_struct<T>(
    reader: &mut Reader<'_>,
    read: impl FnOnce(&mut Reader<'_>) -> Result<T>,
) -> Result<Option<T>> {
    if reader.peek_i32()? == 4 {
        reader.read_i32()?;
        Ok(None)
    } else {
        read(reader).map(Some)
    }
}

fn read_short_event_info(reader: &mut Reader<'_>) -> Result<ShortEventInfo> {
    reader.read_struct(|reader| {
        Ok(ShortEventInfo {
            event_name: reader.read_string()?,
            text_char: reader.read_string()?,
        })
    })
}

fn read_extended_event_info(reader: &mut Reader<'_>) -> Result<ExtendedEventInfo> {
    reader.read_struct(|reader| {
        Ok(ExtendedEventInfo {
            text_char: reader.read_string()?,
        })
    })
}

fn read_content_info(reader: &mut Reader<'_>) -> Result<ContentInfo> {
    reader.read_struct(|reader| {
        Ok(ContentInfo {
            nibble_list: reader.read_vector(read_content_data)?,
        })
    })
}

fn read_content_data(reader: &mut Reader<'_>) -> Result<ContentData> {
    reader.read_struct(|reader| {
        Ok(ContentData {
            content_nibble: reader.read_u16()?.rotate_left(8),
            user_nibble: reader.read_u16()?.rotate_left(8),
        })
    })
}

fn read_component_info(reader: &mut Reader<'_>) -> Result<ComponentInfo> {
    reader.read_struct(|reader| {
        Ok(ComponentInfo {
            stream_content: reader.read_u8()?,
            component_type: reader.read_u8()?,
            component_tag: reader.read_u8()?,
            text_char: reader.read_string()?,
        })
    })
}

fn read_audio_component_info(reader: &mut Reader<'_>) -> Result<AudioComponentInfo> {
    reader.read_struct(|reader| {
        Ok(AudioComponentInfo {
            component_list: reader.read_vector(read_audio_component_info_data)?,
        })
    })
}

fn read_audio_component_info_data(reader: &mut Reader<'_>) -> Result<AudioComponentInfoData> {
    reader.read_struct(|reader| {
        Ok(AudioComponentInfoData {
            stream_content: reader.read_u8()?,
            component_type: reader.read_u8()?,
            component_tag: reader.read_u8()?,
            stream_type: reader.read_u8()?,
            simulcast_group_tag: reader.read_u8()?,
            es_multi_lingual_flag: reader.read_u8()?,
            main_component_flag: reader.read_u8()?,
            quality_indicator: reader.read_u8()?,
            sampling_rate: reader.read_u8()?,
            text_char: reader.read_string()?,
        })
    })
}

fn read_event_group_info(reader: &mut Reader<'_>) -> Result<EventGroupInfo> {
    reader.read_struct(|reader| {
        Ok(EventGroupInfo {
            group_type: reader.read_u8()?,
            event_data_list: reader.read_vector(read_event_data)?,
        })
    })
}

fn read_event_data(reader: &mut Reader<'_>) -> Result<EventData> {
    reader.read_struct(|reader| {
        Ok(EventData {
            onid: reader.read_u16()?,
            tsid: reader.read_u16()?,
            sid: reader.read_u16()?,
            eid: reader.read_u16()?,
        })
    })
}

fn read_search_date_info(reader: &mut Reader<'_>) -> Result<SearchDateInfo> {
    reader.read_struct(|reader| {
        Ok(SearchDateInfo {
            start_day_of_week: reader.read_u8()?,
            start_hour: reader.read_u16()?,
            start_min: reader.read_u16()?,
            end_day_of_week: reader.read_u8()?,
            end_hour: reader.read_u16()?,
            end_min: reader.read_u16()?,
        })
    })
}

fn read_search_key_info(reader: &mut Reader<'_>) -> Result<SearchKeyInfo> {
    reader.read_struct(|reader| {
        let mut and_key = reader.read_string()?;
        let key_disabled = and_key.starts_with("^!{999}");
        if key_disabled {
            and_key = and_key.trim_start_matches("^!{999}").to_string();
        }
        let case_sensitive = and_key.starts_with("C!{999}");
        if case_sensitive {
            and_key = and_key.trim_start_matches("C!{999}").to_string();
        }

        let (and_key, chk_duration_min, chk_duration_max) = parse_duration_prefix(and_key.as_str());

        let not_key = reader.read_string()?;
        let reg_exp_flag = reader.read_i32()? != 0;
        let title_only_flag = reader.read_i32()? != 0;
        let content_list = reader.read_vector(read_content_data)?;
        let date_list = reader.read_vector(read_search_date_info)?;
        let service_list = reader.read_vector(|reader| reader.read_i64())?;
        let video_list = reader.read_vector(|reader| reader.read_u16())?;
        let audio_list = reader.read_vector(|reader| reader.read_u16())?;
        let aimai_flag = reader.read_u8()? != 0;
        let not_contet_flag = reader.read_u8()? != 0;
        let not_date_flag = reader.read_u8()? != 0;
        let free_ca_flag = reader.read_u8()?;
        let chk_rec_end = reader.read_u8()? != 0;
        let chk_rec_day = reader.read_u16()?;

        Ok(SearchKeyInfo {
            and_key,
            not_key,
            key_disabled,
            case_sensitive,
            reg_exp_flag,
            title_only_flag,
            content_list,
            date_list,
            service_list,
            video_list,
            audio_list,
            aimai_flag,
            not_contet_flag,
            not_date_flag,
            free_ca_flag,
            chk_rec_end,
            chk_rec_day: if chk_rec_day >= 40000 {
                chk_rec_day % 10000
            } else {
                chk_rec_day
            },
            chk_rec_no_service: chk_rec_day >= 40000,
            chk_duration_min,
            chk_duration_max,
        })
    })
}

fn parse_duration_prefix(value: &str) -> (String, u16, u16) {
    if value.len() >= 13
        && value.starts_with("D!{1")
        && value.as_bytes()[12] == b'}'
        && value.as_bytes()[4..12]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        let duration = value[3..12].parse::<u32>().unwrap_or(0);
        (
            value[13..].to_string(),
            ((duration / 10000) % 10000) as u16,
            (duration % 10000) as u16,
        )
    } else {
        (value.to_string(), 0, 0)
    }
}

pub(crate) fn read_auto_add_data(reader: &mut Reader<'_>) -> Result<AutoAddData> {
    reader.read_struct(|reader| {
        Ok(AutoAddData {
            data_id: reader.read_i32()?,
            search_info: read_search_key_info(reader)?,
            rec_setting: read_rec_setting_data(reader)?,
            add_count: reader.read_i32()?,
        })
    })
}

pub(crate) fn write_auto_add_data(writer: &mut Writer, value: &AutoAddData) {
    writer.write_struct(|writer| {
        writer.write_i32(value.data_id);
        write_search_key_info2(writer, &value.search_info);
        write_rec_setting_data(writer, &value.rec_setting);
        writer.write_i32(value.add_count);
    });
}

pub(crate) fn read_manual_auto_add_data(reader: &mut Reader<'_>) -> Result<ManualAutoAddData> {
    reader.read_struct(|reader| {
        Ok(ManualAutoAddData {
            data_id: reader.read_i32()?,
            day_of_week_flag: reader.read_u8()?,
            start_time: reader.read_u32()?,
            duration_second: reader.read_u32()?,
            title: reader.read_string()?,
            station_name: reader.read_string()?,
            onid: reader.read_u16()?,
            tsid: reader.read_u16()?,
            sid: reader.read_u16()?,
            rec_setting: read_rec_setting_data(reader)?,
        })
    })
}

pub(crate) fn read_nw_play_time_shift_info(reader: &mut Reader<'_>) -> Result<NwPlayTimeShiftInfo> {
    reader.read_struct(|reader| {
        Ok(NwPlayTimeShiftInfo {
            ctrl_id: reader.read_i32()?,
            file_path: reader.read_string()?,
        })
    })
}

pub(crate) fn read_notify_srv_info(reader: &mut Reader<'_>) -> Result<NotifySrvInfo> {
    reader.read_struct(|reader| {
        Ok(NotifySrvInfo {
            notify_id: reader.read_u32()?,
            time: reader.read_system_time()?,
            param1: reader.read_u32()?,
            param2: reader.read_u32()?,
            count: reader.read_u32()?,
            param4: reader.read_string()?,
            param5: reader.read_string()?,
            param6: reader.read_string()?,
        })
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, FixedOffset, TimeZone, Timelike};

    use super::{Reader, Writer};
    use crate::EdcbError;

    #[test]
    fn writes_and_reads_little_endian_primitives() {
        let mut writer = Writer::new();
        writer.write_u8(0x12);
        writer.write_u16(0x3456);
        writer.write_i32(-20);
        writer.write_u32(0x89abcdef);
        writer.write_i64(-30);

        assert_eq!(
            writer.as_slice(),
            &[
                0x12, 0x56, 0x34, 0xec, 0xff, 0xff, 0xff, 0xef, 0xcd, 0xab, 0x89, 0xe2, 0xff, 0xff,
                0xff, 0xff, 0xff, 0xff, 0xff,
            ]
        );

        let bytes = writer.into_inner();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.read_u8().unwrap(), 0x12);
        assert_eq!(reader.read_u16().unwrap(), 0x3456);
        assert_eq!(reader.read_i32().unwrap(), -20);
        assert_eq!(reader.read_u32().unwrap(), 0x89abcdef);
        assert_eq!(reader.read_i64().unwrap(), -30);
        assert!(reader.is_finished());
    }

    #[test]
    fn writes_and_reads_utf16le_size_prefixed_strings() {
        let mut writer = Writer::new();
        writer.write_string("BS朝日");

        let bytes = writer.into_inner();
        assert_eq!(i32::from_le_bytes(bytes[0..4].try_into().unwrap()), 14);
        assert_eq!(&bytes[bytes.len() - 2..], &[0, 0]);

        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.read_string().unwrap(), "BS朝日");
        assert!(reader.is_finished());
    }

    #[test]
    fn rejects_truncated_strings() {
        let bytes = [10, 0, 0, 0, b'a', 0];
        let mut reader = Reader::new(&bytes);

        assert!(matches!(reader.read_string(), Err(EdcbError::Decode(_))));
    }

    #[test]
    fn reads_vectors_with_declared_boundary() {
        let mut writer = Writer::new();
        writer.write_vector(&[10_i32, 20_i32], |writer, value| writer.write_i32(*value));

        let bytes = writer.into_inner();
        let mut reader = Reader::new(&bytes);
        let values = reader
            .read_vector(|reader| reader.read_i32())
            .expect("vector decodes");

        assert_eq!(values, vec![10, 20]);
        assert!(reader.is_finished());
    }

    #[test]
    fn writes_and_reads_jst_system_time() {
        let jst = FixedOffset::east_opt(9 * 3600).unwrap();
        let value = jst.with_ymd_and_hms(2026, 6, 29, 5, 45, 30).unwrap();

        let mut writer = Writer::new();
        writer.write_system_time(value);

        let bytes = writer.into_inner();
        let mut reader = Reader::new(&bytes);
        let decoded = reader.read_system_time().unwrap();

        assert_eq!(decoded.year(), 2026);
        assert_eq!(decoded.month(), 6);
        assert_eq!(decoded.day(), 29);
        assert_eq!(decoded.hour(), 5);
        assert_eq!(decoded.minute(), 45);
        assert_eq!(decoded.second(), 30);
        assert_eq!(decoded.offset().local_minus_utc(), 9 * 3600);
    }
}
