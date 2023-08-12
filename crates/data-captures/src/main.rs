use std::fmt::Display;

/// A Segment is a segment of Packets that are sent together from one device to another without interruption from the other device (half duplex)

#[derive(Debug, PartialEq, PartialOrd)]
enum Segment<'a> {
    Desk(&'a [Packet<'a>]),
    Controller(&'a [Packet<'a>]),
}
impl<'a> std::fmt::Display for Segment<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Segment::Desk(_) => f.write_str("Desk\n")?,
            Segment::Controller(_) => f.write_str("Controller\n")?,
        };
        match self {
            Segment::Desk(packet) | Segment::Controller(packet) => {
                for temp in packet.iter() {
                    f.write_str(&temp.to_string())?;
                }
            }
        }
        f.write_str("\n")
    }
}

/// A Packet in the communication always starts with 0xFA and ends with 0xFD
#[derive(Debug, PartialEq)]
enum Packet<'a> {
    Desk(&'a [Frame]),
    Controller(&'a [Frame]),
}
impl<'a> Display for Packet<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Packet::Desk(frame) | Packet::Controller(frame) => {
                for temp in frame.iter() {
                    f.write_fmt(format_args!("{} ", &temp.to_string()))?;
                }
            }
        }
        f.write_str("\n")
    }
}
impl<'a> PartialOrd for Packet<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (
                Packet::Desk(first_frame) | Packet::Controller(first_frame),
                Packet::Desk(second_frame) | Packet::Controller(second_frame),
            ) => first_frame
                .get(0)?
                .time
                .partial_cmp(&second_frame.get(0)?.time),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Source {
    Desk,
    Controller,
}

/// A frame is single parsed uart data packet
#[derive(Debug, PartialEq)]
struct Frame {
    time: f64,
    value: FrameValue,
}
impl std::fmt::Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value.to_string())
    }
}
impl PartialOrd for Frame {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time.partial_cmp(&other.time)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd)]
enum FrameValue {
    Value(u8),
    ParityError(String),
    FramingError(String),
}
impl std::fmt::Display for FrameValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameValue::Value(val) => write!(f, "{:#04x}", val),
            FrameValue::ParityError(pe) => write!(f, "{}", pe),
            FrameValue::FramingError(fe) => write!(f, "{}", fe),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let controller_frames = parse_frames("crates/data-captures/data/connect/controller.csv")?;
    let desk_frames = parse_frames("crates/data-captures/data/connect/desk.csv")?;

    let controller_packets = parse_packets(&controller_frames, Source::Controller);
    let mut desk_packets = parse_packets(&desk_frames, Source::Desk);
    let mut all_packets = controller_packets;
    all_packets.append(&mut desk_packets);
    all_packets.sort_by(|first, second| {
        first
            .partial_cmp(second)
            .unwrap_or(std::cmp::Ordering::Less)
    });

    let segments = build_segments(&all_packets);

    for segment in segments {
        println!("{segment}");
    }

    // let segments =

    Ok(())
}

fn build_segments<'a>(all_packets: &'a [Packet]) -> Vec<Segment<'a>> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut start_index = 0;
    let mut current_source = match all_packets.first() {
        Some(Packet::Desk(_)) => Source::Desk,
        Some(Packet::Controller(_)) => Source::Controller,
        None => return Vec::new(),
    };

    for (index, packet) in all_packets.iter().enumerate() {
        match (packet, current_source) {
            (Packet::Desk(_), Source::Desk) | (Packet::Controller(_), Source::Controller) => {
                continue
            }
            (Packet::Desk(_), Source::Controller) => {
                segments.push(Segment::Controller(&all_packets[start_index..index]));
                start_index = index;
                current_source = Source::Desk;
            }
            (Packet::Controller(_), Source::Desk) => {
                segments.push(Segment::Desk(&all_packets[start_index..index]));
                start_index = index;
                current_source = Source::Controller;
            }
        }
    }

    if start_index < all_packets.len() {
        match current_source {
            Source::Desk => segments.push(Segment::Desk(&all_packets[start_index..])),
            Source::Controller => segments.push(Segment::Controller(&all_packets[start_index..])),
        }
    }

    segments
}

fn parse_packets(frames: &[Frame], source: Source) -> Vec<Packet> {
    let mut packets = Vec::new();
    let mut start_index = None;

    for (index, frame) in frames.iter().enumerate() {
        match frame.value {
            FrameValue::Value(0xFA) => start_index = Some(index),
            FrameValue::Value(0xFD) => {
                if let Some(start) = start_index {
                    let packet_data = &frames[start + 1..index];
                    packets.push(match source {
                        Source::Desk => Packet::Desk(packet_data),
                        Source::Controller => Packet::Controller(packet_data),
                    });
                    start_index = None;
                }
            }
            _ => continue,
        }
    }
    packets
}

fn parse_frames(path: &str) -> Result<Vec<Frame>, Box<dyn std::error::Error>> {
    let raw_data = std::fs::read_to_string(path)?;
    let mut csv_reader = csv::Reader::from_reader(raw_data.as_bytes());
    let mut parsed_frames = Vec::new();

    for result in csv_reader.records() {
        let record = result?;
        let time: f64 = record[0].parse()?;
        let value = u8::from_str_radix(&record[1][2..], 16)?;

        let frame_value = match (record.get(2), record.get(3)) {
            (Some(pe), _) if !pe.is_empty() => FrameValue::ParityError(pe.to_string()),
            (_, Some(fe)) if !fe.is_empty() => FrameValue::FramingError(fe.to_string()),
            _ => FrameValue::Value(value),
        };

        parsed_frames.push(Frame {
            time,
            value: frame_value,
        });
    }
    Ok(parsed_frames)
}
