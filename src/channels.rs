use std::collections::HashMap;

use crate::types::{ChSet5Item, Channel, ChannelType, ServiceInfo, ServiceKey};

pub(crate) fn channels_from_sources(
    chset_services: Vec<ChSet5Item>,
    epg_services: &[ServiceInfo],
) -> Vec<Channel> {
    let epg_remocon_by_service = epg_services
        .iter()
        .map(|service| {
            (
                (service.onid, service.tsid, service.sid),
                u16::from(service.remote_control_key_id),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut services = chset_services
        .into_iter()
        .filter(is_supported_service)
        .filter_map(|service| {
            let channel_type = channel_type_from_onid(service.onid)?;
            Some((service, channel_type))
        })
        .collect::<Vec<_>>();
    services.sort_by_key(|(service, _)| {
        (
            service.onid,
            service.sid,
            service.tsid,
            service.service_name.clone(),
        )
    });

    let mut same_network_counts = HashMap::<u16, u16>::new();
    let mut same_remocon_counts = HashMap::<u16, i32>::new();
    let mut channels = Vec::new();

    for (service, channel_type) in services {
        let network_count = same_network_counts.entry(service.onid).or_insert(0);
        *network_count += 1;
        let remocon_id = remocon_id(&service, channel_type, &epg_remocon_by_service);
        let channel_number = channel_number(
            channel_type,
            service.onid,
            service.sid,
            remocon_id,
            *network_count,
            &mut same_remocon_counts,
        );
        let display_channel_id = format!(
            "{}{}",
            channel_type_slug(channel_type),
            channel_number.as_str()
        );
        let is_subchannel = is_subchannel(channel_type, service.sid);
        let is_radiochannel = service.service_type == 0x02;
        let is_watchable = is_watchable(channel_type, service.sid, &service.service_name);

        channels.push(Channel {
            id: format!("NID{}-SID{:03}", service.onid, service.sid),
            display_channel_id,
            service_key: ServiceKey {
                onid: service.onid,
                tsid: service.tsid,
                sid: service.sid,
            },
            network_id: service.onid,
            transport_stream_id: service.tsid,
            service_id: service.sid,
            remocon_id,
            channel_number,
            channel_type,
            name: service.service_name,
            is_subchannel,
            is_radiochannel,
            is_watchable,
        });
    }

    channels.sort_by_key(|channel| {
        (
            channel_type_order(channel.channel_type),
            channel.remocon_id,
            channel.channel_number.clone(),
            channel.service_id,
        )
    });
    channels
}

pub(crate) fn chset_from_services(services: Vec<ServiceInfo>) -> Vec<ChSet5Item> {
    services
        .into_iter()
        .map(|service| ChSet5Item {
            service_name: service.service_name,
            network_name: service.network_name,
            onid: service.onid,
            tsid: service.tsid,
            sid: service.sid,
            service_type: service.service_type,
            partial_flag: service.partial_reception_flag != 0,
            epg_cap_flag: true,
            search_flag: true,
            remocon_id: u16::from(service.remote_control_key_id),
        })
        .collect()
}

fn is_supported_service(service: &ChSet5Item) -> bool {
    matches!(service.service_type, 0x01 | 0x02 | 0xa1 | 0xa2 | 0xad)
}

fn channel_type_from_onid(onid: u16) -> Option<ChannelType> {
    match onid {
        0x7880..=0x7fe8 => Some(ChannelType::Gr),
        0x0004 => Some(ChannelType::Bs),
        0x0006 | 0x0007 => Some(ChannelType::Cs),
        0xfffe | 0xfffa | 0xfffd | 0xfff9 | 0xfff7 => Some(ChannelType::Catv),
        0x000a | 0x0001 | 0x0003 => Some(ChannelType::Sky),
        0x000b | 0x000c => Some(ChannelType::Bs4k),
        _ => None,
    }
}

fn remocon_id(
    service: &ChSet5Item,
    channel_type: ChannelType,
    epg_remocon_by_service: &HashMap<(u16, u16, u16), u16>,
) -> u16 {
    if channel_type == ChannelType::Gr {
        let epg_remocon = epg_remocon_by_service
            .get(&(service.onid, service.tsid, service.sid))
            .copied()
            .unwrap_or(0);
        if epg_remocon != 0 {
            return epg_remocon;
        }
        if service.remocon_id != 0 {
            return service.remocon_id;
        }
    }
    calculate_remocon_id(channel_type, service.sid)
}

fn calculate_remocon_id(channel_type: ChannelType, sid: u16) -> u16 {
    if channel_type == ChannelType::Bs {
        match sid {
            101..=102 => 1,
            103..=104 => 3,
            141..=149 => 4,
            151..=159 => 5,
            161..=169 => 6,
            171..=179 => 7,
            181..=189 => 8,
            191..=193 => 9,
            200..=202 => 10,
            211 => 11,
            222 => 12,
            _ => sid,
        }
    } else if channel_type == ChannelType::Sky {
        sid % 1024
    } else {
        sid
    }
}

fn channel_number(
    channel_type: ChannelType,
    onid: u16,
    sid: u16,
    remocon_id: u16,
    same_network_count: u16,
    same_remocon_counts: &mut HashMap<u16, i32>,
) -> String {
    if channel_type == ChannelType::Gr {
        let remocon_count = same_remocon_counts.entry(remocon_id).or_insert(-1);
        if same_network_count == 1 {
            *remocon_count += 1;
        }
        let mut number = format!("{remocon_id:02}{same_network_count}");
        if *remocon_count > 0 {
            number.push_str(&format!("-{remocon_count}"));
        }
        return number;
    }
    if channel_type == ChannelType::Sky {
        return format!("{:03}", sid % 1024);
    }
    let _ = onid;
    format!("{sid:03}")
}

fn is_subchannel(channel_type: ChannelType, sid: u16) -> bool {
    match channel_type {
        ChannelType::Gr => sid & 0x0187 != 0,
        ChannelType::Bs => {
            matches!(sid, 102 | 104 | 232 | 233)
                || (142..=149).contains(&sid)
                || (152..=159).contains(&sid)
                || (162..=169).contains(&sid)
                || (172..=179).contains(&sid)
                || (182..=189).contains(&sid)
        }
        _ => false,
    }
}

fn is_watchable(channel_type: ChannelType, sid: u16, name: &str) -> bool {
    if channel_type == ChannelType::Bs && matches!(sid, 103 | 104 | 238 | 241 | 258 | 263) {
        return false;
    }
    !name.starts_with("試験チャンネル")
}

fn channel_type_slug(channel_type: ChannelType) -> &'static str {
    match channel_type {
        ChannelType::Gr => "gr",
        ChannelType::Bs => "bs",
        ChannelType::Cs => "cs",
        ChannelType::Catv => "catv",
        ChannelType::Sky => "sky",
        ChannelType::Bs4k => "bs4k",
    }
}

fn channel_type_order(channel_type: ChannelType) -> u8 {
    match channel_type {
        ChannelType::Gr => 0,
        ChannelType::Bs => 1,
        ChannelType::Cs => 2,
        ChannelType::Catv => 3,
        ChannelType::Sky => 4,
        ChannelType::Bs4k => 5,
    }
}
