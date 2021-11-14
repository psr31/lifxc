use anyhow::{anyhow, ensure, Result};

const PACKET_ERROR: &str = "Bad packet received from device.";

fn read_u16(input: &[u8]) -> u16 {
    u16::from_le_bytes([input[0], input[1]])
}

fn read_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes([input[0], input[1], input[2], input[3]])
}

fn read_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes([
        input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7],
    ])
}

fn read_lifx_str(input: &[u8]) -> Result<&str> {
    let last = input
        .iter()
        .position(|b| *b == 0)
        .ok_or(anyhow!(PACKET_ERROR))?;
    std::str::from_utf8(&input[..last]).map_err(|_| anyhow!(PACKET_ERROR))
}

pub enum Message {
    GetService,

    GetPower,
    SetPower(bool),
    StatePower(bool),

    GetLabel,
    SetLabel(String),
    StateLabel(String),

    GetColor,
    SetColor(u16, u16, u16, u16, u32),
    LightState(u16, u16, u16, u16, bool, String),

    Unknown,
}

impl Message {
    const GET_SERVICE: u16 = 0x02;
    const GET_POWER: u16 = 0x14;
    const SET_POWER: u16 = 0x15;
    const STATE_POWER: u16 = 0x16;
    const GET_LABEL: u16 = 0x17;
    const SET_LABEL: u16 = 0x18;
    const STATE_LABEL: u16 = 0x19;
    const GET_COLOR: u16 = 0x65;
    const SET_COLOR: u16 = 0x66;
    const LIGHT_STATE: u16 = 0x6B;

    pub fn ty(&self) -> u16 {
        use Message::*;

        match self {
            GetService => Self::GET_SERVICE,
            GetPower => Self::GET_POWER,
            SetPower(_) => Self::SET_POWER,
            StatePower(_) => Self::STATE_POWER,
            GetLabel => Self::GET_LABEL,
            SetLabel(_) => Self::SET_LABEL,
            StateLabel(_) => Self::STATE_LABEL,
            GetColor => Self::GET_COLOR,
            SetColor(..) => Self::SET_COLOR,
            LightState(..) => Self::LIGHT_STATE,
            Unknown => u16::MAX,
        }
    }

    pub fn encode(&self, require_ack: bool, sequence: u8, target: Option<u64>) -> Vec<u8> {
        let mut packet = Vec::new();

        // header
        packet.extend([0u8; 3]); // Reserve space for length + LSB of protocol
        packet.push(0x14 | (target.is_some() as u8) << 5); // MSB of protocol and tagged bit
        packet.extend(2u32.to_le_bytes()); // Source

        // address
        packet.extend(target.unwrap_or(0).to_le_bytes()); // Target
        packet.extend([0u8; 6]); // Reserved
        packet.push((require_ack as u8) << 1);
        packet.push(sequence); // Sequence

        // protocol
        packet.extend([0u8; 8]); // Reserved
        packet.extend(self.ty().to_le_bytes()); // Message type
        packet.extend([0u8; 2]); // Reserved

        // payload
        packet.extend(self.construct_payload());

        // length prefix
        let len_bytes = packet.len().to_le_bytes();
        packet[0] = len_bytes[0];
        packet[1] = len_bytes[1];

        packet
    }

    pub fn decode(ty: u16, payload: &[u8]) -> Result<Message> {
        Ok(match ty {
            Self::STATE_POWER => {
                ensure!(payload.len() == 2, PACKET_ERROR);
                let power = read_u16(payload);
                Message::StatePower(power > 0)
            }
            Self::STATE_LABEL => {
                ensure!(payload.len() == 32, PACKET_ERROR);
                let label = read_lifx_str(payload)?;
                Message::StateLabel(label.to_string())
            }
            Self::LIGHT_STATE => {
                ensure!(payload.len() == 52, PACKET_ERROR);
                let hue = read_u16(payload);
                let saturation = read_u16(&payload[2..]);
                let brightness = read_u16(&payload[4..]);
                let kelvin = read_u16(&payload[6..]);
                let power = read_u16(&payload[10..]);
                let label = read_lifx_str(&payload[12..])?;
                Message::LightState(
                    hue,
                    saturation,
                    brightness,
                    kelvin,
                    power > 0,
                    label.to_string(),
                )
            }
            _ => Self::Unknown,
        })
    }

    fn construct_payload(&self) -> Vec<u8> {
        use Message::*;

        match self {
            SetPower(power) => {
                let level = if *power { u16::MAX } else { 0 };
                level.to_le_bytes().to_vec()
            }
            SetLabel(label) => {
                let mut chars = label.as_bytes().to_vec();
                chars.resize(32, 0);
                chars
            }
            SetColor(hue, saturation, brightness, kelvin, duration) => {
                let mut payload = Vec::with_capacity(13);
                payload.push(0);
                payload.extend(hue.to_le_bytes());
                payload.extend(saturation.to_le_bytes());
                payload.extend(brightness.to_le_bytes());
                payload.extend(kelvin.to_le_bytes());
                payload.extend(duration.to_le_bytes());
                payload
            }
            _ => Vec::new(),
        }
    }
}

pub struct Response {
    pub message_type: u16,
    pub payload: Vec<u8>,
    pub source: u32,
    pub target: u64,
    pub sequence: u8,

    pub message: Option<Message>,
}

impl Response {
    pub fn decode(raw: &[u8]) -> Result<Response> {
        // Read packet length
        ensure!(raw.len() > 2, PACKET_ERROR);
        let length = read_u16(raw);
        ensure!(raw.len() < length as usize, PACKET_ERROR);

        // Check protocol
        ensure!(raw[2] == 0 && (raw[3] & !0xF8) == 4, PACKET_ERROR);

        let source = read_u32(&raw[4..]);
        let target = read_u64(&raw[8..]);
        let sequence = raw[23];
        let message_type = read_u16(&raw[32..]);
        let payload = raw[36..length as usize].to_vec();

        let message = Message::decode(message_type, &payload)?;

        Ok(Response {
            message_type,
            payload,
            source,
            target,
            sequence,
            message: Some(message),
        })
    }
}
