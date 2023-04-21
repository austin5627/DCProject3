use std::fmt::{Debug, Display};
use std::io::{Read, Write};

use std::net::TcpStream;

use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum Message {
    Search(i32), // Sender layer
    Ack,
    Reject,
    NewPhase(i32),
    PhaseComplete(bool), // New children added?
    Terminate,
    Connect(i32), // Sender id
}

impl Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Search(l) => write!(f, "{}({})", "Search".blue(), l),
            Message::Ack => write!(f, "{}", "Ack".green()),
            Message::Reject => write!(f, "{}", "Reject".red()),
            Message::NewPhase(l) => write!(f, "{}({})", "NewPhase".cyan(), l),
            Message::PhaseComplete(b) => write!(f, "{}({})", "PhaseComplete".purple(), b),
            Message::Terminate => write!(f, "{}", "Terminate".yellow()),
            Message::Connect(_) => write!(f, "{}", "Connect".magenta()),
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn send_message(msg: Message, stream: &mut TcpStream) -> Result<(), std::io::Error> {
    let bytes: Vec<u8> = msg.into();
    let len = bytes.len() as u32;
    let len_bytes = len.to_le_bytes();
    stream.write(&len_bytes)?;
    stream.write(&bytes)?;
    Ok(())
}

pub fn recv_message(stream: &mut TcpStream) -> Option<Message> {
    let mut len_bytes = [0; 4];
    stream.read_exact(&mut len_bytes).ok()?;
    let len = u32::from_le_bytes(len_bytes);
    let mut bytes = vec![0; len as usize];
    stream.read_exact(&mut bytes).ok()?;
    let msg: Message = bytes[..].into();
    Some(msg)
}

impl From<&[u8]> for Message {
    fn from(bytes: &[u8]) -> Self {
        bincode::deserialize(bytes).expect("Unable to deserialize message")
    }
}

impl From<Message> for Vec<u8> {
    fn from(msg: Message) -> Self {
        bincode::serialize(&msg).expect("Unable to serialize message")
    }
}
