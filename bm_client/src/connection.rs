use channel::{ConstrainedReceiver,ConstrainedSender,constrained_channel};
use message::{Message,ParseError,read_message,write_message};
use net::to_socket_addr;
use std::io::{Error,Write};
use std::net::{Shutdown,SocketAddr,TcpStream};
use std::sync::{Arc,RwLock};
use std::sync::mpsc::{Receiver,SyncSender,TryRecvError,sync_channel};
use std::thread::{Builder,JoinHandle,sleep_ms};
use time::{Duration,Timespec,get_time};

const MAX_WRITE_BUFFER: usize = 20_000_000;

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum ConnectionState {
    Fresh(Timespec),
    GotVerackAwaitingVersion(Timespec),
    GotVersionAwaitingVerack(Timespec),
    Established(Timespec),
    Stale,
    Error
}

pub type State = Arc<RwLock<ConnectionState>>;

pub struct Connection {
    pub state: State,
    tcp_stream: Option<TcpStream>
}

impl Connection {
    pub fn new(socket_addr: SocketAddr, nonce: u64) -> Connection {
        match TcpStream::connect(&socket_addr) {
            Ok(tcp_stream) => new_from_stream(tcp_stream, nonce),
            Err(_) => error_connection(None)
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        for tcp_stream in self.tcp_stream.iter() {
            let _ = tcp_stream.shutdown(Shutdown::Both);
        }
    }
}

fn new_from_stream(tcp_stream: TcpStream, nonce: u64) -> Connection {
    let socket_addr = tcp_stream.peer_addr().unwrap();
    let state = Arc::new(RwLock::new(ConnectionState::Fresh(get_time())));

    // Make channels for thread communication
    let (read_state_tx, read_state_rx) = sync_channel(0);
    let (state_response_tx, state_response_rx) = constrained_channel(MAX_WRITE_BUFFER);
    let (response_write_tx, response_write_rx) = sync_channel(0);

    // Make thread to read messages from the peer
    let read_name = format!("Connection {} - read", socket_addr);
    let read_thread = create_read_thread(read_name, &tcp_stream, read_state_tx);

    // Make thread to manage the state of this connnection
    let state_name = format!("Connection {} - state", socket_addr);
    let state_thread = create_state_thread(state_name, state.clone(), read_state_rx, state_response_tx);

    // Make thread to create appropriate response messages
    let response_name = format!("Connection {} - response", socket_addr);
    let response_thread = create_response_thread(response_name, socket_addr, nonce, state_response_rx, response_write_tx);

    // Make thread to write messages to the peer
    let write_name = format!("Connection {} - write", socket_addr);
    let write_thread = create_write_thread(write_name, &tcp_stream, response_write_rx);

    if read_thread.is_err() || state_thread.is_err() || response_thread.is_err() || write_thread.is_err() {
        return error_connection(Some(tcp_stream));
    }

    Connection {
        state: state,
        tcp_stream: Some(tcp_stream)
    }
}

fn error_connection(tcp_stream: Option<TcpStream>) -> Connection {
    Connection {
        state: Arc::new(RwLock::new(ConnectionState::Error)),
        tcp_stream: tcp_stream
    }
}

fn create_read_thread(name: String, borrowed_stream: &TcpStream, state_chan: SyncSender<Result<Message,ParseError>>) -> Result<JoinHandle<()>,Error> {
    let mut stream = borrowed_stream.try_clone().unwrap();
    Builder::new().name(name).spawn(move || {
        loop {
            let message: Result<Message,ParseError> = read_message(&mut stream);
            let parse_error = message.is_err();

            state_chan.send(message).unwrap();

            if parse_error {
                break;
            }
        }
    })
}

fn create_state_thread(name: String, state: State, read_chan: Receiver<Result<Message,ParseError>>, response_chan: ConstrainedSender<Message>) -> Result<JoinHandle<()>,Error> {
    Builder::new().name(name).spawn(move || {
        loop {
            let current_state = get_state(&state);

            let (new_state, forward_messages) = match (current_state, read_chan.try_recv()) {
                (_, Err(TryRecvError::Empty)) => (current_state, vec![]),
                (_, Err(TryRecvError::Disconnected)) => (ConnectionState::Error, vec![]),
                (_, Ok(Err(_))) => (ConnectionState::Error, vec![]),
                (ConnectionState::Fresh(_), Ok(Ok(m @ Message::Version {..}))) => (ConnectionState::GotVersionAwaitingVerack(get_time()), vec![ m ]),
                (ConnectionState::Fresh(_), Ok(Ok(Message::Verack))) => (ConnectionState::GotVerackAwaitingVersion(get_time()), vec![]),
                (ConnectionState::Fresh(_), Ok(Ok(_))) => (ConnectionState::Error, vec![]),
                (ConnectionState::GotVersionAwaitingVerack(_), Ok(Ok(m @ Message::Verack))) => (ConnectionState::Established(get_time()), vec![ m ]),
                (ConnectionState::GotVersionAwaitingVerack(_), Ok(Ok(_))) => (ConnectionState::Error, vec![]),
                (ConnectionState::GotVerackAwaitingVersion(_), Ok(Ok(m @ Message::Version{..}))) => (ConnectionState::Established(get_time()), vec![ m ]),
                (ConnectionState::GotVerackAwaitingVersion(_), Ok(Ok(_))) => (ConnectionState::Error, vec![]),
                (ConnectionState::Established(_), Ok(Ok(m))) => (ConnectionState::Established(get_time()), vec![ m ]),
                (_, Ok(Ok(_))) => (current_state, vec![])
            };

            set_state(&state, new_state);
            for forward_message in forward_messages.into_iter() {
                response_chan.send(forward_message).unwrap();
            }

            if new_state == ConnectionState::Error {
                break;
            }

            match current_state {
                ConnectionState::Fresh(time) => {
                    let now = get_time();
                    if now > time + Duration::seconds(20) {
                        set_state(&state, ConnectionState::Stale);
                    }
                },
                ConnectionState::GotVersionAwaitingVerack(time) => {
                    let now = get_time();
                    if now > time + Duration::seconds(20) {
                        set_state(&state, ConnectionState::Stale);
                    }
                },
                ConnectionState::GotVerackAwaitingVersion(time) => {
                    let now = get_time();
                    if now > time + Duration::seconds(20) {
                        set_state(&state, ConnectionState::Stale);
                    }
                },
                ConnectionState::Established(time) => {
                    let now = get_time();
                    if now > time + Duration::minutes(10) {
                        set_state(&state, ConnectionState::Stale)
                    }
                },
                _ => {}
            }

            sleep_ms(100);
        }
    })
}

fn get_state(state: &State) -> ConnectionState {
    *state.read().unwrap()
}

fn set_state(state: &State, new_value: ConnectionState) {
    let mut guard = state.write().unwrap();
    *guard = new_value;
}

fn create_response_thread(name: String, socket_addr: SocketAddr, nonce: u64, state_chan: ConstrainedReceiver<Message>, write_chan: SyncSender<Message>) -> Result<JoinHandle<()>,Error> {
    Builder::new().name(name).spawn(move || {
        if let Err(_) = write_chan.send(create_version_message(socket_addr, nonce)) {
            return;
        }

        loop {
            let message = match state_chan.recv() {
                Ok(m) => m,
                Err(_) => break
            };

            match message {
                Message::Version { .. } => {
                    if let Err(_) = write_chan.send(Message::Verack) {
                        break;
                    }
                },
                Message::Verack => {
//                     create addr_message
//                     create inv messages
                },
                Message::Addr { .. } => {},
                Message::Inv { .. } => {
//                    create_filtered_getdata_message
                },
                Message::GetData { .. } => {
//                    create object messages
                },
                Message::Object { .. } => {}
            };
        }
    })
}

fn create_version_message(peer_addr: SocketAddr, nonce: u64) -> Message {
    let our_addr = to_socket_addr("127.0.0.1:8555");
    let user_agent = "Rubbem".to_string();
    let streams = vec![ 1 ];

    Message::Version {
        version: 3,
        services: 1,
        timestamp: get_time(),
        addr_recv: peer_addr,
        addr_from: our_addr,
        nonce: nonce,
        user_agent: user_agent,
        streams: streams
    }
}

fn create_write_thread(name: String, borrowed_stream: &TcpStream, response_chan: Receiver<Message>) -> Result<JoinHandle<()>,Error> {
    let mut stream = borrowed_stream.try_clone().unwrap();
    Builder::new().name(name).spawn(move || {
        loop {
            let message = match response_chan.recv() {
                Ok(m) => m,
                Err(_) => break
            };

            let mut message_bytes = vec![];
            write_message(&mut message_bytes, &message);

            if let Err(_) = stream.write_all(&message_bytes) {
                break;
            }
        }
    })
}