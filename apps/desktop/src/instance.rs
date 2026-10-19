//! One running copy of the app.
//!
//! The first copy listens on a loopback port. A second copy connects, sends a
//! greeting, waits for the same greeting back, and stops. The first copy then
//! brings its window forward. The greeting makes sure that the other side of the
//! port is this app, and that it runs for the same user. The port is shared by
//! all users of the machine, so the user name is part of the greeting.
//!
//! shortcut: a fixed port, not a lock file or a named pipe. If another program
//! holds the port, the guard is off and the app starts as before.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const ADDR: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 47_653);
const WAIT: Duration = Duration::from_secs(1);

fn hello() -> Vec<u8> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    format!("garmin-companion {user}\r\n").into_bytes()
}

pub enum Claim {
    /// This copy runs. A message arrives each time a second copy starts.
    First(async_channel::Receiver<()>),
    /// A copy of the app runs already. It has been told to show its window.
    Second,
    /// The guard is not available. Start as if no other copy ran.
    Off,
}

pub fn claim() -> Claim {
    match TcpListener::bind(ADDR) {
        Ok(listener) => {
            let (tx, rx) = async_channel::unbounded();
            std::thread::spawn(move || serve(listener, tx));
            Claim::First(rx)
        }
        Err(_) if tell_first() => Claim::Second,
        Err(_) => Claim::Off,
    }
}

fn serve(listener: TcpListener, tx: async_channel::Sender<()>) {
    for mut stream in listener.incoming().flatten() {
        let _ = stream.set_read_timeout(Some(WAIT));
        let expected = hello();
        let mut got = vec![0u8; expected.len()];
        let greeted = stream.read_exact(&mut got).is_ok() && got == expected;
        if greeted && stream.write_all(&expected).is_ok() && tx.send_blocking(()).is_err() {
            return;
        }
    }
}

fn tell_first() -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&ADDR, WAIT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(WAIT));
    let hello = hello();
    let mut reply = vec![0u8; hello.len()];
    stream.write_all(&hello).is_ok() && stream.read_exact(&mut reply).is_ok() && reply == hello
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_greeting_reaches_the_listener_and_a_stranger_does_not() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = async_channel::unbounded();
        std::thread::spawn(move || serve(listener, tx));

        let mut stranger = TcpStream::connect(addr).unwrap();
        stranger.write_all(&vec![b'x'; hello().len()]).unwrap();
        let mut sink = Vec::new();
        let _ = stranger.read_to_end(&mut sink);
        assert!(rx.try_recv().is_err());

        let mut friend = TcpStream::connect(addr).unwrap();
        friend.write_all(&hello()).unwrap();
        let mut reply = vec![0u8; hello().len()];
        friend.read_exact(&mut reply).unwrap();
        assert_eq!(reply, hello());
        assert!(rx.recv_blocking().is_ok());
    }
}
