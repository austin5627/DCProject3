extern crate owo_colors;
extern crate serde;

use std::collections::HashMap;
use std::env::args;
use std::sync::{Arc, Mutex};
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
        .filter(|line| line.starts_with(|c: char| c.is_digit(10) || c == '('));

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

    return (nodes, leader.expect("Leader not found"));
}
fn handle_message_layered_bfs(node: &mut Node, msg: Message, id: i32) -> bool {
    match msg {
        Message::Search(layer) => {
            if node.free {
                node.free = false;
                node.layer = layer + 1;
                node.parent = Some(id);
                if !node.send(id, Message::Ack) {
                    return true;
                }
            } else {
                if !node.send(id, Message::Reject) {
                    return true;
                }
            }
        }
        Message::Ack | Message::Reject => {
            node.responses_received.insert(id, msg == Message::Ack);
            if msg == Message::Ack {
                node.children.push(id);
            }
            if node.responses_received.len() == node.neighbors.len() - 1 {
                if let Some(parent) = node.parent {
                    let child_added = node.responses_received.values().any(|&x| x);
                    if !node.send(parent, Message::PhaseComplete(child_added)) {
                        return true;
                    }
                } else {
                    if !node.broadcast_tree(Message::NewPhase(node.layer + 1)) {
                        return true;
                    }
                }
                node.responses_received.clear();
            }
        }
        Message::NewPhase(layer) => {
            if node.layer == layer {
                if !node.broadcast(Message::Search(layer)) {
                    return true;
                }
            } else if !node.children.is_empty() {
                if !node.broadcast_tree(Message::NewPhase(layer)) {
                    return true;
                }
            } else if let Some(parent) = node.parent{
                node.send(parent, Message::PhaseComplete(false));
            }
        }
        Message::PhaseComplete(child_added) => {
            node.responses_received.insert(id, child_added);
            if node.responses_received.len() == node.children.len() {
                let child_added = node.responses_received.values().any(|&x| x);
                if let Some(parent) = node.parent {
                    if !node.send(parent, Message::PhaseComplete(child_added)) {
                        return true;
                    }
                } else {
                    if child_added {
                        node.layer += 1;
                        if !node.broadcast_tree(Message::NewPhase(node.layer)) {
                            return true;
                        }
                    } else {
                        node.broadcast(Message::Terminate);
                        return true;
                    }
                }
                node.responses_received.clear();
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
    debugln!("{:?}", node);

    return false;
}

fn layered_bfs(node_id: i32, nodes: HashMap<i32, NodeData>, leader: i32) {
    let mut n = Node::new(node_id, nodes, leader);
    if node_id == leader {
        n.broadcast(Message::Search(0));
    }
    let mut threads = Vec::new();
    let neighbors = n.neighbors.clone();
    let node = Arc::new(Mutex::new(n));
    for i in 0..neighbors.len() {
        let node = node.clone();
        let id = neighbors[i];
        let mut listener = node
            .lock()
            .expect("Cannot get lock")
            .listeners
            .get_mut(&id)
            .expect("Listener not found")
            .try_clone()
            .expect("Cannot clone listener");
        let t = thread::spawn(move || loop {
            let msg_op = recv_message(&mut listener);
            if let Some(msg) = msg_op {
                if msg != Message::Terminate {
                    debugln!("Node {} received {:?} from {}", node_id, msg, id);
                }
                let q = handle_message_layered_bfs(&mut node.lock().expect("Cannot get lock"), msg, id);
                if q {
                    break;
                }
            } else {
                node.lock()
                    .expect("Cannot get lock")
                    .broadcast(Message::Terminate);
                break;
            }
        });
        threads.push(t);
    }
    for t in threads {
        t.join().expect("Thread failed");
    }
    let node = node.lock().expect("Cannot get lock");
    println!("Node {} finished", node_id);
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
    if args.len() != 3 {
        println!("Usage: {} <config file> <node id>", args[0]);
        return;
    }
    let node_id = args[2].parse::<i32>().expect("Node id must be an integer");
    println!("Starting node {}...", node_id);

    let (nodes, leader) = read_config(&args[1]);
    layered_bfs(node_id, nodes, leader);
}
