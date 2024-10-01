// main.rs

use std::{
    collections::HashMap,
    env,
    fs::{File, OpenOptions},
    io::{stdout, Error, Read, Seek, SeekFrom, Write},
    net::*,
    ops::Mul,
    str::FromStr,
    thread,
    time::{Duration, SystemTime},
};

use time::{Date, PrimitiveDateTime, Time};

#[derive(Ord, Eq, PartialOrd, PartialEq)]
enum SafeReadWritePacket {
    Write,
    Ack,
    ResendRequest,
    End,
}
use SafeReadWritePacket::*;

struct SafeReadWrite {
    socket: UdpSocket,
    last_transmitted: HashMap<u16, Vec<u8>>,
    packet_count_out: u64,
    packet_count_in: u64,
}

struct Wrap<T>(T);

static VERSION: &str = env!("CARGO_PKG_VERSION");
static HELPER: &str = "tudbut.de:4277";

impl Mul<Wrap<&str>> for u64 {
    type Output = String;

    fn mul(self, rhs: Wrap<&str>) -> Self::Output {
        let strings: Vec<&str> = (0..self).map(|_| rhs.0).collect();
        strings.join("")
    }
}

impl SafeReadWrite {
    pub fn new(socket: UdpSocket) -> SafeReadWrite {
        SafeReadWrite {
            socket,
            last_transmitted: HashMap::new(),
            packet_count_in: 0,
            packet_count_out: 0,
        }
    }

    pub fn write_safe(&mut self, buf: &[u8], delay: u64) -> Result<(), Error> {
        self.write_flush_safe(buf, false, delay)
    }

    pub fn write_flush_safe(&mut self, buf: &[u8], flush: bool, delay: u64) -> Result<(), Error> {
        self.internal_write_safe(buf, Write, flush, false, delay)
    }

    pub fn read_safe(&mut self, buf: &[u8]) -> Result<(Vec<u8>, usize), Error> {
        if buf.len() > 0xfffc {
            panic!(
                "attempted to receive too large data packet with SafeReadWrite ({} > 0xfffc)",
                buf.len()
            );
        }

        let mut mbuf = Vec::from(buf);
        mbuf.insert(0, 0);
        mbuf.insert(0, 0);
        mbuf.insert(0, 0);
        let buf: &mut [u8] = mbuf.as_mut();

        let mut r = (vec![], 0);

        let mut try_again = true;
        let mut is_catching_up = false;
        while try_again {
            match self.socket.recv(buf) {
                Ok(x) => {
                    if x < 3 {
                        continue;
                    }
                    let id = u16::from_be_bytes([buf[0], buf[1]]);
                    if id <= self.packet_count_in as u16 {
                        self.socket
                            .send(&[buf[0], buf[1], Ack as u8])
                            .expect("Send error");
                    }
                    if id == self.packet_count_in as u16 {
                        if id == 0xffff {
                            println!("\nPacket ID wrap successful.");
                        }
                        try_again = false;
                        self.packet_count_in += 1;
                        r.1 = x - 3;
                    } else if id > self.packet_count_in as u16 && (id - self.packet_count_in as u16) < 0xC000 {
                        if !is_catching_up && !env::var("QFT_HIDE_DROPS").is_ok() {
                            println!("\r\x1b[KA packet dropped: {} (got) is newer than {} (expected)",
                                &id, &(self.packet_count_in as u16));
                        }
                        is_catching_up = true;
                        // Ask to resend, then do nothing
                        let id = (self.packet_count_in as u16).to_be_bytes();
                        self.socket
                            .send(&[id[0], id[1], ResendRequest as u8])
                            .expect("send error");
                    }
                    if buf[2] == End as u8 {
                        return Ok((vec![], 0));
                    }
                }
                Err(_) => {}
            }
        }
        mbuf.remove(0);
        mbuf.remove(0);
        mbuf.remove(0);
        r.0 = mbuf;
        return Ok(r);
    }

    pub fn end(mut self) -> UdpSocket {
        let _ = self.internal_write_safe(&mut [], End, true, true, 3000);

        self.socket
    }

    fn internal_write_safe(
        &mut self,
        buf: &[u8],
        packet: SafeReadWritePacket,
        flush: bool,
        exit_on_lost: bool,
        delay: u64,
    ) -> Result<(), Error> {
        if buf.len() > 0xfffc {
            panic!("Too large data packet sent over SafeReadWrite ({} > 0xfffc)", buf.len());
        }

        let id = (self.packet_count_out as u16).to_be_bytes();
        let idn = self.packet_count_out as u16;
        self.packet_count_out += 1;

        let mut vbuf = Vec::from(buf);
        vbuf.insert(0, packet as u8);
        vbuf.insert(0, id[1]);
        vbuf.insert(0, id[0]);  // This is now the first byte
        let buf = vbuf.as_slice();

        loop {
            match self.socket.send(buf) {
                Ok(x) => {
                    if x != buf.len() {
                        continue;
                    }
                }
                Err(_) => {
                    continue;
                }
            }
            thread::sleep(Duration::from_micros(delay));
            self.last_transmitted.insert(idn, vbuf);
            break;
        }
        let mut buf = [0, 0, 0];
        let mut wait = idn == 0xffff || flush;
        if self.last_transmitted.len() < 256 {
            self.socket
                .set_read_timeout(Some(Duration::from_millis(1)))
                .unwrap();
        } else {
            wait = true;
        }
        let mut start = unix_millis();
        if idn == 0xffff {
            print!("\nPacket ID needs to wrap. Waiting for partner to catch up...")
        }
        let mut is_catching_up = false;
        loop {
            match (
                if !wait {
                    self.socket.set_nonblocking(true).unwrap()
                } else {
                    ()
                },
                self.socket.recv(&mut buf).ok(),
                self.socket.set_nonblocking(false).unwrap(),
            ).1 {
                Some(x) => {
                    if x != 3 {
                        continue;
                    }
                    if buf[2] == Ack as u8 {
                        let n = u16::from_be_bytes([buf[0], buf[1]]);
                        self.last_transmitted.remove(&n);
                        if n == idn {
                            if idn == 0xffff {
                                println!("\r\x1b[KPacket ID wrap successful.");
                            }
                            wait = false;
                            // If the latest packet is ACK'd, all previous ones must be as well
                            self.last_transmitted.clear();
                        }
                    }
                    if buf[2] == ResendRequest as u8 {
                        let mut n = u16::from_be_bytes([buf[0], buf[1]]);
                        thread::sleep(Duration::from_millis(100));
                        while let Some(_) = self.socket.recv(&mut buf).ok() {}
                        if !is_catching_up && !env::var("QFT_HIDE_DROPS").is_ok() {
                            println!("\r\x1b[KA packet dropped: {}", &n);
                        }
                        if !is_catching_up {
                            wait = true;
                            is_catching_up = true;
                            while n <= idn && !(idn == 0xffff && n == 0) {
                                let buf = self.last_transmitted.get(&n);
                                if let Some(buf) = buf {
                                    loop { // Resend until success
                                        match self.socket.send(&buf.as_slice()) {
                                            Ok(x) => {
                                                if x != buf.len() {
                                                    continue;
                                                }
                                            }
                                            Err(_) => {
                                                continue;
                                            }
                                        };
                                        thread::sleep(Duration::from_millis(4));
                                        break;
                                    }
                                } else {
                                    break;
                                }
                                // Do NOT remove from last_transmitted yet, wait for Ack to do that
                                n += 1;
                            }
                        }
                    }
                }
                None => {
                    if unix_millis() - start > 5000 && exit_on_lost { // Check lost on exit after 5s
                        break;
                    }
                    if unix_millis() - start > 10000 { // Retry after 10s
                        println!("\n10s passed since last packet ==> Connection broken. Trying to resend packet...");
                        if let Some(buf) = self.last_transmitted.get(&idn) {
                            loop {
                                match self.socket.send(buf) {
                                    Ok(x) => {
                                        if x != buf.len() {
                                            continue;
                                        }
                                    }
                                    Err(_) => {
                                        continue;
                                    }
                                }
                                thread::sleep(Duration::from_millis(4));
                                break;
                            }
                            start = unix_millis();
                        } else { // Latest packet ACK'd, no packets really lost, continue with next packet
                            break;
                        }
                    }
                    if !wait {
                        break;
                    }
                }
            }
        }
        self.socket
            .set_read_timeout(Some(Duration::from_millis(1000)))
            .unwrap();
        return Ok(());
    }
}

pub fn helper(args: &Vec<String>) {
    if args.len() > 3 {
        usage(args, "Too many arguments");
    }
    if args.len() < 3 {
        usage(args, "No PORT given");
    }
    let bind_addr = (
        "0.0.0.0",
        u16::from_str_radix(args[2].as_str(), 10).expect("Invalid port: must be integer"),
    );
    let mut map: HashMap<[u8; 200], SocketAddr> = HashMap::new();
    let listener = UdpSocket::bind(&bind_addr).expect("Unable to create socket");
    let mut buf = [0 as u8; 200];
    loop {
        let (l, addr) = listener.recv_from(&mut buf).expect("Read error");
        if l != 200 {
            continue;
        }
        if map.contains_key(&buf) {
            let other = map.get(&buf).unwrap();
            // We got a connection
            let mut bytes: &[u8] = addr.to_string().bytes().collect::<Vec<u8>>().leak();
            let mut addr_buf = [0 as u8; 200];
            for i in 0..bytes.len().min(200) {
                addr_buf[i] = bytes[i];
            }
            bytes = other.to_string().bytes().collect::<Vec<u8>>().leak();
            let mut other_buf = [0 as u8; 200];
            for i in 0..bytes.len().min(200) {
                other_buf[i] = bytes[i];
            }
            if listener.send_to(&addr_buf, other).is_ok() && listener.send_to(&other_buf, addr).is_ok() {
                // Success!
                let d = PrimitiveDateTime::new(
                    Date::from_calendar_date(1970, time::Month::January, 1).unwrap(),
                    Time::MIDNIGHT,
                 ) + Duration::from_millis(unix_millis());
                print!("{} UTC  ", d);
                println!("Connected {} & {}", addr, other);
            }
            map.remove(&buf);
        } else {
            map.insert(buf, addr);
        }
    }
}

pub fn sender<F: Fn(f32)>(args: &Vec<String>, on_progress: F) {
    let connection = holepunch(args);
    let dly = args
        .get(5)
        .map(|s| u64::from_str_radix(s, 10))
        .unwrap_or(Ok(500))
        .expect("Non-numeric DELAY operand");
    let br = args
        .get(6)
        .map(|s| u32::from_str_radix(s, 10))
        .unwrap_or(Ok(256))
        .expect("Non-numeric BITRATE argument");
    let begin = args
        .get(7)
        .map(|s| u64::from_str_radix(s, 10))
        .unwrap_or(Ok(0))
        .expect("Non-numeric SKIP operand");
    let mut buf: Vec<u8> = Vec::new();
    buf.resize(br as usize, 0);
    let mut buf = buf.leak();
    let mut file = File::open(args.get(2).unwrap_or_else(|| {
        usage(args, "No FILE");
        panic!("notreached")
    }))
    .expect("File not readable");

    if begin != 0 {
        println!("Skipping to {}...", begin);
        file.seek(SeekFrom::Start(begin)).expect("unable to skip");
        println!("Done.");
    }

    let mut sc = SafeReadWrite::new(connection);
    let mut bytes_sent: u64 = 0;
    let mut last_update = unix_millis();
    let len = file.metadata().expect("bad metadata").len();
    sc.write_safe(&len.to_be_bytes(), 3000)
        .expect("unable to send file length");
    println!("Length: {}", &len);
    let mut time = unix_millis();
    loop {
        let read = file.read(&mut buf).expect("File read error");
        if read == 0 && !env::var("QFT_STREAM").is_ok() {
            println!();
            println!("Transferred");
            sc.end();
            return;
        }

        sc.write_safe(&buf[..read], dly).expect("Send error");
        bytes_sent += read as u64;
        if (bytes_sent % (br * 20) as u64) < (br as u64) {
            let elapsed = unix_millis() - time;
            let elapsed = if elapsed == 0 { 1 } else { elapsed };

            print!(
                "\r\x1b[KSent {} bytes; Speed: {} kb/s",
                bytes_sent,
                br as usize * 20 / elapsed as usize
            );
            stdout().flush().unwrap();
            time = unix_millis();
        }
        if unix_millis() - last_update > 100 {
            on_progress((bytes_sent + begin) as f32 / len as f32);
            last_update = unix_millis();
        }
    }
}

pub fn receiver<F: Fn(f32)>(args: &Vec<String>, on_progress: F) {
    let connection = holepunch(args);
    let br = args
        .get(5)
        .map(|s| u32::from_str_radix(s, 10))
        .unwrap_or(Ok(256))
        .expect("Non-numeric BITRATE argument");
    let begin = args
        .get(6)
        .map(|s| u64::from_str_radix(s, 10))
        .unwrap_or(Ok(0))
        .expect("Non-numeric SKIP argument");
    let mut buf: Vec<u8> = Vec::new();
    buf.resize(br as usize, 0);
    let buf: &[u8] = buf.leak();
    let mut file = OpenOptions::new()
        .truncate(false)
        .write(true)
        .create(true)
        .open(&args.get(2).unwrap_or_else(|| {
            usage(args, "No FILE");
            panic!("notreached")
        }))
        .expect("File not writable");

    if begin != 0 {
        println!("Skipping to {}...", begin);
        file.seek(SeekFrom::Start(begin)).expect("Unable to skip");
        println!("Done");
    }

    let mut sc = SafeReadWrite::new(connection);
    let mut bytes_received: u64 = 0;
    let mut last_update = unix_millis();
    let mut len_bytes = [0 as u8; 8];
    let len = sc
        .read_safe(&mut len_bytes)
        .expect("Unable to read file length from sender")
        .0;
    let len = u64::from_be_bytes([
        len[0], len[1], len[2], len[3], len[4], len[5], len[6], len[7],
    ]);
    let _ = file.set_len(len);
    println!("Length: {}", &len);
    let mut time = unix_millis();
    loop {
        let (mbuf, amount) = sc.read_safe(buf).expect("Read error");
        let buf = &mbuf.leak()[..amount];
        if amount == 0 {
            println!();
            println!("Transferred");
            return;
        }

        file.write(buf).expect("Write error");
        file.flush().expect("File flush error");
        bytes_received += amount as u64;
        if (bytes_received % (br * 20) as u64) < (br as u64) {
            let elapsed = unix_millis() - time;
            let elapsed = if elapsed == 0 { 1 } else { elapsed };

            print!(
                "\r\x1b[KReceived {} bytes; Speed: {} kb/s",
                bytes_received,
                br as usize * 20 / elapsed as usize
            );
            stdout().flush().unwrap();
            time = unix_millis();
        }
        if unix_millis() - last_update > 100 {
            on_progress((bytes_received + begin) as f32 / len as f32);
            last_update = unix_millis();
        }
    }
}

fn holepunch(args: &Vec<String>) -> UdpSocket {
    let bind_addr = (Ipv4Addr::from(0 as u32), 0);
    let holepunch = UdpSocket::bind(&bind_addr).expect("Unable to create socket");
    let mut helper = match env::var_os("QFT_HELPER") {
        Some(v) => v.into_string().unwrap(),
        None => HELPER.to_string()
    };
    if args.len() > 4 {
        helper = args.get(4).unwrap().to_string();
    }
		println!("Using helper: {}", helper);
    holepunch
        .connect(helper)
        .expect("Unable to connect to helper");
    let bytes = args
        .get(3)
        .unwrap_or_else(|| {
            usage(args, "No password");
            panic!("notreached")
        })
        .as_bytes();
    let mut buf = [0 as u8; 200];
    for i in 0..bytes.len().min(200) {
        buf[i] = bytes[i];
    }
    holepunch.send(&buf).expect("Unable to send to helper");
    holepunch.recv(&mut buf).expect("Unable to receive from helper");
    // Now buf should contain partner's address data
    let mut s = Vec::from(buf);
    s.retain(|e| *e != 0);
    let bind_addr = String::from_utf8_lossy(s.as_slice()).to_string();
    println!("Holepunching here ({}) to there ({})", holepunch.local_addr().unwrap().port(), bind_addr);
    holepunch
        .connect(SocketAddrV4::from_str(bind_addr.as_str()).unwrap())
        .expect("Connection failed");
    holepunch
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    holepunch
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    if env::var("QFT_USE_TIMED_HOLEPUNCH").is_ok() {
        println!("Warning: You are using the QFT_USE_TIMED_HOLEPUNCH environment variable. This obstructs \
            backwards-compatibility. It is meant as a fallback for bad connections. Be absolutely sure the \
            other end uses QFT_USE_TIMED_HOLEPUNCH as well, otherwise data can get corrupted on transfer.");
        println!("Waiting...");
        let mut stop = false;
        while !stop {
            thread::sleep(Duration::from_millis(500 - (unix_millis() % 500)));
            println!("CONNECT {}", unix_millis());
            let _ = holepunch.send(&[0]);
            let result = holepunch.recv(&mut [0, 0]);
            if result.is_ok() && result.unwrap() == 1 {
                holepunch.send(&[0, 0]).expect("Connection failed");
                let result = holepunch.recv(&mut [0, 0]);
                if result.is_ok() && result.unwrap() == 2 {
                    stop = true;
                }
            }
        }
    } else {
        println!("Connecting...");
        thread::sleep(Duration::from_millis(500 - (unix_millis() % 500)));
        for _ in 0..40 {
            let m = unix_millis();
            let _ = holepunch.send(&[0]);
            thread::sleep(Duration::from_millis((50 - (unix_millis() - m)).max(0)));
        }
        let mut result = Ok(1);
        while result.is_ok() && result.unwrap() == 1 {
            result = holepunch.recv(&mut [0, 0]);
        }
        holepunch.send(&[0, 0]).expect("Connection failed");
        holepunch.send(&[0, 0]).expect("Connection failed");
        result = Ok(1);
        while result.is_ok() && result.unwrap() != 2 {
            result = holepunch.recv(&mut [0, 0]);
        }
        result = Ok(1);
        while result.is_ok() && result.unwrap() == 2 {
            result = holepunch.recv(&mut [0, 0]);
        }
    }
    println!("Holepunch and connection successful");
    return holepunch;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 0 {
        panic!("No commandline...");
    }
    if args.len() == 1 {
        usage(&args, "No command");
    }
    match args.get(1).unwrap().as_str() { // Command
        "help" => usage(&args, ""),
        "h" => usage(&args, ""),
        "-h" => usage(&args, ""),
        "--help" => usage(&args, ""),
        "helper" => helper(&args),
        "H" => helper(&args),
        "send" => sender(&args, |_| {}),
        "s" => sender(&args, |_| {}),
        "S" => sender(&args, |_| {}),
        "receive" => receiver(&args, |_| {}),
        "r" => receiver(&args, |_| {}),
        "R" => receiver(&args, |_| {}),
        "version" => println!("qft v{}", VERSION),
        "v" => println!("qft v{}", VERSION),
        "V" => println!("qft v{}", VERSION),
        "-V" => println!("qft v{}", VERSION),
        "--version" => println!("qft v{}", VERSION),
        _ => usage(&args, "Unrecognized command"),
    }
}

fn usage(args: &Vec<String>, msg: &str) {
    let f: Vec<_> = args.get(0).unwrap().split('/').collect();
    let c = f[f.len()-1];
    println!("\
{} v{} - Quick file transfer
Usage:  {} COMMAND ARGUMENT...
    COMMAND:
        H | helper  PORT
        S | send  FILE PASSWORD [ADDRESS:PORT] [DELAY [BITRATE [SKIP]]]
        R | receive  FILE PASSWORD [ADDRESS:PORT] [BITRATE [SKIP]]
        V | version
        help"
        , c, env!("CARGO_PKG_VERSION"), c);
    if msg.len() == 0 {
        std::process::exit(0);
    }
    println!(">>> {}", msg);
    std::process::exit(1);
}

pub fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
