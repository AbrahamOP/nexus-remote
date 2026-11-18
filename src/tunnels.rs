//! SSH TCP forwarding. All SSH and forwarded traffic lives on one worker thread;
//! no forwarding listener or network operation runs inside an egui frame.
use crate::{
    model::{Profile, Protocol},
    net::{self, TrustBroker},
};
use serde::{Deserialize, Serialize};
use ssh2::{Channel, ErrorCode};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

const MAX_STREAMS: usize = 32;
const MAX_BUFFER: usize = 256 * 1024;
const IO_CHUNK: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunnelKind {
    #[default]
    Local,
    Remote,
}
impl TunnelKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local · SSH -L",
            Self::Remote => "Distant · SSH -R",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelRule {
    pub id: String,
    pub name: String,
    pub profile_id: String,
    /// Listening address on this PC for Local; on the SSH server for Remote.
    pub local_host: String,
    /// Listening port on this PC for Local; on the SSH server for Remote.
    pub local_port: u16,
    pub destination_host: String,
    pub destination_port: u16,
    pub kind: TunnelKind,
}
impl Default for TunnelRule {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            profile_id: String::new(),
            local_host: "127.0.0.1".into(),
            local_port: 8080,
            destination_host: "127.0.0.1".into(),
            destination_port: 80,
            kind: TunnelKind::Local,
        }
    }
}
impl TunnelRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 100
            || self.profile_id.is_empty()
            || self.profile_id.len() > 100
        {
            return Err("Sélectionnez une connexion SSH pour ce tunnel.".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 200
            || self.name.chars().any(char::is_control)
        {
            return Err("Donnez un nom au tunnel (200 caractères maximum).".into());
        }
        loopback_ip(&self.local_host)?;
        if self.local_port == 0 || self.destination_port == 0 {
            return Err("Les ports doivent être compris entre 1 et 65535.".into());
        }
        if self.destination_host.is_empty()
            || self.destination_host.len() > 253
            || self
                .destination_host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '\\' | '"'))
        {
            return Err("Indiquez une adresse de destination, sans protocole ni port.".into());
        }
        Ok(())
    }
    pub fn description(&self) -> String {
        let side = if self.kind == TunnelKind::Local {
            "Ce PC"
        } else {
            "Serveur SSH"
        };
        format!(
            "{side} {}:{} → {}:{}",
            self.local_host, self.local_port, self.destination_host, self.destination_port
        )
    }
}

fn loopback_ip(host: &str) -> Result<IpAddr, String> {
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(Ipv4Addr::LOCALHOST.into());
    }
    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .ok()
        .filter(IpAddr::is_loopback)
        .ok_or_else(|| {
            "L’écoute est limitée à localhost, 127.0.0.1 ou ::1 dans cette version.".into()
        })
}

enum Update {
    Status(String),
    Finished(String),
}

pub struct TunnelSession {
    id: String,
    cancel: Arc<AtomicBool>,
    updates: Receiver<Update>,
    status: String,
    running: bool,
}
impl TunnelSession {
    pub fn new(
        profile: Profile,
        secret: String,
        rule: TunnelRule,
        trust: TrustBroker,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let secret = Zeroizing::new(secret);
        rule.validate()?;
        profile.validate()?;
        if profile.protocol == Protocol::Rdp || rule.profile_id != profile.id {
            return Err("Le tunnel doit utiliser la connexion SSH sélectionnée.".into());
        }
        // Reserving a local port is immediate and reports bind failures before a
        // worker or SSH session is created. No network connection is made here.
        let local_listener = if rule.kind == TunnelKind::Local {
            let listener = TcpListener::bind(SocketAddr::new(
                loopback_ip(&rule.local_host)?,
                rule.local_port,
            ))
            .map_err(|e| format!("Le port local {} est indisponible : {e}", rule.local_port))?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            Some(listener)
        } else {
            None
        };
        let id = rule.id.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, updates) = mpsc::channel();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker(
                    profile,
                    secret,
                    rule,
                    trust,
                    &worker_cancel,
                    local_listener,
                    &sender,
                    &ctx,
                )
            }));
            let message = if worker_cancel.load(Ordering::Relaxed) {
                "Tunnel arrêté.".into()
            } else {
                match result {
                    Ok(Ok(())) => "Tunnel fermé.".into(),
                    Ok(Err(e)) => e,
                    Err(_) => "Le tunnel a été interrompu. Arrêtez-le puis relancez-le.".into(),
                }
            };
            let _ = sender.send(Update::Finished(message));
            ctx.request_repaint();
        });
        Ok(Self {
            id,
            cancel,
            updates,
            status: "Connexion SSH · vérification du serveur…".into(),
            running: true,
        })
    }
    pub fn poll(&mut self) {
        for _ in 0..32 {
            match self.updates.try_recv() {
                Ok(Update::Status(message)) => self.status = message,
                Ok(Update::Finished(message)) => {
                    self.status = message;
                    self.running = false;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.running {
                        self.status = "Tunnel fermé.".into();
                        self.running = false;
                    }
                    break;
                }
            }
        }
    }
    pub fn rule_id(&self) -> &str {
        &self.id
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.status = "Arrêt du tunnel…".into();
    }
}
impl Drop for TunnelSession {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn emit(sender: &Sender<Update>, ctx: &egui::Context, text: String) {
    let _ = sender.send(Update::Status(text));
    ctx.request_repaint();
}

struct PendingLocal {
    socket: TcpStream,
    origin: SocketAddr,
    since: Instant,
}

fn worker(
    profile: Profile,
    mut secret: Zeroizing<String>,
    rule: TunnelRule,
    trust: TrustBroker,
    cancel: &AtomicBool,
    local_listener: Option<TcpListener>,
    sender: &Sender<Update>,
    ctx: &egui::Context,
) -> Result<(), String> {
    let connection = net::connect(&profile, &secret, &trust, cancel);
    secret.zeroize();
    let session = connection?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut remote_listener = if rule.kind == TunnelKind::Remote {
        let bind = loopback_ip(&rule.local_host)?.to_string();
        let (listener, bound) = session
            .channel_forward_listen(rule.local_port, Some(&bind), Some(MAX_STREAMS as u32))
            .map_err(|e| format!("Le serveur a refusé l’écoute distante : {e}"))?;
        if bound != rule.local_port {
            return Err("Le serveur a ouvert un port inattendu. Le tunnel a été fermé.".into());
        }
        Some(listener)
    } else {
        None
    };
    let destination_addresses = if rule.kind == TunnelKind::Remote {
        let addresses: Vec<_> = (rule.destination_host.as_str(), rule.destination_port)
            .to_socket_addrs()
            .map_err(|e| format!("Destination locale introuvable : {e}"))?
            .collect();
        if addresses.is_empty() {
            return Err("Aucune adresse pour la destination locale.".into());
        }
        addresses
    } else {
        Vec::new()
    };
    session.set_blocking(false);
    let mut pairs: Vec<Forwarded> = Vec::new();
    let mut pending: Option<PendingLocal> = None;
    let mut total = 0u64;
    let mut summary = String::new();
    let mut last_problem: Option<String> = None;
    let mut report_time = Instant::now() - Duration::from_secs(1);
    let mut keepalive_time = Instant::now();
    while !cancel.load(Ordering::Relaxed) {
        let mut progressed = false;
        if pending.is_none() && pairs.len() < MAX_STREAMS {
            if let Some(listener) = &local_listener {
                match listener.accept() {
                    Ok((socket, origin)) => {
                        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
                        let _ = socket.set_nodelay(true);
                        pending = Some(PendingLocal {
                            socket,
                            origin,
                            since: Instant::now(),
                        });
                        progressed = true;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(format!("Écoute locale interrompue : {e}")),
                }
            }
        }
        if let Some(waiting) = &pending {
            if waiting.since.elapsed() > Duration::from_secs(20) {
                // An unfinished libssh2 open request must not be reassigned to
                // the next accepted TCP connection. Close this SSH transport.
                return Err(
                    "Ouverture du canal SSH trop lente. Arrêtez puis relancez le tunnel.".into(),
                );
            } else {
                match session.channel_direct_tcpip(
                    &rule.destination_host,
                    rule.destination_port,
                    Some((&waiting.origin.ip().to_string(), waiting.origin.port())),
                ) {
                    Ok(channel) => {
                        let waiting = pending.take().unwrap();
                        pairs.push(Forwarded::new(waiting.socket, channel));
                        total += 1;
                        progressed = true;
                        last_problem = None;
                    }
                    Err(e) if e.code() == ErrorCode::Session(-37) => {}
                    Err(e) => {
                        pending.take();
                        last_problem = Some(format!("Destination refusée : {e}"));
                    }
                }
            }
        }
        if pairs.len() < MAX_STREAMS {
            if let Some(listener) = &mut remote_listener {
                match listener.accept() {
                    Ok(channel) => match connect_destination(&destination_addresses, cancel) {
                        Ok(socket) => {
                            pairs.push(Forwarded::new(socket, channel));
                            total += 1;
                            progressed = true;
                            last_problem = None;
                        }
                        Err(e) => {
                            drop(channel);
                            last_problem = Some(e);
                        }
                    },
                    Err(e) if e.code() == ErrorCode::Session(-37) => {}
                    Err(e) => return Err(format!("Écoute distante interrompue : {e}")),
                }
            }
        }
        let mut index = 0;
        while index < pairs.len() {
            match pairs[index].pump() {
                Ok((activity, done)) => {
                    progressed |= activity;
                    if done {
                        pairs.swap_remove(index);
                    } else {
                        index += 1;
                    }
                }
                Err(e) => {
                    pairs.swap_remove(index);
                    last_problem = Some(format!("Connexion transférée fermée : {e}"));
                }
            }
        }
        if report_time.elapsed() >= Duration::from_millis(500) {
            let next = if let Some(problem) = &last_problem {
                format!("Actif · {problem}")
            } else {
                format!("Actif · {} connexion(s) · {total} traitée(s)", pairs.len())
            };
            if next != summary {
                emit(sender, ctx, next.clone());
                summary = next;
            }
            report_time = Instant::now();
        }
        if keepalive_time.elapsed() >= Duration::from_secs(20) {
            let _ = session.keepalive_send();
            keepalive_time = Instant::now();
        }
        if !progressed {
            thread::sleep(Duration::from_millis(10));
        }
    }
    drop(pairs);
    drop(pending);
    drop(remote_listener);
    let _ = session.disconnect(None, "Tunnel closed", None);
    Ok(())
}

fn connect_destination(addresses: &[SocketAddr], cancel: &AtomicBool) -> Result<TcpStream, String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut reason = "aucune adresse".to_string();
    for address in addresses {
        if cancel.load(Ordering::Relaxed) {
            return Err("Tunnel arrêté.".into());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(address, remaining.min(Duration::from_millis(500))) {
            Ok(socket) => {
                socket.set_nonblocking(true).map_err(|e| e.to_string())?;
                let _ = socket.set_nodelay(true);
                return Ok(socket);
            }
            Err(e) => reason = e.to_string(),
        }
    }
    Err(format!("Destination locale indisponible : {reason}"))
}

#[derive(Default)]
struct Buffer {
    data: Vec<u8>,
    offset: usize,
}
impl Buffer {
    fn len(&self) -> usize {
        self.data.len() - self.offset
    }
    fn bytes(&self) -> &[u8] {
        &self.data[self.offset..]
    }
    fn append(&mut self, bytes: &[u8]) {
        if self.offset > 0 {
            self.data.drain(..self.offset);
            self.offset = 0;
        }
        self.data.extend_from_slice(bytes);
    }
    fn consume(&mut self, count: usize) {
        self.offset += count;
        if self.offset == self.data.len() {
            self.data.clear();
            self.offset = 0;
        }
    }
}

struct Forwarded {
    socket: TcpStream,
    channel: Channel,
    to_ssh: Buffer,
    to_socket: Buffer,
    socket_eof: bool,
    channel_eof: bool,
    eof_sent: bool,
    socket_shutdown: bool,
    activity: Instant,
}
impl Forwarded {
    fn new(socket: TcpStream, channel: Channel) -> Self {
        Self {
            socket,
            channel,
            to_ssh: Buffer::default(),
            to_socket: Buffer::default(),
            socket_eof: false,
            channel_eof: false,
            eof_sent: false,
            socket_shutdown: false,
            activity: Instant::now(),
        }
    }
    fn pump(&mut self) -> Result<(bool, bool), String> {
        let mut progressed = false;
        let mut buffer = [0u8; IO_CHUNK];
        // libssh2 requires the exact same write buffer until an EAGAIN write
        // completes. Do not append/compact it while a previous chunk is pending.
        if !self.socket_eof && self.to_ssh.len() == 0 {
            let size = (MAX_BUFFER - self.to_ssh.len()).min(buffer.len());
            match self.socket.read(&mut buffer[..size]) {
                Ok(0) => {
                    self.socket_eof = true;
                    progressed = true;
                }
                Ok(n) => {
                    self.to_ssh.append(&buffer[..n]);
                    progressed = true;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if self.to_ssh.len() > 0 {
            match self.channel.write(self.to_ssh.bytes()) {
                Ok(0) => return Err("Le canal SSH a été fermé.".into()),
                Ok(n) => {
                    self.to_ssh.consume(n);
                    progressed = true;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if self.socket_eof && self.to_ssh.len() == 0 && !self.eof_sent {
            match self.channel.send_eof() {
                Ok(()) => {
                    self.eof_sent = true;
                    progressed = true;
                }
                Err(e) if e.code() == ErrorCode::Session(-37) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if !self.channel_eof && self.to_socket.len() < MAX_BUFFER {
            let size = (MAX_BUFFER - self.to_socket.len()).min(buffer.len());
            match self.channel.read(&mut buffer[..size]) {
                Ok(0) if self.channel.eof() => {
                    self.channel_eof = true;
                    progressed = true;
                }
                Ok(0) => {}
                Ok(n) => {
                    self.to_socket.append(&buffer[..n]);
                    progressed = true;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if self.to_socket.len() > 0 {
            match self.socket.write(self.to_socket.bytes()) {
                Ok(0) => return Err("La destination TCP a été fermée.".into()),
                Ok(n) => {
                    self.to_socket.consume(n);
                    progressed = true;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if self.channel_eof && self.to_socket.len() == 0 && !self.socket_shutdown {
            let _ = self.socket.shutdown(Shutdown::Write);
            self.socket_shutdown = true;
            progressed = true;
        }
        if progressed {
            self.activity = Instant::now();
        }
        let done = (self.socket_eof
            && self.channel_eof
            && self.to_ssh.len() == 0
            && self.to_socket.len() == 0)
            || self.activity.elapsed() > Duration::from_secs(300);
        Ok((progressed, done))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rule() -> TunnelRule {
        TunnelRule {
            name: "Database".into(),
            profile_id: "test-profile".into(),
            ..Default::default()
        }
    }
    #[test]
    fn forwarding_rejects_public_listeners_and_invalid_destinations() {
        let mut r = rule();
        assert!(r.validate().is_ok());
        for bind in ["0.0.0.0", "::", "192.0.2.1", "localhost.attacker.example"] {
            r.local_host = bind.into();
            assert!(r.validate().is_err());
        }
        r.local_host = "::1".into();
        assert!(r.validate().is_ok());
        r.destination_host = "server\r\nInjected".into();
        assert!(r.validate().is_err());
        r.destination_host = "127.0.0.1".into();
        r.local_port = 0;
        assert!(r.validate().is_err());
    }
    #[test]
    fn buffers_preserve_partial_writes_and_bound_live_data() {
        let mut b = Buffer::default();
        b.append(b"abcdef");
        b.consume(2);
        assert_eq!(b.bytes(), b"cdef");
        b.append(b"ghi");
        assert_eq!(b.bytes(), b"cdefghi");
        b.consume(7);
        assert_eq!(b.len(), 0);
        b.append(b"next");
        assert_eq!(b.bytes(), b"next");
    }
    #[test]
    fn cancelled_destination_does_not_open_a_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        assert!(
            connect_destination(&[listener.local_addr().unwrap()], &AtomicBool::new(true)).is_err()
        );
        assert!(listener.accept().is_err());
    }

    fn unused_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    struct EchoServer {
        port: u16,
        cancel: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl EchoServer {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            listener.set_nonblocking(true).unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let thread_cancel = cancel.clone();
            let worker = thread::spawn(move || {
                let mut clients = Vec::new();
                while !thread_cancel.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            clients.push(thread::spawn(move || {
                                // Windows accepts inherit the listener's
                                // nonblocking mode; this echo worker is blocking.
                                stream.set_nonblocking(false).unwrap();
                                stream
                                    .set_read_timeout(Some(Duration::from_secs(5)))
                                    .unwrap();
                                stream
                                    .set_write_timeout(Some(Duration::from_secs(5)))
                                    .unwrap();
                                let mut bytes = [0u8; 8192];
                                loop {
                                    match stream.read(&mut bytes) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            if stream.write_all(&bytes[..n]).is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                            }));
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => break,
                    }
                }
                for client in clients {
                    let _ = client.join();
                }
            });
            Self {
                port,
                cancel,
                worker: Some(worker),
            }
        }
    }
    impl Drop for EchoServer {
        fn drop(&mut self) {
            self.cancel.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    #[test]
    fn echo_fixture_half_close_roundtrip() {
        let echo = EchoServer::new();
        let mut stream = TcpStream::connect(("127.0.0.1", echo.port)).unwrap();
        let data = vec![42u8; 200_000];
        stream.write_all(&data).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut result = Vec::new();
        stream.read_to_end(&mut result).unwrap();
        assert_eq!(result.len(), data.len());
    }

    fn real_loopback_roundtrip(kind: TunnelKind) {
        let ssh_port = std::env::var("NEXUS_TEST_PORT")
            .expect("Explicit localhost fixture required")
            .parse::<u16>()
            .unwrap();
        let expected =
            std::env::var("NEXUS_TEST_FINGERPRINT").expect("Pinned fixture fingerprint required");
        assert!(ssh_port > 1024);
        let echo = EchoServer::new();
        let forward_port = unused_port();
        let profile = Profile {
            id: "test-profile".into(),
            name: "Generated localhost forwarding fixture".into(),
            host: "127.0.0.1".into(),
            port: ssh_port,
            username: "testuser".into(),
            ..Default::default()
        };
        let rule = TunnelRule {
            name: "Loopback roundtrip".into(),
            profile_id: profile.id.clone(),
            local_port: forward_port,
            destination_port: echo.port,
            kind,
            ..Default::default()
        };
        let (requests, rx) = mpsc::channel::<net::HostKeyRequest>();
        let trust = thread::spawn(move || {
            let request = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("No SSH identity request");
            assert_eq!(request.endpoint, format!("[127.0.0.1]:{ssh_port}"));
            assert_eq!(request.fingerprint, expected);
            request.reply.send(true).unwrap();
        });
        let mut tunnel = TunnelSession::new(
            profile,
            "fixture-password".into(),
            rule,
            TrustBroker { requests },
            egui::Context::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            tunnel.poll();
            if tunnel.status().starts_with("Actif") {
                break;
            }
            assert!(tunnel.is_running(), "{}", tunnel.status());
            thread::sleep(Duration::from_millis(10));
        }
        assert!(tunnel.status().starts_with("Actif"), "{}", tunnel.status());
        trust.join().unwrap();
        let clients: Vec<_> = (0..3)
            .map(|client| {
                thread::spawn(move || {
                    let mut stream = TcpStream::connect(("127.0.0.1", forward_port)).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(10)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(10)))
                        .unwrap();
                    let data: Vec<u8> = (0..200_000)
                        .map(|index| ((index + client) % 251) as u8)
                        .collect();
                    stream.write_all(&data).unwrap();
                    stream.shutdown(Shutdown::Write).unwrap();
                    let mut echoed = Vec::new();
                    stream.read_to_end(&mut echoed).unwrap();
                    use sha2::{Digest, Sha256};
                    (
                        echoed.len(),
                        data.len(),
                        Sha256::digest(&echoed) == Sha256::digest(&data),
                    )
                })
            })
            .collect();
        let results: Vec<_> = clients
            .into_iter()
            .map(|client| client.join().unwrap())
            .collect();
        thread::sleep(Duration::from_millis(520));
        tunnel.poll();
        assert!(
            results
                .iter()
                .all(|(received, expected, same)| received == expected && *same),
            "Forwarded roundtrip lengths/hash mismatch: {results:?}, {}",
            tunnel.status()
        );
        tunnel.stop();
        let stopped = Instant::now() + Duration::from_secs(3);
        while Instant::now() < stopped {
            tunnel.poll();
            if !tunnel.is_running() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!tunnel.is_running(), "{}", tunnel.status());
        let closed = Instant::now() + Duration::from_secs(3);
        while Instant::now() < closed {
            if TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], forward_port)),
                Duration::from_millis(100),
            )
            .is_err()
            {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("Forwarded listener was not released after stopping");
    }
    #[test]
    #[ignore = "Requires explicitly started localhost SSH fixture with forwarding support"]
    fn loopback_local_forwarding_real_concurrent_transfer_and_stop() {
        real_loopback_roundtrip(TunnelKind::Local);
    }
    #[test]
    #[ignore = "Requires explicitly started localhost SSH fixture with reverse forwarding support"]
    fn loopback_reverse_forwarding_real_concurrent_transfer_and_stop() {
        real_loopback_roundtrip(TunnelKind::Remote);
    }
}
