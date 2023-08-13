//! this is some module stuff

pub trait Write {
    fn write_all(&self, buf: &[u8]) -> Result<(), ProtocolError>;
}

pub trait Writeable {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()>;
}
impl Writeable for () {
    fn write_to<W: Write>(&self, _writer: &mut W) -> ProtocolResult<()> {
        Ok(())
    }
}
impl Writeable for u32 {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        let bytes = self.to_be_bytes();
        writer.write_all(&[bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}
impl Writeable for u16 {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        let bytes = self.to_be_bytes();
        writer.write_all(&[bytes[0], bytes[1]])
    }
}
impl Writeable for bool {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[*self as u8])
    }
}

#[derive(Clone, Debug)]
pub enum ProtocolError {
    UnrecognizedCommand(u8),
    UnrecognizedChangeHeightCommand(u8),
    UnrecognizedReportHeightCommand(u8),
    UnrecognizedMoveState(u8),
    // BadCheckSum(Command),
    UnrecognizedResponseState(u8),
}
impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Protocol Error")
    }
}
pub type ProtocolResult<T> = Result<T, ProtocolError>;

// Will be valic with first byte being 0xFA and last being 0xFD
pub struct Packet<'a> {
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
    fn get_data(&self) -> &[u8] {
        let len = self.raw_data.len();
        &self.raw_data[3..len - 4]
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

pub enum ValidChecksum {
    Valid,
    Invalid,
}

pub trait CommandId {
    fn command_id(&self) -> u8;
}
impl CommandId for () {
    fn command_id(&self) -> u8 {
        unreachable!("Command id of the empty type should never be constructed")
    }
}

pub trait EventResponse: CommandId + Writeable {
    type Response: CommandId + Writeable;
    const EVENT_ID: u8;
    const RESPONSE_ID: u8 = Self::EVENT_ID + 1;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self>
    where
        Self: Sized;

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response>
    where
        Self::Response: Sized;
}

#[derive(Debug, Clone)]
pub enum BaseCommand {
    ChangeHeight(Command<ChangeHeight>),
    ReportHeight(Command<ReportHeight>),
    ReportControllerState(Command<ControllerState>),
    Connect(Command<Connect>),
    // controller: 0x15, desk: 0x16
    // 0x15 is a request for information it seems. The desk responds with 0x16 and the matching command id and 2 bytes of data
    HandShake(Command<Handshake>),
    // 0x13, 24 bit identiier
    Identify(Command<Id>),
}
impl From<u8> for BaseCommand {
    fn from(value: u8) -> Self {
        match value {
            ChangeHeight::EVENT_ID => BaseCommand::ChangeHeight(Command::Command(
                ChangeHeight::Up(ChangeHeightState::Start),
            )),
            ChangeHeight::RESPONSE_ID => {
                BaseCommand::ChangeHeight(Command::Reponse(ChangeHeight::SavedOne))
            }
            Connect::EVENT_ID => BaseCommand::Connect(Command::Command(Connect { state: () })),
            Connect::RESPONSE_ID => {
                BaseCommand::Connect(Command::Reponse(Connect::<bool> { state: true }))
            }
            // Add more cases here as needed.
            _ => todo!(),
        }
    }
}
impl Writeable for BaseCommand {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        match self {
            BaseCommand::ChangeHeight(command) => command.write_to(writer),
            BaseCommand::ReportHeight(command) => command.write_to(writer),
            BaseCommand::ReportControllerState(command) => command.write_to(writer),
            BaseCommand::Connect(command) => command.write_to(writer),
            BaseCommand::HandShake(command) => command.write_to(writer),
            BaseCommand::Identify(command) => command.write_to(writer),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command<C: EventResponse> {
    Command(C),
    Reponse(C::Response),
}
impl<C: EventResponse> Writeable for Command<C> {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        match self {
            Command::Command(c) => {
                writer.write_all(&[C::EVENT_ID])?;
                c.write_to(writer)
            }
            Command::Reponse(cr) => {
                writer.write_all(&[C::RESPONSE_ID])?;
                cr.write_to(writer)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ChangeHeight<S = ChangeHeightState> {
    Up(S),
    Down(S),
    SavedOne,
    SavedTwo,
    SavedThree,
}
impl<S> CommandId for ChangeHeight<S> {
    fn command_id(&self) -> u8 {
        match self {
            ChangeHeight::Up(_) => 0x03,
            ChangeHeight::Down(_) => 0x04,
            ChangeHeight::SavedOne => 0x06,
            ChangeHeight::SavedTwo => 0x07,
            ChangeHeight::SavedThree => 0x08,
        }
    }
}
impl EventResponse for ChangeHeight<ChangeHeightState> {
    type Response = ChangeHeight<()>;
    const EVENT_ID: u8 = 0x17;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self> {
        // match packet.get_command_id() {
        //     0x03 =>
        // }
        todo!()
    }

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response>
    where
        Self::Response: Sized,
    {
        todo!()
    }
}
impl<S: Writeable> Writeable for ChangeHeight<S> {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[self.command_id()])?;
        match self {
            ChangeHeight::Up(state) | ChangeHeight::Down(state) => state.write_to(writer),
            ChangeHeight::SavedOne | ChangeHeight::SavedTwo | ChangeHeight::SavedThree => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ChangeHeightState {
    Stop = 0,
    Start = 1,
}
impl Writeable for ChangeHeightState {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[*self as u8])
    }
}

#[derive(Debug, Clone)]
pub struct ReportHeight(f32);
impl CommandId for ReportHeight {
    fn command_id(&self) -> u8 {
        0x00
    }
}
impl EventResponse for ReportHeight {
    type Response = ();
    const EVENT_ID: u8 = 0x03;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self> {
        todo!()
    }

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response> {
        todo!()
    }
}
impl Writeable for ReportHeight {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        let height = (self.0 * 10.) as u16;
        let height = height.to_be_bytes();
        writer.write_all(&[height[0], height[1]])
    }
}

#[derive(Debug, Clone)]
pub struct Connect<S = ()> {
    state: S,
}
impl<S> CommandId for Connect<S> {
    fn command_id(&self) -> u8 {
        0x11
    }
}
impl EventResponse for Connect<()> {
    type Response = Connect<bool>;
    const EVENT_ID: u8 = 0x11;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self> {
        todo!()
    }

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response> {
        todo!()
    }
}
impl<S: Writeable> Writeable for Connect<S> {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[self.command_id()])?;
        self.state.write_to(writer)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ControllerState {
    Ok = 0xA004,
}
impl CommandId for ControllerState {
    fn command_id(&self) -> u8 {
        0xA0
    }
}
impl EventResponse for ControllerState {
    type Response = ();
    const EVENT_ID: u8 = 0x01;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self> {
        todo!()
    }

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response> {
        todo!()
    }
}
impl Writeable for ControllerState {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[self.command_id()])?;
        let bytes = (*self as u16).to_be_bytes();
        writer.write_all(&[bytes[0], bytes[1]])
    }
}

#[derive(Debug, Clone)]
pub struct Id<D = u32> {
    commmand_id: u8,
    data: D,
}
impl<D> CommandId for Id<D> {
    fn command_id(&self) -> u8 {
        self.commmand_id
    }
}
impl EventResponse for Id<u32> {
    type Response = Id<u16>;
    const EVENT_ID: u8 = 0x13;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self> {
        todo!()
    }

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response> {
        todo!()
    }
}
impl<D: Writeable> Writeable for Id<D> {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[self.commmand_id])?;
        self.data.write_to(writer)
    }
}

#[derive(Debug, Clone)]
pub enum Handshake<D = ()> {
    Thirteen(D),
    Fourteen(D),
    Fifteen(D),
    TwentyOne(D),
    TwentyTwo(D),
    TwentyThree(D),
    SeventyTwo(D),
    SeventyThree(D),
}
impl<D> CommandId for Handshake<D> {
    fn command_id(&self) -> u8 {
        match self {
            Handshake::Thirteen(_) => 0x13,
            Handshake::Fourteen(_) => 0x14,
            Handshake::Fifteen(_) => 0x15,
            Handshake::TwentyOne(_) => 0x32,
            Handshake::TwentyTwo(_) => 0x21,
            Handshake::TwentyThree(_) => 0x23,
            Handshake::SeventyTwo(_) => 0x72,
            Handshake::SeventyThree(_) => 0x73,
        }
    }
}
impl EventResponse for Handshake<()> {
    type Response = Handshake<u16>;
    const EVENT_ID: u8 = 0x15;

    fn read_event_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self> {
        todo!()
    }

    fn read_response_from<'a>(packet: &'a Packet<'a>) -> ProtocolResult<Self::Response> {
        todo!()
    }
}
impl<D: Writeable> Writeable for Handshake<D> {
    fn write_to<W: Write>(&self, writer: &mut W) -> ProtocolResult<()> {
        writer.write_all(&[self.command_id()])?;
        match self {
            Handshake::Thirteen(data)
            | Handshake::Fourteen(data)
            | Handshake::Fifteen(data)
            | Handshake::TwentyOne(data)
            | Handshake::TwentyTwo(data)
            | Handshake::TwentyThree(data)
            | Handshake::SeventyTwo(data)
            | Handshake::SeventyThree(data) => data.write_to(writer),
        }
    }
}
