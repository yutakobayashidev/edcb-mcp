use std::collections::BTreeMap;

use crate::error::{EdcbError, Result};
use crate::types::{
    PostRecordingMode, RecFileSetInfo, RecSettingData, RecordSettings,
    RecordSettingsGlobalDefaults, RecordSettingsPreset, RecordSettingsPresets, RecordingFolder,
    RecordingMode, ServiceRecordingMode,
};

pub(crate) fn record_settings_from_rec_setting(
    rec_setting: &RecSettingData,
) -> Result<RecordSettings> {
    let (caption, data) = service_recording_modes(rec_setting.service_mode);
    Ok(RecordSettings {
        is_enabled: rec_setting.rec_mode <= 4,
        priority: rec_setting.priority,
        recording_folders: recording_folders_from_rec_file_sets(
            &rec_setting.rec_folder_list,
            &rec_setting.partial_rec_folder,
        ),
        recording_start_margin: rec_setting.start_margin,
        recording_end_margin: rec_setting.end_margin,
        recording_mode: recording_mode_from_rec_mode(rec_setting.rec_mode)?,
        caption_recording_mode: caption,
        data_broadcasting_recording_mode: data,
        post_recording_mode: post_recording_mode_from_suspend(
            rec_setting.suspend_mode,
            rec_setting.reboot_flag,
        ),
        post_recording_bat_file_path: non_empty_string(&rec_setting.bat_file_path),
        is_event_relay_follow_enabled: rec_setting.tuijyuu_flag,
        is_exact_recording_enabled: rec_setting.pittari_flag,
        is_oneseg_separate_output_enabled: rec_setting.partial_rec_flag == 1,
        is_sequential_recording_in_single_file_enabled: rec_setting.continue_rec_flag,
        forced_tuner_id: (rec_setting.tuner_id != 0).then_some(rec_setting.tuner_id),
    })
}

pub(crate) fn parse_recording_presets_ini(input: &str) -> Result<RecordSettingsPresets> {
    let ini = EdcbIni::parse(input);
    let global_defaults = parse_global_defaults(&ini)?;
    let mut presets = vec![parse_preset(&ini, 0)?];

    for id in ini
        .get("SET", "PresetID")
        .unwrap_or("")
        .split(',')
        .filter_map(|value| value.trim().parse::<i32>().ok())
        .filter(|id| *id != 0)
    {
        if let Ok(preset) = parse_preset(&ini, id) {
            presets.push(preset);
        }
    }

    Ok(RecordSettingsPresets {
        global_defaults,
        presets,
    })
}

pub(crate) fn recording_mode_from_rec_mode(rec_mode: u8) -> Result<RecordingMode> {
    match rec_mode {
        0 | 9 => Ok(RecordingMode::AllServices),
        1 | 5 => Ok(RecordingMode::SpecifiedService),
        2 | 6 => Ok(RecordingMode::AllServicesWithoutDecoding),
        3 | 7 => Ok(RecordingMode::SpecifiedServiceWithoutDecoding),
        4 | 8 => Ok(RecordingMode::View),
        _ => Err(EdcbError::InvalidInput(format!(
            "unsupported EDCB rec_mode: {rec_mode}"
        ))),
    }
}

pub(crate) fn rec_mode_value(enabled: bool, mode: RecordingMode) -> u8 {
    match (enabled, mode) {
        (true, RecordingMode::AllServices) => 0,
        (true, RecordingMode::SpecifiedService) => 1,
        (true, RecordingMode::AllServicesWithoutDecoding) => 2,
        (true, RecordingMode::SpecifiedServiceWithoutDecoding) => 3,
        (true, RecordingMode::View) => 4,
        (false, RecordingMode::AllServices) => 9,
        (false, RecordingMode::SpecifiedService) => 5,
        (false, RecordingMode::AllServicesWithoutDecoding) => 6,
        (false, RecordingMode::SpecifiedServiceWithoutDecoding) => 7,
        (false, RecordingMode::View) => 8,
    }
}

pub(crate) fn service_recording_modes(
    service_mode: u32,
) -> (ServiceRecordingMode, ServiceRecordingMode) {
    if service_mode & 0x0000_0001 == 0 {
        return (ServiceRecordingMode::Default, ServiceRecordingMode::Default);
    }
    let caption = if service_mode & 0x0000_0010 != 0 {
        ServiceRecordingMode::Enable
    } else {
        ServiceRecordingMode::Disable
    };
    let data = if service_mode & 0x0000_0020 != 0 {
        ServiceRecordingMode::Enable
    } else {
        ServiceRecordingMode::Disable
    };
    (caption, data)
}

pub(crate) fn service_mode_value(
    caption: ServiceRecordingMode,
    data: ServiceRecordingMode,
) -> Result<u32> {
    let caption_default = caption == ServiceRecordingMode::Default;
    let data_default = data == ServiceRecordingMode::Default;
    if caption_default != data_default {
        return Err(EdcbError::InvalidInput(
            "caption and data recording modes must both be Default or both be explicit".to_string(),
        ));
    }
    if caption_default {
        return Ok(0);
    }
    let mut service_mode = 0x0000_0001;
    if caption == ServiceRecordingMode::Enable {
        service_mode |= 0x0000_0010;
    }
    if data == ServiceRecordingMode::Enable {
        service_mode |= 0x0000_0020;
    }
    Ok(service_mode)
}

pub(crate) fn suspend_mode_value(mode: PostRecordingMode) -> (u8, bool) {
    match mode {
        PostRecordingMode::Default => (0, false),
        PostRecordingMode::Standby => (1, false),
        PostRecordingMode::StandbyAndReboot => (1, true),
        PostRecordingMode::Suspend => (2, false),
        PostRecordingMode::SuspendAndReboot => (2, true),
        PostRecordingMode::Shutdown => (3, false),
        PostRecordingMode::Nothing => (4, false),
    }
}

pub(crate) fn rec_file_set_lists(
    folders: &[RecordingFolder],
) -> (Vec<RecFileSetInfo>, Vec<RecFileSetInfo>) {
    let mut normal = Vec::new();
    let mut partial = Vec::new();
    for folder in folders {
        let info = RecFileSetInfo {
            rec_folder: folder.recording_folder_path.clone(),
            write_plug_in: "Write_Default.dll".to_string(),
            rec_name_plug_in: folder
                .recording_file_name_template
                .as_ref()
                .filter(|template| !template.is_empty())
                .map(|template| format!("RecName_Macro.dll?{template}"))
                .unwrap_or_else(|| "RecName_Macro.dll".to_string()),
        };
        if folder.is_oneseg_separate_recording_folder {
            partial.push(info);
        } else {
            normal.push(info);
        }
    }
    (normal, partial)
}

fn parse_global_defaults(ini: &EdcbIni) -> Result<RecordSettingsGlobalDefaults> {
    let rec_end_mode = ini.get_i32("SET", "RecEndMode", 2)?;
    let reboot = ini.get_i32("SET", "Reboot", 0)? != 0;
    Ok(RecordSettingsGlobalDefaults {
        recording_start_margin: ini.get_i32("SET", "StartMargin", 5)?,
        recording_end_margin: ini.get_i32("SET", "EndMargin", 2)?,
        caption_recording_mode: if ini.get_i32("SET", "Caption", 1)? != 0 {
            ServiceRecordingMode::Enable
        } else {
            ServiceRecordingMode::Disable
        },
        data_broadcasting_recording_mode: if ini.get_i32("SET", "Data", 0)? != 0 {
            ServiceRecordingMode::Enable
        } else {
            ServiceRecordingMode::Disable
        },
        post_recording_mode: global_post_recording_mode(rec_end_mode, reboot),
    })
}

fn parse_preset(ini: &EdcbIni, preset_id: i32) -> Result<RecordSettingsPreset> {
    let suffix = if preset_id == 0 {
        String::new()
    } else {
        preset_id.to_string()
    };
    let section = format!("REC_DEF{suffix}");
    let raw_rec_mode = ini.get_u8(&section, "RecMode", 1)?;
    let no_rec_mode = ini.get_u8(&section, "NoRecMode", 1)?;
    let is_enabled = raw_rec_mode <= 4;
    let effective_rec_mode = if is_enabled {
        raw_rec_mode
    } else {
        no_rec_mode
    };
    let service_mode = ini.get_u32(&section, "ServiceMode", 0)?;
    let (caption, data) = service_recording_modes(service_mode);
    let use_margin = ini.get_i32(&section, "UseMargineFlag", 0)? != 0;

    Ok(RecordSettingsPreset {
        id: preset_id,
        name: ini
            .get(&section, "SetName")
            .unwrap_or("Default")
            .to_string(),
        record_settings: RecordSettings {
            is_enabled,
            priority: ini.get_u8(&section, "Priority", 2)?.clamp(1, 5),
            recording_folders: parse_recording_folders(ini, &suffix)?,
            recording_start_margin: use_margin
                .then(|| ini.get_i32(&section, "StartMargine", 5))
                .transpose()?,
            recording_end_margin: use_margin
                .then(|| ini.get_i32(&section, "EndMargine", 2))
                .transpose()?,
            recording_mode: recording_mode_from_rec_mode(effective_rec_mode)?,
            caption_recording_mode: caption,
            data_broadcasting_recording_mode: data,
            post_recording_mode: post_recording_mode_from_suspend(
                ini.get_u8(&section, "SuspendMode", 0)?,
                ini.get_i32(&section, "RebootFlag", 0)? != 0,
            ),
            post_recording_bat_file_path: ini.get(&section, "BatFilePath").and_then(non_empty_str),
            is_event_relay_follow_enabled: ini.get_i32(&section, "TuijyuuFlag", 1)? != 0,
            is_exact_recording_enabled: ini.get_i32(&section, "PittariFlag", 0)? != 0,
            is_oneseg_separate_output_enabled: ini.get_i32(&section, "PartialRec", 0)? == 1,
            is_sequential_recording_in_single_file_enabled: ini.get_i32(
                &section,
                "ContinueRec",
                0,
            )? != 0,
            forced_tuner_id: non_zero_u32(ini.get_u32(&section, "TunerID", 0)?),
        },
    })
}

fn parse_recording_folders(ini: &EdcbIni, suffix: &str) -> Result<Vec<RecordingFolder>> {
    let mut folders = parse_folder_section(ini, &format!("REC_DEF_FOLDER{suffix}"), false)?;
    folders.extend(parse_folder_section(
        ini,
        &format!("REC_DEF_FOLDER_1SEG{suffix}"),
        true,
    )?);
    Ok(folders)
}

fn parse_folder_section(
    ini: &EdcbIni,
    section: &str,
    is_oneseg: bool,
) -> Result<Vec<RecordingFolder>> {
    let mut folders = Vec::new();
    for index in 0..ini.get_i32(section, "Count", 0)? {
        let Some(path) = ini.get(section, &index.to_string()).and_then(non_empty_str) else {
            continue;
        };
        let template = ini
            .get(section, &format!("RecNamePlugIn{index}"))
            .and_then(template_from_rec_name_plugin);
        folders.push(RecordingFolder {
            recording_folder_path: path,
            recording_file_name_template: template,
            is_oneseg_separate_recording_folder: is_oneseg,
        });
    }
    Ok(folders)
}

fn recording_folders_from_rec_file_sets(
    normal: &[RecFileSetInfo],
    partial: &[RecFileSetInfo],
) -> Vec<RecordingFolder> {
    normal
        .iter()
        .map(|folder| recording_folder_from_rec_file_set(folder, false))
        .chain(
            partial
                .iter()
                .map(|folder| recording_folder_from_rec_file_set(folder, true)),
        )
        .collect()
}

fn recording_folder_from_rec_file_set(folder: &RecFileSetInfo, is_oneseg: bool) -> RecordingFolder {
    RecordingFolder {
        recording_folder_path: folder.rec_folder.clone(),
        recording_file_name_template: template_from_rec_name_plugin(&folder.rec_name_plug_in),
        is_oneseg_separate_recording_folder: is_oneseg,
    }
}

fn global_post_recording_mode(rec_end_mode: i32, reboot: bool) -> PostRecordingMode {
    match (rec_end_mode, reboot) {
        (1, false) => PostRecordingMode::Standby,
        (1, true) => PostRecordingMode::StandbyAndReboot,
        (2, false) => PostRecordingMode::Suspend,
        (2, true) => PostRecordingMode::SuspendAndReboot,
        (3, _) => PostRecordingMode::Shutdown,
        _ => PostRecordingMode::Nothing,
    }
}

fn post_recording_mode_from_suspend(suspend_mode: u8, reboot: bool) -> PostRecordingMode {
    match (suspend_mode, reboot) {
        (0, _) => PostRecordingMode::Default,
        (1, false) => PostRecordingMode::Standby,
        (1, true) => PostRecordingMode::StandbyAndReboot,
        (2, false) => PostRecordingMode::Suspend,
        (2, true) => PostRecordingMode::SuspendAndReboot,
        (3, _) => PostRecordingMode::Shutdown,
        (4, _) => PostRecordingMode::Nothing,
        _ => PostRecordingMode::Default,
    }
}

fn template_from_rec_name_plugin(value: &str) -> Option<String> {
    value
        .split_once('?')
        .and_then(|(_, template)| non_empty_str(template))
}

fn non_empty_string(value: &str) -> Option<String> {
    non_empty_str(value)
}

fn non_empty_str(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn non_zero_u32(value: u32) -> Option<u32> {
    (value != 0).then_some(value)
}

#[derive(Debug, Default)]
struct EdcbIni {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl EdcbIni {
    fn parse(input: &str) -> Self {
        let mut ini = Self::default();
        let mut current_section = String::new();
        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(section) = line
                .strip_prefix('[')
                .and_then(|line| line.strip_suffix(']'))
            {
                current_section = section.trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            ini.sections
                .entry(current_section.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
        ini
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(section)
            .and_then(|section| section.get(key))
            .map(String::as_str)
    }

    fn get_i32(&self, section: &str, key: &str, default: i32) -> Result<i32> {
        self.get(section, key)
            .map(|value| {
                value.parse().map_err(|_| {
                    EdcbError::InvalidInput(format!("{section}.{key} must be i32: {value}"))
                })
            })
            .unwrap_or(Ok(default))
    }

    fn get_u8(&self, section: &str, key: &str, default: u8) -> Result<u8> {
        self.get(section, key)
            .map(|value| {
                value.parse().map_err(|_| {
                    EdcbError::InvalidInput(format!("{section}.{key} must be u8: {value}"))
                })
            })
            .unwrap_or(Ok(default))
    }

    fn get_u32(&self, section: &str, key: &str, default: u32) -> Result<u32> {
        self.get(section, key)
            .map(|value| {
                value.parse().map_err(|_| {
                    EdcbError::InvalidInput(format!("{section}.{key} must be u32: {value}"))
                })
            })
            .unwrap_or(Ok(default))
    }
}
