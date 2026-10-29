const SUCCESS: u8 = 0x7e;
const OUT_OF_BOUNDS: u8 = 0x81;
const OUT_OF_MEMORY: u8 = 0x82;
const NOT_LOADED: u8 = 0x83;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetEndian {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Md5Probe {
    pub md5: String,
    pub target_endian: TargetEndian,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugListResponse {
    pub tick: u32,
    pub last_index: u16,
    pub data: Vec<u8>,
}

pub fn pack_debug_addr(array: u8, element: u16) -> u32 {
    (u32::from(array) << 16) | u32::from(element)
}

pub fn build_get_md5_request() -> Vec<u8> {
    vec![0x45, 0xde, 0xad, 0x00, 0x00]
}

pub fn build_get_list_request(indexes: &[u32]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(indexes.len())
        .map_err(|_| format!("debug request has too many indexes: {}", indexes.len()))?;
    let mut output = Vec::with_capacity(3 + indexes.len() * 3);
    output.push(0x44);
    output.extend_from_slice(&count.to_be_bytes());
    for index in indexes {
        output.push(((index >> 16) & 0xff) as u8);
        output.extend_from_slice(&(*index as u16).to_be_bytes());
    }
    Ok(output)
}

pub fn build_set_variable_request(
    index: u32,
    force: bool,
    value: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let data_length = if force {
        value.map_or(0, <[u8]>::len)
    } else {
        1
    };
    if force && data_length == 0 {
        return Err("forced debug writes require a value payload".to_owned());
    }
    let data_length = u16::try_from(data_length)
        .map_err(|_| "debug write payload exceeds u16 wire length".to_owned())?;
    let mut output = Vec::with_capacity(7 + usize::from(data_length));
    output.push(0x42);
    output.push(((index >> 16) & 0xff) as u8);
    output.extend_from_slice(&(index as u16).to_be_bytes());
    output.push(u8::from(force));
    output.extend_from_slice(&data_length.to_be_bytes());
    if force {
        let Some(value) = value else {
            return Err("forced debug writes require a value payload".to_owned());
        };
        output.extend_from_slice(value);
    } else {
        output.push(0);
    }
    Ok(output)
}

pub fn parse_get_md5_response(data: &[u8]) -> Result<Md5Probe, String> {
    if data.len() < 2 {
        return Err(format!(
            "Invalid MD5 response: too short ({} bytes)",
            data.len()
        ));
    }
    if data[0] != 0x45 {
        return Err("Function code mismatch in MD5 response".to_owned());
    }
    if data[1] != SUCCESS {
        return Err(format!("MD5 request failed: {}", status_error(data[1])));
    }
    if data.len() < 4 {
        return Err(format!(
            "Invalid MD5 response: success payload too short ({} bytes)",
            data.len()
        ));
    }
    let trailer = &data[data.len() - 2..];
    let target_endian = match trailer {
        [0xad, 0xde] => TargetEndian::Little,
        [0xde, 0xad] => TargetEndian::Big,
        _ => {
            return Err(format!(
                "Invalid MD5 endianness sentinel: {:02X} {:02X}",
                trailer[0], trailer[1]
            ));
        }
    };
    let md5 = String::from_utf8(data[2..data.len() - 2].to_vec())
        .map_err(|error| format!("Invalid UTF-8 in MD5 response: {error}"))?
        .trim_end_matches('\0')
        .trim()
        .to_ascii_lowercase();
    Ok(Md5Probe { md5, target_endian })
}

pub fn parse_get_list_response(data: &[u8]) -> Result<DebugListResponse, String> {
    if data.len() < 2 {
        return Err(format!(
            "Invalid response: too short ({} bytes)",
            data.len()
        ));
    }
    if data[0] != 0x44 {
        return Err("Function code mismatch".to_owned());
    }
    if data[1] != SUCCESS {
        return Err(status_error(data[1]));
    }
    if data.len() < 10 {
        return Err(format!(
            "Incomplete success response ({} bytes, expected at least 10)",
            data.len()
        ));
    }
    let last_index = u16::from_be_bytes([data[2], data[3]]);
    let tick = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let response_size = usize::from(u16::from_be_bytes([data[8], data[9]]));
    if data.len() < 10 + response_size {
        return Err(format!(
            "Incomplete variable data (expected {response_size} bytes, got {})",
            data.len().saturating_sub(10)
        ));
    }
    Ok(DebugListResponse {
        tick,
        last_index,
        data: data[10..10 + response_size].to_vec(),
    })
}

pub fn parse_set_variable_response(data: &[u8]) -> Result<(), String> {
    if data.len() < 2 {
        return Err(format!(
            "Invalid response: too short ({} bytes)",
            data.len()
        ));
    }
    if data[0] != 0x42 {
        return Err("Function code mismatch".to_owned());
    }
    if data[1] != SUCCESS {
        return Err(status_error(data[1]));
    }
    Ok(())
}

fn status_error(code: u8) -> String {
    match code {
        OUT_OF_BOUNDS => "ERROR_OUT_OF_BOUNDS".to_owned(),
        OUT_OF_MEMORY => "ERROR_OUT_OF_MEMORY".to_owned(),
        NOT_LOADED => "ERROR_NOT_LOADED".to_owned(),
        _ => format!("Unknown error code: 0x{code:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_get_list_request, build_get_md5_request, build_set_variable_request, pack_debug_addr,
        parse_get_list_response, parse_get_md5_response, parse_set_variable_response, TargetEndian,
    };

    #[test]
    fn builders_match_pinned_modbus_pdu_bytes() {
        assert_eq!(build_get_md5_request(), vec![0x45, 0xde, 0xad, 0, 0]);
        assert_eq!(
            build_get_list_request(&[pack_debug_addr(2, 0x1234), pack_debug_addr(0xff, 7)])
                .unwrap(),
            vec![0x44, 0, 2, 2, 0x12, 0x34, 0xff, 0, 7]
        );
        assert_eq!(
            build_set_variable_request(pack_debug_addr(3, 0x4567), true, Some(&[0xaa, 0xbb]))
                .unwrap(),
            vec![0x42, 3, 0x45, 0x67, 1, 0, 2, 0xaa, 0xbb]
        );
        assert_eq!(
            build_set_variable_request(pack_debug_addr(0, 1), false, None).unwrap(),
            vec![0x42, 0, 0, 1, 0, 0, 1, 0]
        );
    }

    #[test]
    fn response_parsers_match_pinned_wire_contract() {
        let mut md5 = vec![0x45, 0x7e];
        md5.extend_from_slice(b"0123456789abcdef0123456789abcdef");
        md5.extend_from_slice(&[0xad, 0xde]);
        let probe = parse_get_md5_response(&md5).unwrap();
        assert_eq!(probe.md5, "0123456789abcdef0123456789abcdef");
        assert_eq!(probe.target_endian, TargetEndian::Little);

        let list = vec![0x44, 0x7e, 0, 4, 0, 0, 0, 9, 0, 3, 1, 2, 3];
        let parsed = parse_get_list_response(&list).unwrap();
        assert_eq!(parsed.last_index, 4);
        assert_eq!(parsed.tick, 9);
        assert_eq!(parsed.data, vec![1, 2, 3]);
        assert!(parse_set_variable_response(&[0x42, 0x7e]).is_ok());
        assert_eq!(
            parse_set_variable_response(&[0x42, 0x81]).unwrap_err(),
            "ERROR_OUT_OF_BOUNDS"
        );
    }
}
