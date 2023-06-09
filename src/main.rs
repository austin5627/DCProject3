extern crate owo_colors;
extern crate serde;

use std::collections::HashMap;
use std::env::args;
use std::sync::mpsc::channel;
use std::thread;

mod message;
mod node;

use message::Message;
use node::{Node, NodeData};

use crate::message::recv_message;

macro_rules! debugln {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)] println!($($arg)*);
    };
}

fn read_config(filename: &str) -> (HashMap<i32, NodeData>, i32) {
    let file_string = std::fs::read_to_string(filename).expect("Failed to read config file");
    let mut nodes: HashMap<i32, NodeData> = HashMap::new();

    let mut lines = file_string
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.starts_with(|c: char| c.is_digit(10) || c == '(' || c == ')'));

    let num_nodes = lines
        .next()
        .expect("Config file is empty")
        .parse::<i32>()
        .expect("First line of config file must be an integer");

    for _ in 0..num_nodes {
        if let Some(line) = lines.next() {
            let mut parts = line.split_whitespace();
            let id = parts
                .next()
                .expect("Node ID not found")
                .parse::<i32>()
                .expect("Node ID must be an integer");
            let ip = parts.next().expect("IP not found").to_string();
            let port = parts
                .next()
                .expect("Port not found")
                .parse::<i32>()
                .expect("Port must be an integer");

            let edges = Vec::new();
            let neighbors = Vec::new();
            let node = NodeData {
                ip,
                port,
                edges,
                neighbors,
            };
            nodes.insert(id, node);
        }
    }

    let mut leader = None;
    while let Some(line) = lines.next() {
        let mut parts = line.split_whitespace();
        // smaller id, larger id
        let edge = parts
            .next()
            .expect("Edge line is empty")
            .trim_matches(|c| c == '(' || c == ')')
            .split(',')
            .map(|s| s.parse::<i32>().expect("Node id be an integer"))
            .collect::<Vec<i32>>();
        if edge.len() == 1 {
            leader = Some(edge[0]);
            break;
        }

        let weight = parts
            .next()
            .expect("Weight not found")
            .parse::<i32>()
            .expect("Weight must be an integer");
        let n0 = nodes.get_mut(&edge[0]).expect("Node {edge[0]} not found");
        n0.edges.append(&mut vec![(edge[1], weight)]);
        n0.neighbors.append(&mut vec![edge[1]]);
        let n1 = nodes.get_mut(&edge[1]).expect("Node {edge[1]} not found");
        n1.edges.append(&mut vec![(edge[0], weight)]);
        n1.neighbors.append(&mut vec![edge[0]]);
    }

    for node in nodes.values_mut() {
        node.edges.sort_by(|a, b| a.1.cmp(&b.1));
    }

    return (nodes, leader.unwrap());
}
fn handle_message_hybrid_bfs(
    node: &mut Node,
    msg: Message,
    id: i32,
    layers_per_phase: i32,
) -> bool {
    println!("Received: {:?} from {}", msg, id);
    match msg {
        Message::Search(layer, max_layer) => {
            if node.free || node.layer > layer + 1 {
                node.free = false;
                node.layer = layer + 1;
                if let Some(parent) = node.parent {
                    node.send(parent, Message::Reject);
                }
                node.parent = Some(id);
                if node.layer + 1 <= max_layer {
                    node.broadcast(Message::Search(node.layer, max_layer));
                } else {
                    node.send(id, Message::Ack);
                }
            } else {
                node.send(id, Message::Reject);
            }
        }
        Message::Ack | Message::Reject => {
            node.acks_received.insert(id, msg == Message::Ack);
            if msg == Message::Ack {
                node.children.insert(id);
            } else if msg == Message::Reject {
                node.children.remove(&id);
            }
            let should_send = if node.parent.is_some() {
                node.acks_received.len() == node.neighbors.len() - 1
            } else {
                node.acks_received.len() == node.neighbors.len()
            };
            if should_send {
                if let Some(parent) = node.parent {
                    let child_added = node.acks_received.values().any(|&x| x);
                    if node.starting_node {
                        node.starting_node = false;
                        node.send(parent, Message::PhaseComplete(child_added));
                    } else {
                        node.send(parent, Message::Ack);
                    }
                } else {
                    node.layer += 1;
                    node.broadcast_tree(Message::NewPhase(node.layer));
                }
                node.acks_received.clear();
            }
        }
        Message::NewPhase(layer) => {
            if node.layer == layer {
                node.starting_node = true;
                node.broadcast(Message::Search(layer, layer + layers_per_phase));
            } else if !node.children.is_empty() {
                node.broadcast_tree(Message::NewPhase(layer));
            } else if let Some(parent) = node.parent {
                node.send(parent, Message::PhaseComplete(false));
            }
        }
        Message::PhaseComplete(child_added) => {
            node.phase_complete_received.insert(id, child_added);
            if node.phase_complete_received.len() == node.children.len() {
                let child_added = node.phase_complete_received.values().any(|&x| x);
                if let Some(parent) = node.parent {
                    node.send(parent, Message::PhaseComplete(child_added));
                } else {
                    if child_added {
                        node.layer += 1;
                        node.broadcast_tree(Message::NewPhase(node.layer));
                    } else {
                        node.broadcast(Message::Terminate);
                        return true;
                    }
                }
                node.phase_complete_received.clear();
            }
        }
        Message::Terminate => {
            node.broadcast(Message::Terminate);
            return true;
        }
        Message::Connect(_) => {
            panic!("Unexpected message type");
        }
    }
    debugln!("{:#?}", node);

    return false;
}

fn hybrid_bfs(node_id: i32, nodes: HashMap<i32, NodeData>, leader: i32, layers_per_phase: i32) {
    let mut node = Node::new(node_id, nodes, leader);
    if node_id == leader {
        node.broadcast(Message::Search(0, layers_per_phase));
    }
    let mut threads = Vec::new();
    let (tx, rx) = channel::<(i32, Message)>();
    for id in node.neighbors.iter() {
        let mut socket = node.sockets[&id].try_clone().unwrap();
        let tx = tx.clone();
        let id = *id;
        let t = thread::spawn(move || loop {
            let msg_op = recv_message(&mut socket);
            if let Some(msg) = msg_op {
                if let Err(_) = tx.send((id, msg)) {
                    break;
                }
            } else {
                break;
            }
        });
        threads.push(t);
    }

    loop {
        let (id, msg) = rx.recv().expect("Cannot receive message");
        if handle_message_hybrid_bfs(&mut node, msg, id, layers_per_phase) {
            break;
        }
    }
    println!();
    println!();
    println!();
    println!("Node {}", node_id);
    if let Some(parent) = node.parent {
        println!("Depth: {}", node.layer);
        println!("Parent: {}", parent);
    } else {
        println!("Depth: 0");
        println!("Root node");
    }
    println!("Children: {:?}", node.children);
}

fn main() {
    let args: Vec<String> = args().collect();
    if args.len() < 3 {
        println!(
            "Usage: {} <config file> <node id> [layers per phase]",
            args[0]
        );
        return;
    }
    let (nodes, leader) = read_config(&args[1]);
    let node_id = args[2].parse::<i32>().expect("Node id must be an integer");
    println!("Starting node {}...", node_id);
    let layers_per_phase = match args.get(3) {
        Some(x) => x
            .parse::<i32>()
            .expect("Layers per phase must be an integer"),
        None => 1,
    };
    hybrid_bfs(node_id, nodes, leader, layers_per_phase);
}
