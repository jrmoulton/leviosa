pub trait Write {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), ProtocolError>;
}

#[derive(Clone, Debug)]
pub enum ProtocolError {
    UnrecognizedCommand(u8),
    UnrecognizedChangeHeightCommand(u8),
    UnrecognizedReportHeightCommand(u8),
    UnrecognizedMoveState(u8),
    BadCheckSum(Command),
    UnrecognizedResponseState(u8),
}
impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Protocol Error")
    }
}
pub type ProtocolResult<T> = Result<T, ProtocolError>;

// Will be valic with first byte being 0xFA and last being 0xFD
struct Packet<'a> {
    raw_data: &'a mut [u8],
}
impl<'a> Packet<'a> {
    fn get_command_prefix(&self) -> u8 {
        self.raw_data[1]
    }
    fn get_command_id(&self) -> u8 {
        self.raw_data[2]
    }
    fn get_checksum(&self) -> u8 {
        self.raw_data[self.raw_data.len() - 2]
    }
    fn get_packet_num(&self) -> u16 {
        let len = self.raw_data.len();
        let slice = &self.raw_data[len - 4..=len - 3];
        u16::from_be_bytes([slice[0], slice[1]])
    }
    fn get_data(&self, len: usize) -> &[u8] {
        &self.raw_data[3..3 + len]
    }
    fn validate_checksum(&self) -> ValidChecksum {
        let len = self.raw_data.len();
        // -2 to exclude the end tag and the chesksum itself
        let computed_checksum = self.raw_data[1..len - 2].iter().fold(0, |acc, &b| acc ^ b);
        if computed_checksum == self.get_checksum() {
            ValidChecksum::Valid
        } else {
            ValidChecksum::Invalid
        }
    }
    fn insert_checksum(&mut self) {
        let len = self.raw_data.len();
        let computed_checksum = self.raw_data[1..len - 2].iter().fold(0, |acc, &b| acc ^ b);
        self.raw_data[len - 2] = computed_checksum;
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    /// controller: 0x17, desk: 0x18
    ChangeHeight(SourceChangeHeight),
    /// desk: 0x03
    ReportHeight(ReportHeight),
    /// controller: 0x01
    ControllerKeepAlive(),
    /// controller: 0x11, desk: 0x12
    Connect(SourceConnect),
    /// controller: 0x15, desk: 0x16
    /// 0x15 is a request for information it seems. The desk responds with 0x16 and the matching command id and 2 bytes of data
    HandShake(),
    /// 0x13, 24 bit identiier
    Identify(Id),
}
impl Command {
    pub fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        match self {
            Command::ChangeHeight(change_height) => change_height.write_to(writer),
            Command::ReportHeight(report_height) => report_height.write_to(writer),
            Command::Connect(connect) => connect.write_to(writer),
            Command::HandShake() => todo!(),
        }
    }

    /// This assumes that the start tag and end tag have already been stripped
    pub fn read_from(packet: &mut Packet) -> ProtocolResult<Self> {
        let command = match packet.get_command_prefix() {
            SourceChangeHeight::DESK_HEIGHT_PREFIX => {
                Self::ChangeHeight(SourceChangeHeight::Desk(ChangeHeight::read_from(packet)?))
            }
            SourceChangeHeight::CONTROLLER_HEIGHT_PREFIX => {
                Self::ChangeHeight(SourceChangeHeight::Controller {
                    height_command: ChangeHeight::read_from(packet)?,
                    response_state: ResponseState::read_from(packet)?,
                })
            }
            ReportHeight::DESK_REPORT_PREFIX => {
                Self::ReportHeight(ReportHeight::read_from(packet)?)
            }

            command => return Err(ProtocolError::UnrecognizedCommand(command)),
        };
        Ok(command)
    }
}

#[derive(Debug, Clone)]
pub enum SourceConnect {
    /// The command
    Controller,
    /// The response
    Desk { response_state: ResponseState },
}
impl SourceConnect {
    const DESK_HEIGHT_PREFIX: u8 = 0x18;
    const CONTROLLER_HEIGHT_PREFIX: u8 = 0x17;

    pub fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        match self {
            SourceChangeHeight::Desk(height_command) => {
                writer.write_all(&[Self::DESK_HEIGHT_PREFIX])?;
                height_command.write_to(writer)
            }
            SourceChangeHeight::Controller {
                height_command,
                response_state,
            } => {
                writer.write_all(&[Self::CONTROLLER_HEIGHT_PREFIX])?;
                height_command.write_to(writer)?;
                writer.write_all(&[*response_state as u8])
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReportHeight(f32);
impl ReportHeight {
    const DESK_REPORT_PREFIX: u8 = 0x03;
    const COMMAND_ID: u8 = 0x00;
    // I'm not sure if this changes
    const STATE: u8 = 0x01;

    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[Self::DESK_REPORT_PREFIX, Self::COMMAND_ID, Self::STATE])?;
        let height = self.0 * 10.;
        let height = height as u16;
        writer.write_all(&height.to_be_bytes())
    }

    pub fn read_from(packet: &mut Packet) -> ProtocolResult<Self> {
        // would handle other command id's here but only know of one so no need to do anything with it for now
        if packet.get_command_id() != Self::COMMAND_ID {
            return Err(ProtocolError::UnrecognizedReportHeightCommand(
                packet.get_command_id(),
            ));
        }
        let height_data = packet.get_data(2);
        let height = u16::from_be_bytes([height_data[0], height_data[1]]);
        let height = height as f32 / 10.;
        Ok(Self(height))
    }
}

#[derive(Debug, Clone)]
pub enum SourceChangeHeight {
    /// The command
    Desk(ChangeHeight),
    /// The response
    Controller {
        height_command: ChangeHeight,
        response_state: ResponseState,
    },
}
impl SourceChangeHeight {
    const DESK_HEIGHT_PREFIX: u8 = 0x18;
    const CONTROLLER_HEIGHT_PREFIX: u8 = 0x17;

    pub fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        match self {
            SourceChangeHeight::Desk(height_command) => {
                writer.write_all(&[Self::DESK_HEIGHT_PREFIX])?;
                height_command.write_to(writer)
            }
            SourceChangeHeight::Controller {
                height_command,
                response_state,
            } => {
                writer.write_all(&[Self::CONTROLLER_HEIGHT_PREFIX])?;
                height_command.write_to(writer)?;
                writer.write_all(&[*response_state as u8])
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ChangeHeight {
    Up(MoveState),
    Down(MoveState),
}
impl ChangeHeight {
    const HEIGHT_UP: u8 = 0x03;
    const HEIGHT_DOWN: u8 = 0x04;

    pub fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        let state = match self {
            ChangeHeight::Up(state) => {
                writer.write_all(&[Self::HEIGHT_UP])?;
                state
            }
            ChangeHeight::Down(state) => {
                writer.write_all(&[Self::HEIGHT_DOWN])?;
                state
            }
        };
        writer.write_all(&[*state as u8])
    }

    pub fn read_from(packet: &mut Packet) -> ProtocolResult<Self> {
        Ok(match packet.get_command_id() {
            Self::HEIGHT_UP => Self::Up(MoveState::read_from(packet)?),
            Self::HEIGHT_DOWN => Self::Down(MoveState::read_from(packet)?),
            val => {
                return Err(ProtocolError::UnrecognizedChangeHeightCommand(val));
            }
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MoveState {
    Stop = 0,
    Start = 1,
}
impl MoveState {
    pub fn read_from(packet: &mut Packet) -> ProtocolResult<Self> {
        Ok(match packet.get_data(1)[0] {
            0 => Self::Stop,
            1 => Self::Start,
            state => {
                return Err(ProtocolError::UnrecognizedMoveState(state));
            }
        })
    }
}

/// TODO: Finish defining this
#[derive(Debug, Clone, Copy)]
pub enum ResponseState {
    Ok = 0,
}
impl ResponseState {
    pub fn read_from(packet: &mut Packet) -> ProtocolResult<Self> {
        Ok(match packet.get_data(1)[0] {
            0 => Self::Ok,
            state => {
                return Err(ProtocolError::UnrecognizedResponseState(state));
            }
        })
    }
}

pub enum ValidChecksum {
    Valid,
    Invalid,
}
