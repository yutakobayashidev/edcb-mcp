use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use encoding_rs::{Encoding, SHIFT_JIS, UTF_8, UTF_16LE};

use crate::types::ChSet5Item;

pub fn convert_bytes_to_string(buf: &[u8], default_encoding: &str) -> String {
    if buf.is_empty() {
        return String::new();
    }

    let (encoding, bytes) = if buf.len() >= 2 && buf[0] == 0xff && buf[1] == 0xfe {
        (UTF_16LE, &buf[2..])
    } else if buf.len() >= 3 && buf[0] == 0xef && buf[1] == 0xbb && buf[2] == 0xbf {
        (UTF_8, &buf[3..])
    } else {
        (
            Encoding::for_label(default_encoding.as_bytes()).unwrap_or(SHIFT_JIS),
            buf,
        )
    };

    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

pub fn parse_ch_set5(input: &str) -> Vec<ChSet5Item> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() < 9 {
                return None;
            }
            Some(ChSet5Item {
                service_name: fields[0].to_string(),
                network_name: fields[1].to_string(),
                onid: fields[2].parse().ok()?,
                tsid: fields[3].parse().ok()?,
                sid: fields[4].parse().ok()?,
                service_type: fields[5].parse().ok()?,
                partial_flag: fields[6].parse::<i32>().ok()? != 0,
                epg_cap_flag: fields[7].parse::<i32>().ok()? != 0,
                search_flag: fields[8].parse::<i32>().ok()? != 0,
                remocon_id: fields
                    .get(9)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
            })
        })
        .collect()
}

pub fn get_logo_id_from_logo_data_ini(input: &str, onid: u16, sid: u16) -> Option<i32> {
    let target = format!("{onid:04X}{sid:04X}");
    for line in input.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(&target) {
            return value.trim().parse().ok();
        }
    }
    None
}

pub fn get_logo_file_name_from_directory_index(
    input: &str,
    onid: u16,
    logo_id: u16,
    logo_type: u8,
) -> Option<String> {
    let target = format!("{onid:04X}_{logo_id:03X}_");
    let target_type = format!("_{logo_type:02}.");
    for line in input.lines() {
        let fields: Vec<_> = line.splitn(4, ' ').collect();
        if fields.len() != 4 {
            continue;
        }
        let name = fields[3];
        if name.len() >= 16
            && name[0..9].eq_ignore_ascii_case(&target)
            && name[12..16] == target_type
        {
            return Some(name.to_string());
        }
    }
    None
}

pub fn parse_program_extended_text(input: &str) -> BTreeMap<String, String> {
    let input = input.replace('\r', "");
    let mut sections = BTreeMap::new();
    if input.is_empty() {
        return sections;
    }

    let mut current_head = String::new();
    let mut current_body = String::new();
    let mut saw_heading = false;

    for line in input.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        if let Some(head) = line_without_newline.strip_prefix("- ") {
            if saw_heading || !current_body.is_empty() {
                insert_unique(&mut sections, current_head, current_body);
            }
            current_head = head.to_string();
            current_body = String::new();
            saw_heading = true;
        } else {
            current_body.push_str(line);
        }
    }

    if saw_heading || !current_body.is_empty() {
        insert_unique(&mut sections, current_head, current_body);
    }

    sections
}

fn insert_unique(map: &mut BTreeMap<String, String>, mut key: String, value: String) {
    while map.contains_key(&key) {
        key.push('\t');
    }
    map.insert(key, value);
}

pub fn datetime_to_file_time(value: DateTime<FixedOffset>) -> i64 {
    (value.timestamp() + i64::from(value.offset().local_minus_utc())) * 10_000_000
        + i64::from(value.timestamp_subsec_nanos() / 100)
        + 116444736000000000
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::{
        convert_bytes_to_string, datetime_to_file_time, get_logo_file_name_from_directory_index,
        get_logo_id_from_logo_data_ini, parse_ch_set5, parse_program_extended_text,
    };

    #[test]
    fn converts_bytes_using_bom_or_cp932() {
        assert_eq!(
            convert_bytes_to_string(&[0xff, 0xfe, 0x42, 0x30], "cp932"),
            "あ"
        );
        assert_eq!(
            convert_bytes_to_string(&[0x83, 0x65, 0x83, 0x58, 0x83, 0x67], "cp932"),
            "テスト"
        );
    }

    #[test]
    fn parses_ch_set5_rows() {
        let rows = parse_ch_set5("NHK\t地デジ\t1\t2\t3\t1\t0\t1\t1\nbad");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].service_name, "NHK");
        assert_eq!(rows[0].onid, 1);
        assert!(!rows[0].partial_flag);
        assert!(rows[0].epg_cap_flag);
        assert_eq!(rows[0].remocon_id, 0);
    }

    #[test]
    fn extracts_logo_metadata() {
        assert_eq!(
            get_logo_id_from_logo_data_ini("00010003=42\n", 1, 3),
            Some(42)
        );
        assert_eq!(
            get_logo_file_name_from_directory_index(
                "2026/01/01 00:00 0000 1234_02A_000_01.png\n",
                0x1234,
                0x02a,
                1,
            ),
            Some("1234_02A_000_01.png".to_string())
        );
    }

    #[test]
    fn parses_program_extended_text_sections() {
        let parsed = parse_program_extended_text("- 見出し1\n本文1\n- 見出し2\n本文2");

        assert_eq!(parsed.get("見出し1").unwrap(), "本文1\n");
        assert_eq!(parsed.get("見出し2").unwrap(), "本文2");
    }

    #[test]
    fn converts_datetime_to_filetime() {
        let utc = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
            .unwrap();

        assert_eq!(datetime_to_file_time(utc), 116444736000000000);
    }
}
