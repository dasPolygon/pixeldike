extern crate spmc;

use std::thread::JoinHandle;
use std::sync::{Mutex, Arc, mpsc};
use std::thread;
use std::net::*;
use std::io::{BufReader, BufRead};


use crate::command_handler;
use self::command_handler::Command;


pub fn start(map: Vec<Vec<Arc<Mutex<String>>>>, port: u16, forward_tx: spmc::Sender<String>) -> JoinHandle<()> {
    print!("Starting TCP PX server on port {}...", &port);
    let socket = setup_socket(port);
    println!("done");

    thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let _input_handler = start_input_handler(map, rx, forward_tx);
        loop_server(socket, tx);
    })
}

fn setup_socket(port: u16) -> TcpListener {
    let address = SocketAddr::new(IpAddr::from(Ipv4Addr::new(0, 0, 0, 0)), port);
    TcpListener::bind(address).expect("Could not bind TCP socket")
}

fn start_input_handler(map: Vec<Vec<Arc<Mutex<String>>>>,
                       input_rx: mpsc::Receiver<Vec<u8>>,
                       forward_tx: spmc::Sender<String>) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            // Receive input from other channels
            let  buf= input_rx.recv().expect("All senders to input_handler have closed");
            // Decode buffer into string
            if let Ok(msg) = String::from_utf8(buf) {

                // Parse command from string
                if let Ok(cmd) = command_handler::parse_message(msg.clone()) {

                    // Execute correct command
                    let _answer = match cmd {
                        Command::SIZE => command_handler::cmd_size(),
                        Command::PX(x, y, color) => command_handler::cmd_px(&map, x, y, color)
                    };

                    //println!("{}", _answer);

                    // Forward pixel command
                    forward_tx.send(msg).expect("Could not forward PX command to websockets");

                }

            }
        }
    })
}

fn loop_server(socket: TcpListener, tx: mpsc::Sender<Vec<u8>>) {
    loop {
        match socket.accept() {
            Ok((stream, addr)) => {
                handle_client(stream, addr, tx.clone());
            }
            Err(e) => println!("Error: Couldn't get client: {:?}", e)
        }
    }
}

fn handle_client(stream: TcpStream, addr: SocketAddr, tx: mpsc::Sender<Vec<u8>>) -> JoinHandle<()> {
    println!("New PX TCP client: {:?}", addr);
    let mut reader = BufReader::new(stream);

    thread::spawn(move || {
        loop {
            match receive_msg(&mut reader) {
                Ok(msg) => tx.send(msg).expect("Could not send received string to input_handler"),
                Err(e) => {
                    println!("Error receiving from PX client {:?}: {}", addr, e);
                    break;
                }
            }
        }
    })
}

fn receive_msg(reader: &mut BufReader<TcpStream>) -> Result<Vec<u8>, String> {
    // Receive bytes from input stream
    let mut buf = Vec::new();
    let acm = reader.read_until(";".as_bytes()[0], &mut buf);
    if let Ok(acm) = acm {

        // If read() returns without having read any bytes, the stream seems to be closed
        if acm == 0 {
            return Err("Received 0 bytes from peer".to_string());
        }

        return Ok(buf);
    } else {
        return Err(acm.unwrap_err().to_string());
    }
}