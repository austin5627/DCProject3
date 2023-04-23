use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread::sleep;
use std::time::Duration;

use crate::message::{recv_message, send_message, Message};

macro_rules! debugln {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)] println!($($arg)*);
    };
}

#[derive(Debug)]
pub struct NodeData {
    pub ip: String,
    pub port: i32,
    pub edges: Vec<(i32, i32)>, // neighbor id weight
    pub neighbors: Vec<i32>,
}

pub struct Node {
    pub id: i32,
    pub listeners: HashMap<i32, TcpStream>,
    pub free: bool,
    pub parent: Option<i32>,
    pub children: HashSet<i32>,
    pub neighbors: Vec<i32>,
    pub layer: i32,
    pub starting_node: bool,
    pub phase_complete_received: HashMap<i32, bool>,
    pub acks_received: HashMap<i32, bool>,
}

impl Node {
    pub fn new(id: i32, data: HashMap<i32, NodeData>, leader: i32) -> Node {
        let mut node = Node {
            id,
            listeners: HashMap::new(),
            free: leader != id,
            parent: None,
            children: HashSet::new(),
            neighbors: data[&id].neighbors.clone(),
            layer: if id == leader { 0 } else { -1 },
            starting_node: id == leader,
            phase_complete_received: HashMap::new(),
            acks_received: HashMap::new(),
        };
        node.connect_to_neighbors(data);
        return node;
    }

    fn connect_to_neighbors(&mut self, nodes: HashMap<i32, NodeData>) {
        let node = &nodes[&self.id];
        let listener = TcpListener::bind(format!("{}:{}", node.ip, node.port))
            .expect("Unable to bind to port");
        for (neighbor, _) in &node.edges {
            let mut socket: TcpStream;
            let addr: SocketAddr;
            if self.id < *neighbor {
                loop {
                    match TcpStream::connect(format!(
                        "{}:{}",
                        nodes[neighbor].ip, nodes[neighbor].port
                    )) {
                        Ok(s) => {
                            socket = s;
                            addr = socket.peer_addr().expect("Unable to get peer address");
                            break;
                        }
                        Err(_) => {
                            debugln!("Unable to connect to {}, retrying...", neighbor);
                            sleep(Duration::from_secs(1));
                        }
                    }
                }
            } else {
                // accept connections from neighbors with lower id
                let conn = listener.accept().expect("Unable to accept connection");
                socket = conn.0;
                addr = conn.1;
            }
            send_message(
                Message::Connect(self.id),
                &mut socket.try_clone().expect("Unable to clone socket"),
            )
            .expect("Unable to send message");
            let msg = recv_message(&mut socket).expect("Unable to receive message");
            if let Message::Connect(other_id) = msg {
                debugln!("Connection established with {} {}", addr, other_id);
                self.listeners.insert(other_id, socket);
            } else {
                panic!("Unexpected message type");
            }
        }
    }

    pub fn broadcast(&mut self, msg: Message) -> bool {
        for neighbor in &self.neighbors.clone() {
            if Some(*neighbor) == self.parent {
                continue;
            }
            if !self.send(*neighbor, msg.clone()) {
                return false;
            }
        }
        return true;
    }

    pub fn broadcast_tree(&mut self, msg: Message) -> bool {
        for child in &self.children.clone() {
            if !self.send(*child, msg.clone()) {
                return false;
            }
        }
        return true;
    }

    pub fn send(&mut self, id: i32, msg: Message) -> bool {
        let listener = &mut self.listeners.get_mut(&id).expect("No listener for id");
        debugln!("Sent {:?} to {}", msg, id);
        let ok = send_message(msg, listener);
        if let Err(_) = ok {
            return false;
        }
        return true;
    }
}


impl Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("\n\tid", &self.id)
            .field("\n\tfree", &self.free)
            .field("\n\tparent", &self.parent)
            .field("\n\tchildren", &self.children)
            .field("\n\tneighbors", &self.neighbors)
            .field("\n\tlayer", &self.layer)
            .field("\n\tresponses_received", &self.phase_complete_received)
            .field("\n\tacks_received", &self.acks_received)
            .field("\n", &"")
            .finish()
    }
}
