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
use std::sync::atomic::{AtomicBool, Ordering};
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

/// The updater sets this in the new process. The old copy may still hold the
/// port for a moment, so the claim retries before it reads a busy port as a
/// copy that runs.
pub const RELAUNCH_ENV: &str = "GARMIN_COMPANION_RELAUNCHED";
const RETRIES: u32 = 10;
const RETRY_EVERY: Duration = Duration::from_millis(200);

/// True while the serve thread holds the listener.
static LISTENING: AtomicBool = AtomicBool::new(false);
static CLOSE: AtomicBool = AtomicBool::new(false);

pub fn claim() -> Claim {
    let tries = if std::env::var_os(RELAUNCH_ENV).is_some() {
        RETRIES
    } else {
        1
    };
    for attempt in 0..tries {
        if attempt > 0 {
            std::thread::sleep(RETRY_EVERY);
        }
        if let Ok(listener) = TcpListener::bind(ADDR) {
            let (tx, rx) = async_channel::unbounded();
            CLOSE.store(false, Ordering::SeqCst);
            LISTENING.store(true, Ordering::SeqCst);
            std::thread::spawn(move || serve(listener, tx));
            return Claim::First(rx);
        }
    }
    if tries == 1 && tell_first() {
        Claim::Second
    } else {
        Claim::Off
    }
}

/// Give up the port and wait until it is free (at most one second). The
/// updater calls this before it starts the new copy.
pub fn release() {
    if !LISTENING.load(Ordering::SeqCst) {
        return;
    }
    CLOSE.store(true, Ordering::SeqCst);
    // A connection wakes the accept call, so the serve thread sees the flag.
    let _ = TcpStream::connect_timeout(&ADDR, WAIT);
    for _ in 0..50 {
        if !LISTENING.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn serve(listener: TcpListener, tx: async_channel::Sender<()>) {
    serve_loop(&listener, &tx);
    drop(listener);
    LISTENING.store(false, Ordering::SeqCst);
}

fn serve_loop(listener: &TcpListener, tx: &async_channel::Sender<()>) {
    for mut stream in listener.incoming().flatten() {
        if CLOSE.load(Ordering::SeqCst) {
            return;
        }
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

        // The close flag stops the thread, and the port is free to bind again.
        // This stays in one test: the flag is shared.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, _rx) = async_channel::unbounded();
        let thread = std::thread::spawn(move || serve(listener, tx));
        CLOSE.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(addr);
        thread.join().unwrap();
        CLOSE.store(false, Ordering::SeqCst);
        TcpListener::bind(addr).unwrap();
    }
}
