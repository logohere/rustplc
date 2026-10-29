use super::{
    DebugProvider, MAX_FRAME, MAX_INDEXES, NOT_LOADED, OUT_OF_BOUNDS, OUT_OF_MEMORY, SUCCESS,
};
use crate::debug_write::{DebugWriteQueue, QueueError, WriteOp};

fn short(code: u8, status: u8) -> Vec<u8> {
    vec![code, status]
}

fn valid_array(provider: &dyn DebugProvider, array: u8) -> bool {
    array < provider.array_count()
}

fn be16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn put16(output: &mut [u8], value: u16) {
    output.copy_from_slice(&value.to_be_bytes());
}

fn header(code: u8, last: u16, tick: u32, size: usize) -> Vec<u8> {
    let mut output = vec![code, SUCCESS, 0, 0, 0, 0, 0, 0, 0, 0];
    put16(&mut output[2..4], last);
    output[4..8].copy_from_slice(&tick.to_be_bytes());
    put16(&mut output[8..10], size as u16);
    output
}

pub(super) fn process(
    frame: &[u8],
    provider: Option<&dyn DebugProvider>,
    writes: &DebugWriteQueue,
    tick: u32,
) -> Vec<u8> {
    let Some(&code) = frame.first() else {
        return Vec::new();
    };
    let known = matches!(code, 0x41..=0x45);
    let Some(provider) = provider else {
        return if known {
            short(code, NOT_LOADED)
        } else {
            Vec::new()
        };
    };
    match code {
        0x41 => info(provider),
        0x42 => set(frame, provider, writes),
        0x43 => get(frame, provider, tick),
        0x44 => get_list(frame, provider, tick),
        0x45 => md5(frame, provider),
        _ => Vec::new(),
    }
}

fn info(provider: &dyn DebugProvider) -> Vec<u8> {
    let count = usize::from(provider.array_count()).min((MAX_FRAME - 3) / 2);
    let mut output = vec![0x41, count as u8, SUCCESS];
    for array in 0..count as u8 {
        output.extend_from_slice(&provider.elem_count(array).to_be_bytes());
    }
    output
}

fn set(frame: &[u8], provider: &dyn DebugProvider, writes: &DebugWriteQueue) -> Vec<u8> {
    if frame.len() < 7 {
        return short(0x42, OUT_OF_BOUNDS);
    }
    let array = frame[1];
    let element = be16(&frame[2..4]);
    let forcing = frame[4] != 0;
    let len = usize::from(be16(&frame[5..7]));
    if !valid_array(provider, array) || len > MAX_FRAME - 7 || frame.len() < len + 7 {
        return short(0x42, OUT_OF_BOUNDS);
    }
    let op = if forcing {
        WriteOp::Force
    } else {
        WriteOp::Unforce
    };
    let value = if forcing { &frame[7..7 + len] } else { &[] };
    let status = match writes.enqueue(array, element, op, value) {
        Ok(()) => SUCCESS,
        Err(QueueError::Full | QueueError::Poisoned) => OUT_OF_MEMORY,
    };
    short(0x42, status)
}

fn get(frame: &[u8], provider: &dyn DebugProvider, tick: u32) -> Vec<u8> {
    if frame.len() < 6 {
        return short(0x43, OUT_OF_BOUNDS);
    }
    let array = frame[1];
    let start = be16(&frame[2..4]);
    let end = be16(&frame[4..6]);
    if !valid_array(provider, array) {
        return short(0x43, OUT_OF_BOUNDS);
    }
    let count = provider.elem_count(array);
    if count == 0 || start >= count || end >= count || start > end {
        return short(0x43, OUT_OF_BOUNDS);
    }
    let mut body = Vec::new();
    let mut last = None;
    let mut element = start;
    loop {
        let size = usize::from(provider.elem_size(array, element));
        if size > 0 {
            if body.len() + size + 10 > MAX_FRAME {
                break;
            }
            if let Some(value) = provider.read(array, element) {
                body.extend_from_slice(&value[..value.len().min(size)]);
            }
        }
        last = Some(element);
        if element == end {
            break;
        }
        element += 1;
    }
    let mut output = header(0x43, last.unwrap_or(start), tick, body.len());
    output.extend(body);
    output
}

fn get_list(frame: &[u8], provider: &dyn DebugProvider, tick: u32) -> Vec<u8> {
    if frame.len() < 3 {
        return short(0x44, OUT_OF_BOUNDS);
    }
    let count = usize::from(be16(&frame[1..3]));
    if count == 0 || count > MAX_INDEXES {
        return short(
            0x44,
            if count > MAX_INDEXES {
                OUT_OF_MEMORY
            } else {
                OUT_OF_BOUNDS
            },
        );
    }
    if frame.len() < 3 + count * 3 {
        return short(0x44, OUT_OF_BOUNDS);
    }
    let mut body = Vec::new();
    let mut last = 0u16;
    for index in 0..count {
        let offset = 3 + index * 3;
        let array = frame[offset];
        let element = be16(&frame[offset + 1..offset + 3]);
        if !valid_array(provider, array) {
            last = index as u16;
            continue;
        }
        let size = usize::from(provider.elem_size(array, element));
        if size == 0 {
            last = index as u16;
            continue;
        }
        if body.len() + size + 10 > MAX_FRAME {
            break;
        }
        if let Some(value) = provider.read(array, element) {
            body.extend_from_slice(&value[..value.len().min(size)]);
        }
        last = index as u16;
    }
    let mut output = header(0x44, last, tick, body.len());
    output.extend(body);
    output
}

fn md5(frame: &[u8], provider: &dyn DebugProvider) -> Vec<u8> {
    let Some(md5) = (frame.len() >= 3).then(|| provider.md5()).flatten() else {
        return short(0x45, NOT_LOADED);
    };
    let mut output = vec![0x45, SUCCESS];
    output.extend_from_slice(&md5.as_bytes()[..md5.len().min(32)]);
    output.extend_from_slice(&0xdead_u16.to_ne_bytes());
    output
}
