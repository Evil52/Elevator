#![cfg_attr(not(test), no_std)]

pub const MAX_PAYLOAD: usize = 256;
pub const MAX_RAW: usize = MAX_PAYLOAD + 3;
pub const MAX_COBS: usize = MAX_RAW + MAX_RAW / 254 + 1;
pub const MAX_FRAME: usize = MAX_COBS + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadCrc,
    TooLong,
    Truncated,
    BadCobs,
}

#[must_use]
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

pub fn cobs_encode(input: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    if out.is_empty() {
        return Err(Error::TooLong);
    }
    let mut code_at = 0;
    let mut write = 1;
    let mut code: u8 = 1;
    for &byte in input {
        if byte == 0 {
            out[code_at] = code;
            code_at = write;
            write += 1;
            code = 1;
        } else {
            *out.get_mut(write).ok_or(Error::TooLong)? = byte;
            write += 1;
            code += 1;
            if code == 0xFF {
                out[code_at] = code;
                code_at = write;
                write += 1;
                code = 1;
            }
        }
        if code_at >= out.len() {
            return Err(Error::TooLong);
        }
    }
    out[code_at] = code;
    Ok(write)
}

pub fn cobs_decode(input: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    let mut read = 0;
    let mut write = 0;
    while read < input.len() {
        let code = usize::from(input[read]);
        if code == 0 {
            return Err(Error::BadCobs);
        }
        read += 1;
        let block = input.get(read..read + code - 1).ok_or(Error::BadCobs)?;
        if block.contains(&0) {
            return Err(Error::BadCobs);
        }
        out.get_mut(write..write + block.len())
            .ok_or(Error::TooLong)?
            .copy_from_slice(block);
        write += block.len();
        read += block.len();
        if code != 0xFF && read < input.len() {
            *out.get_mut(write).ok_or(Error::TooLong)? = 0;
            write += 1;
        }
    }
    Ok(write)
}

pub fn encode_frame(seq: u8, payload: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::TooLong);
    }
    let mut raw = [0u8; MAX_RAW];
    raw[0] = seq;
    raw[1..=payload.len()].copy_from_slice(payload);
    let body = 1 + payload.len();
    let crc = crc16(&raw[..body]);
    raw[body..body + 2].copy_from_slice(&crc.to_le_bytes());
    let encoded = cobs_encode(&raw[..body + 2], out)?;
    *out.get_mut(encoded).ok_or(Error::TooLong)? = 0;
    Ok(encoded + 1)
}

pub fn decode_frame(frame: &[u8], out: &mut [u8]) -> Result<(u8, usize), Error> {
    if frame.is_empty() {
        return Err(Error::Truncated);
    }
    let mut raw = [0u8; MAX_RAW];
    let len = cobs_decode(frame, &mut raw)?;
    if len < 3 {
        return Err(Error::Truncated);
    }
    let (body, crc_bytes) = raw[..len].split_at(len - 2);
    if crc16(body) != u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]) {
        return Err(Error::BadCrc);
    }
    let payload = &body[1..];
    out.get_mut(..payload.len())
        .ok_or(Error::TooLong)?
        .copy_from_slice(payload);
    Ok((body[0], payload.len()))
}

pub struct Decoder {
    buf: [u8; MAX_COBS],
    len: usize,
    overflow: bool,
}

impl Decoder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: [0; MAX_COBS],
            len: 0,
            overflow: false,
        }
    }

    pub fn push(&mut self, byte: u8, out: &mut [u8]) -> Option<Result<(u8, usize), Error>> {
        if byte != 0 {
            match self.buf.get_mut(self.len) {
                Some(slot) => {
                    *slot = byte;
                    self.len += 1;
                }
                None => self.overflow = true,
            }
            return None;
        }
        let len = core::mem::take(&mut self.len);
        if core::mem::take(&mut self.overflow) {
            return Some(Err(Error::TooLong));
        }
        if len == 0 {
            return None;
        }
        Some(decode_frame(&self.buf[..len], out))
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

pub mod protocol;

#[cfg(test)]
mod tests {
    use super::*;

    fn nonzero(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| u8::try_from(i % 255 + 1).unwrap())
            .collect()
    }

    fn cobs_enc(input: &[u8]) -> Vec<u8> {
        let mut out = [0u8; 700];
        let n = cobs_encode(input, &mut out).unwrap();
        out[..n].to_vec()
    }

    fn cobs_dec(input: &[u8]) -> Result<Vec<u8>, Error> {
        let mut out = [0u8; 700];
        cobs_decode(input, &mut out).map(|n| out[..n].to_vec())
    }

    fn frame_of(seq: u8, payload: &[u8]) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        let n = encode_frame(seq, payload, &mut buf).unwrap();
        buf[..n].to_vec()
    }

    fn body_of(frame: &[u8]) -> &[u8] {
        assert!(frame.len() > 1, "no frame: {frame:02X?}");
        &frame[..frame.len() - 1]
    }

    #[test]
    fn crc16_matches_known_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn crc16_of_empty_input_is_the_init_value() {
        assert_eq!(crc16(&[]), 0xFFFF);
    }

    #[test]
    fn cobs_encodes_known_vectors() {
        let cases: [(&[u8], &[u8]); 5] = [
            (&[], &[0x01]),
            (&[0x00], &[0x01, 0x01]),
            (&[0x00, 0x00], &[0x01, 0x01, 0x01]),
            (&[0x11, 0x22, 0x00, 0x33], &[0x03, 0x11, 0x22, 0x02, 0x33]),
            (&[0x11, 0x00, 0x00, 0x00], &[0x02, 0x11, 0x01, 0x01, 0x01]),
        ];
        for (input, expected) in cases {
            assert_eq!(cobs_enc(input), expected, "input {input:02X?}");
        }
    }

    #[test]
    fn cobs_decodes_known_vectors() {
        let cases: [(&[u8], &[u8]); 4] = [
            (&[0x01, 0x01], &[0x00]),
            (&[0x01, 0x01, 0x01], &[0x00, 0x00]),
            (&[0x03, 0x11, 0x22, 0x02, 0x33], &[0x11, 0x22, 0x00, 0x33]),
            (&[0x02, 0x11, 0x01, 0x01, 0x01], &[0x11, 0x00, 0x00, 0x00]),
        ];
        for (input, expected) in cases {
            assert_eq!(cobs_dec(input), Ok(expected.to_vec()), "input {input:02X?}");
        }
    }

    #[test]
    fn cobs_round_trips_and_never_emits_zero() {
        let mut inputs: Vec<Vec<u8>> = vec![vec![], vec![0], vec![0, 0, 0], vec![1, 0, 2, 0]];
        for len in [1, 253, 254, 255, 508] {
            inputs.push(nonzero(len));
        }
        let mut with_zeros = nonzero(300);
        with_zeros[100] = 0;
        with_zeros[254] = 0;
        inputs.push(with_zeros);

        for input in inputs {
            let encoded = cobs_enc(&input);
            assert!(!encoded.is_empty(), "empty output for len {}", input.len());
            assert!(
                !encoded.contains(&0),
                "zero in output for len {}",
                input.len()
            );
            assert_eq!(cobs_dec(&encoded), Ok(input.clone()), "len {}", input.len());
        }
    }

    #[test]
    fn cobs_encode_reports_too_long_for_a_small_buffer() {
        let mut out = [0u8; 3];
        assert_eq!(cobs_encode(&[1, 2, 3], &mut out), Err(Error::TooLong));
    }

    #[test]
    fn cobs_decode_rejects_a_block_that_runs_past_the_end() {
        assert_eq!(cobs_dec(&[0x05, 0x11]), Err(Error::BadCobs));
    }

    #[test]
    fn cobs_decode_rejects_a_zero_byte_inside() {
        assert_eq!(cobs_dec(&[0x01, 0x00, 0x01]), Err(Error::BadCobs));
    }

    #[test]
    fn cobs_decode_reports_too_long_for_a_small_buffer() {
        let mut out = [0u8; 2];
        assert_eq!(
            cobs_decode(&[0x04, 0x11, 0x22, 0x33], &mut out),
            Err(Error::TooLong)
        );
    }

    fn assert_round_trip(seq: u8, payload: &[u8]) {
        let frame = frame_of(seq, payload);
        assert_eq!(frame.last(), Some(&0), "frame must end with the delimiter");
        assert_eq!(
            frame.iter().copied().filter(|&b| b == 0).count(),
            1,
            "the delimiter must be the only zero"
        );
        let mut out = [0u8; MAX_PAYLOAD];
        let body = body_of(&frame);
        assert_eq!(decode_frame(body, &mut out), Ok((seq, payload.len())));
        assert_eq!(&out[..payload.len()], payload);
    }

    #[test]
    fn frame_round_trips_a_small_payload() {
        assert_round_trip(7, &[1, 2, 3]);
    }

    #[test]
    fn frame_round_trips_an_empty_payload() {
        assert_round_trip(0, &[]);
    }

    #[test]
    fn frame_round_trips_a_payload_of_zeros() {
        assert_round_trip(255, &[0; 40]);
    }

    #[test]
    fn frame_carries_the_sequence_number() {
        assert_ne!(frame_of(1, &[9, 9]), frame_of(2, &[9, 9]));
    }

    #[test]
    fn any_single_flipped_bit_is_rejected() {
        let frame = frame_of(5, &[0x10, 0x00, 0x20, 0x30, 0x00, 0x40]);
        assert!(frame.len() > 8, "frame too short: {frame:02X?}");
        let body = body_of(&frame);
        let mut out = [0u8; MAX_PAYLOAD];
        for byte in 0..body.len() {
            for bit in 0..8 {
                let mut broken = body.to_vec();
                broken[byte] ^= 1 << bit;
                assert!(
                    decode_frame(&broken, &mut out).is_err(),
                    "byte {byte} bit {bit} was accepted"
                );
            }
        }
    }

    #[test]
    fn a_changed_crc_gives_bad_crc() {
        let frame = frame_of(5, &[1, 2, 3, 4]);
        let mut raw = cobs_dec(body_of(&frame)).unwrap();
        assert_eq!(raw.len(), 1 + 4 + 2, "seq + payload + crc");
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        let body = cobs_enc(&raw);
        let mut out = [0u8; MAX_PAYLOAD];
        assert_eq!(decode_frame(&body, &mut out), Err(Error::BadCrc));
    }

    #[test]
    fn an_empty_frame_is_truncated() {
        let mut out = [0u8; MAX_PAYLOAD];
        assert_eq!(decode_frame(&[], &mut out), Err(Error::Truncated));
    }

    #[test]
    fn fewer_than_three_raw_bytes_is_truncated() {
        let mut out = [0u8; MAX_PAYLOAD];
        let body = cobs_enc(&[0x01, 0x02]);
        assert_eq!(decode_frame(&body, &mut out), Err(Error::Truncated));
    }

    #[test]
    fn every_prefix_of_a_frame_is_rejected() {
        let frame = frame_of(9, &[0x11, 0x22, 0x00, 0x33, 0x44]);
        assert!(frame.len() > 8, "frame too short: {frame:02X?}");
        let body = body_of(&frame);
        let mut out = [0u8; MAX_PAYLOAD];
        for cut in 0..body.len() {
            assert!(
                decode_frame(&body[..cut], &mut out).is_err(),
                "prefix of {cut} bytes was accepted"
            );
        }
    }

    #[test]
    fn the_largest_payload_fits_the_largest_frame() {
        let payload = nonzero(MAX_PAYLOAD);
        let mut buf = [0u8; MAX_FRAME];
        let n = encode_frame(3, &payload, &mut buf).unwrap();
        assert!(n > MAX_PAYLOAD, "frame shorter than its payload: {n}");
        assert!(n <= MAX_FRAME);
        assert_round_trip(3, &payload);
    }

    #[test]
    fn one_byte_over_the_limit_is_too_long() {
        let payload = nonzero(MAX_PAYLOAD + 1);
        let mut buf = [0u8; 2 * MAX_FRAME];
        assert_eq!(encode_frame(3, &payload, &mut buf), Err(Error::TooLong));
    }

    #[test]
    fn encode_frame_reports_too_long_for_a_small_buffer() {
        let mut buf = [0u8; 4];
        assert_eq!(
            encode_frame(1, &[1, 2, 3, 4], &mut buf),
            Err(Error::TooLong)
        );
    }

    #[test]
    fn decode_frame_reports_too_long_when_the_frame_is_over_the_limit() {
        let body = cobs_enc(&nonzero(MAX_RAW + 1));
        let mut out = [0u8; 2 * MAX_PAYLOAD];
        assert_eq!(decode_frame(&body, &mut out), Err(Error::TooLong));
    }

    #[test]
    fn decode_frame_reports_too_long_when_out_is_small() {
        let frame = frame_of(1, &[1, 2, 3, 4, 5]);
        let mut out = [0u8; 2];
        assert_eq!(decode_frame(body_of(&frame), &mut out), Err(Error::TooLong));
    }

    type Item = Result<(u8, Vec<u8>), Error>;

    fn feed(decoder: &mut Decoder, bytes: &[u8]) -> Vec<Item> {
        let mut out = [0u8; MAX_PAYLOAD];
        let mut items = Vec::new();
        for &byte in bytes {
            if let Some(result) = decoder.push(byte, &mut out) {
                items.push(result.map(|(seq, len)| (seq, out[..len].to_vec())));
            }
        }
        items
    }

    #[test]
    fn decoder_returns_nothing_until_the_delimiter() {
        let frame = frame_of(4, &[1, 2, 3]);
        let mut decoder = Decoder::new();
        assert_eq!(feed(&mut decoder, body_of(&frame)), vec![]);
        assert_eq!(feed(&mut decoder, &[0]), vec![Ok((4, vec![1, 2, 3]))]);
    }

    #[test]
    fn decoder_reads_frames_back_to_back() {
        let mut bytes = frame_of(1, &[0xAA]);
        bytes.extend(frame_of(2, &[0x00, 0xBB]));
        let mut decoder = Decoder::new();
        assert_eq!(
            feed(&mut decoder, &bytes),
            vec![Ok((1, vec![0xAA])), Ok((2, vec![0x00, 0xBB]))]
        );
    }

    #[test]
    fn decoder_skips_empty_frames() {
        let mut bytes = vec![0, 0, 0];
        bytes.extend(frame_of(6, &[7]));
        let mut decoder = Decoder::new();
        assert_eq!(feed(&mut decoder, &bytes), vec![Ok((6, vec![7]))]);
    }

    #[test]
    fn decoder_resyncs_after_garbage() {
        let mut bytes = vec![0x13, 0x37, 0x00];
        bytes.extend(frame_of(8, &[1, 2]));
        let mut decoder = Decoder::new();
        let items = feed(&mut decoder, &bytes);
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(items[0].is_err(), "garbage was accepted: {items:?}");
        assert_eq!(items[1], Ok((8, vec![1, 2])));
    }

    #[test]
    fn decoder_drops_an_overlong_frame_and_recovers() {
        let mut bytes = vec![0x01; MAX_FRAME + 10];
        bytes.push(0x00);
        bytes.extend(frame_of(9, &[5]));
        let mut decoder = Decoder::new();
        let items = feed(&mut decoder, &bytes);
        assert_eq!(items, vec![Err(Error::TooLong), Ok((9, vec![5]))]);
    }
}
