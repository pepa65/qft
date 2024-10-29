// main.rs

use std::{
	collections::HashMap,
	env,
	fs::{File, OpenOptions},
	io::{stdout, Error, Read, Seek, SeekFrom, Write as W},
	net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
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
use SafeReadWritePacket::{Ack, End, ResendRequest, Write};

struct SafeReadWrite {
	socket: UdpSocket,
	last_transmitted: HashMap<u16, Vec<u8>>,
	packet_count_out: u64,
	packet_count_in: u64,
}

struct Wrap<T>(T);

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELPER: &str = "tudbut.de:4277";
const DEFAULT_PORT: u16 = 4277;
const BITRATE: usize = 256;
const DELAY: u64 = 500;
const MAX_BUF_LEN: usize = 0xFFFC;
const ID_WRAP: u16 = 0xFFFF;
const SOCKET_DELAY: u64 = 3000;
const RESEND_DELAY: u64 = 100;
const LOST_DELAY: u64 = 5000;
const BROKEN_DELAY: u64 = 10000;
const MAP_LEN: usize = 200;
const WRITE_DELAY: u64 = 3000;
const KB_FAC: usize = 20;
const PUNCH_DELAY: u64 = 500;
const PUNCH_INC_DELAY: u64 = 50;
const PUNCH_DEC_DELAY: u64 = 40;

impl Mul<Wrap<&str>> for u64 {
	type Output = String;
	fn mul(self, rhs: Wrap<&str>) -> Self::Output {
		let strings: Vec<&str> = (0..self).map(|_| rhs.0).collect();
		strings.join("")
	}
}

impl SafeReadWrite {
	pub fn new(socket: UdpSocket) -> Self {
		Self { socket, last_transmitted: HashMap::new(), packet_count_in: 0, packet_count_out: 0 }
	}

	pub fn write_safe(&mut self, buf: &[u8], delay: u64) -> Result<(), Error> {
		self.write_flush_safe(buf, false, delay)
	}

	pub fn write_flush_safe(&mut self, buf: &[u8], flush: bool, delay: u64) -> Result<(), Error> {
		self.internal_write_safe(buf, Write, flush, false, delay);
		Ok(())
	}

	pub fn read_safe(&mut self, buf: &[u8]) -> Result<(Vec<u8>, usize), Error> {
		assert!(buf.len() <= MAX_BUF_LEN, "attempted to receive too large data packet with SafeReadWrite ({} > 0xFFFC)", buf.len());
		let mut mbuf = Vec::from(buf);
		mbuf.insert(0, 0);
		mbuf.insert(0, 0);
		mbuf.insert(0, 0);
		let buf: &mut [u8] = mbuf.as_mut();
		let mut r = (vec![], 0);
		let mut try_again = true;
		let mut is_catching_up = false;
		while try_again {
			if let Ok(x) = self.socket.recv(buf) {
				if x < 3 {
					continue;
				}
				let id = u16::from_be_bytes([buf[0], buf[1]]);
				if id <= self.packet_count_in as u16 {
					self.socket.send(&[buf[0], buf[1], Ack as u8]).expect("Send error");
				}
				if id == self.packet_count_in as u16 {
					if id == ID_WRAP {
						println!("\nPacket ID wrap successful.");
					}
					try_again = false;
					self.packet_count_in += 1;
					r.1 = x - 3;
				} else if id > self.packet_count_in as u16 && (id - self.packet_count_in as u16) < 0xC000 {
					if !is_catching_up && env::var("QFT_HIDE_DROPS").is_err() {
						println!("\r\x1b[KA packet dropped: {} (got) is newer than {} (expected)", &id, &(self.packet_count_in as u16));
					}
					is_catching_up = true;
					// Ask to resend, then do nothing
					let id = (self.packet_count_in as u16).to_be_bytes();
					self.socket.send(&[id[0], id[1], ResendRequest as u8]).expect("Send error");
				}
				if buf[2] == End as u8 {
					return Ok((vec![], 0));
				}
			}
		}
		mbuf.remove(0);
		mbuf.remove(0);
		mbuf.remove(0);
		r.0 = mbuf;
		Ok(r)
	}

	pub fn end(mut self) -> UdpSocket {
		self.internal_write_safe(&[], End, true, true, SOCKET_DELAY);
		self.socket
	}

	fn internal_write_safe(&mut self, buf: &[u8], packet: SafeReadWritePacket, flush: bool, exit_on_lost: bool, delay: u64) {
		assert!((buf.len() <= MAX_BUF_LEN), "Too large data packet sent over SafeReadWrite ({} > 0xFFFC)", buf.len());
		let id = (self.packet_count_out as u16).to_be_bytes();
		let idn = self.packet_count_out as u16;
		self.packet_count_out += 1;
		let mut vbuf = Vec::from(buf);
		vbuf.insert(0, packet as u8);
		vbuf.insert(0, id[1]);
		vbuf.insert(0, id[0]); // This is now the first byte
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
		let mut wait = idn == ID_WRAP || flush;
		if self.last_transmitted.len() < 256 {
			self.socket.set_read_timeout(Some(Duration::from_millis(1))).unwrap();
		} else {
			wait = true;
		}
		let mut start = unix_millis();
		if idn == ID_WRAP {
			print!("\nPacket ID needs to wrap. Waiting for partner to catch up...");
		}
		let mut is_catching_up = false;
		loop {
			if let Some(x) = (
				if !wait {
					self.socket.set_nonblocking(true).unwrap();
				},
				self.socket.recv(&mut buf).ok(),
				self.socket.set_nonblocking(false).unwrap(),
			)
				.1
			{
				if x != 3 {
					continue;
				}
				if buf[2] == Ack as u8 {
					let n = u16::from_be_bytes([buf[0], buf[1]]);
					self.last_transmitted.remove(&n);
					if n == idn {
						if idn == ID_WRAP {
							println!("\r\x1b[KPacket ID wrap successful.");
						}
						wait = false;
						// If the latest packet is ACK'd, all previous ones must be as well
						self.last_transmitted.clear();
					}
				}
				if buf[2] == ResendRequest as u8 {
					let mut n = u16::from_be_bytes([buf[0], buf[1]]);
					thread::sleep(Duration::from_millis(RESEND_DELAY));
					while self.socket.recv(&mut buf).ok().is_some() {}
					if is_catching_up && env::var("QFT_HIDE_DROPS").is_ok() {
						println!("\r\x1b[KA packet dropped: {}", &n);
					}
					if !is_catching_up {
						wait = true;
						is_catching_up = true;
						while n <= idn && !(idn == ID_WRAP && n == 0) {
							let buf = self.last_transmitted.get(&n);
							if let Some(buf) = buf {
								loop {
									// Resend until success
									match self.socket.send(buf.as_slice()) {
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
			} else {
				if unix_millis() - start > LOST_DELAY && exit_on_lost {
					// Check lost on exit after 5s
					break;
				}
				if unix_millis() - start > BROKEN_DELAY {
					// Retry after 10s
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
					} else {
						// Latest packet ACK'd, no packets really lost, continue with next packet
						break;
					}
				}
				if !wait {
					break;
				}
			}
		}
		self.socket.set_read_timeout(Some(Duration::from_millis(1000))).unwrap();
		//Ok(())
	}
}

fn helper(cli: &Cli) {
	let bind_addr = ("0.0.0.0", cli.port);
	let mut map: HashMap<[u8; MAP_LEN], SocketAddr> = HashMap::new();
	let listener = UdpSocket::bind(bind_addr).expect("Unable to create socket");
	let mut buf = [0_u8; MAP_LEN];
	loop {
		let (l, addr) = listener.recv_from(&mut buf).expect("Read error");
		if l != MAP_LEN {
			continue;
		}
		if let std::collections::hash_map::Entry::Vacant(e) = map.entry(buf) {
			e.insert(addr);
		} else {
			let other = map.get(&buf).unwrap();
			// We got a connection
			let mut bytes: &[u8] = addr.to_string().bytes().collect::<Vec<u8>>().leak();
			let mut addr_buf = [0_u8; MAP_LEN];
			addr_buf[..bytes.len().min(MAP_LEN)].copy_from_slice(&bytes[..bytes.len().min(MAP_LEN)]);
			bytes = other.to_string().bytes().collect::<Vec<u8>>().leak();
			let mut other_buf = [0_u8; MAP_LEN];
			other_buf[..bytes.len().min(MAP_LEN)].copy_from_slice(&bytes[..bytes.len().min(MAP_LEN)]);
			if listener.send_to(&addr_buf, other).is_ok() && listener.send_to(&other_buf, addr).is_ok() {
				// Success!
				let d = PrimitiveDateTime::new(Date::from_calendar_date(1970, time::Month::January, 1).unwrap(), Time::MIDNIGHT)
					+ Duration::from_millis(unix_millis());
				print!("{d} UTC  ");
				println!("Connected {addr} & {other}");
			}
			map.remove(&buf);
		}
	}
}

fn sender<F: Fn(f32)>(cli: &Cli, on_progress: F) {
	let connection = holepunch(cli);
	let buf: Vec<u8> = vec![0; cli.bitrate];
	let buf = buf.leak();
	let mut file = File::open(cli.file.as_str()).expect("File not readable");
	if cli.start != 0 {
		println!("Starting at {}...", cli.start);
		file.seek(SeekFrom::Start(cli.start)).expect("Unable to skip");
		println!("Done.");
	}
	let mut sc = SafeReadWrite::new(connection);
	let mut bytes_sent: u64 = 0;
	let mut last_update = unix_millis();
	let len = file.metadata().expect("Bad metadata").len();
	sc.write_safe(&len.to_be_bytes(), WRITE_DELAY).expect("Unable to send file length");
	println!("Length: {}", &len);
	let mut time = unix_millis();
	loop {
		let read = file.read(buf).expect("File read error");
		if read == 0 && env::var("QFT_STREAM").is_err() {
			println!();
			println!("Transferred");
			sc.end();
			return;
		}
		sc.write_safe(&buf[..read], cli.delay).expect("Send error");
		bytes_sent += read as u64;
		if (bytes_sent % (cli.bitrate * KB_FAC) as u64) < cli.bitrate as u64 {
			let elapsed = unix_millis() - time;
			let elapsed = if elapsed == 0 { 1 } else { elapsed };
			print!("\r\x1b[KSent {} bytes; Speed: {} kb/s", bytes_sent, cli.bitrate as u64 * KB_FAC as u64 / elapsed);
			stdout().flush().unwrap();
			time = unix_millis();
		}
		if unix_millis() - last_update > 100 {
			on_progress((bytes_sent + cli.start) as f32 / len as f32);
			last_update = unix_millis();
		}
	}
}

fn receiver<F: Fn(f32)>(cli: &Cli, on_progress: F) {
	let connection = holepunch(cli);
	let buf: Vec<u8> = vec![0; cli.bitrate];
	let buf: &[u8] = buf.leak();
	let mut file = OpenOptions::new().truncate(false).write(true).create(true).open(&cli.file).expect("File not writable");
	if cli.start != 0 {
		println!("Starting at {}", cli.start);
		file.seek(SeekFrom::Start(cli.start)).expect("Unable to skip");
		println!("Done");
	}
	let mut sc = SafeReadWrite::new(connection);
	let mut bytes_received: u64 = 0;
	let mut last_update = unix_millis();
	let len_bytes = [0_u8; 8];
	let len = sc.read_safe(&len_bytes).expect("Unable to read file length from sender").0;
	let len = u64::from_be_bytes([len[0], len[1], len[2], len[3], len[4], len[5], len[6], len[7]]);
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
		file.write_all(buf).expect("Write error");
		file.flush().expect("File flush error");
		bytes_received += amount as u64;
		if (bytes_received % (cli.bitrate * KB_FAC) as u64) < (cli.bitrate as u64) {
			let elapsed = unix_millis() - time;
			let elapsed = if elapsed == 0 { 1 } else { elapsed };
			print!("\r\x1b[KReceived {} bytes; Speed: {} kb/s", bytes_received, cli.bitrate as u64 * KB_FAC as u64 / elapsed);
			stdout().flush().unwrap();
			time = unix_millis();
		}
		if unix_millis() - last_update > RESEND_DELAY {
			on_progress((bytes_received + cli.start) as f32 / len as f32);
			last_update = unix_millis();
		}
	}
}

fn holepunch(cli: &Cli) -> UdpSocket {
	let bind_addr = (Ipv4Addr::from(0_u32), 0);
	let holepunch = UdpSocket::bind(bind_addr).expect("Unable to create socket");
	let helper = if cli.helper.is_empty() {
		env::var_os("QFT_HELPER").map_or_else(|| HELPER.to_string(), |v| v.into_string().unwrap())
	} else {
		cli.helper.clone()
	};
	if cli.command == "S" {
		println!("Sending using helper: {helper}");
	} else {
		println!("Receiving using helper: {helper}");
	};
	holepunch.connect(helper).expect("Unable to connect to helper");
	let bytes = cli.tag.as_bytes();
	let mut buf = [0_u8; MAP_LEN];
	buf[..bytes.len().min(MAP_LEN)].copy_from_slice(&bytes[..bytes.len().min(MAP_LEN)]);
	holepunch.send(&buf).expect("Unable to send to helper");
	holepunch.recv(&mut buf).expect("Unable to receive from helper");
	// Now buf should contain partner's address data
	let mut s = Vec::from(buf);
	s.retain(|e| *e != 0);
	let bind_addr = String::from_utf8_lossy(s.as_slice()).to_string();
	println!("Holepunching here ({}) to there ({})", holepunch.local_addr().unwrap().port(), bind_addr);
	holepunch.connect(SocketAddrV4::from_str(bind_addr.as_str()).unwrap()).expect("Connection failed");
	holepunch.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
	holepunch.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
	if env::var("QFT_USE_TIMED_HOLEPUNCH").is_ok() {
		println!(
			"Warning: You are using the QFT_USE_TIMED_HOLEPUNCH environment variable. This obstructs \
             backwards-compatibility. It is meant as a fallback for bad connections. Be absolutely sure the \
             other end uses QFT_USE_TIMED_HOLEPUNCH as well, otherwise data can get corrupted on transfer."
		);
		println!("Waiting...");
		let mut stop = false;
		while !stop {
			thread::sleep(Duration::from_millis(PUNCH_DELAY - (unix_millis() % PUNCH_DELAY)));
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
		thread::sleep(Duration::from_millis(PUNCH_DELAY - (unix_millis() % PUNCH_DELAY)));
		for _ in 0..PUNCH_DEC_DELAY {
			let m = unix_millis();
			let _ = holepunch.send(&[0]);
			thread::sleep(Duration::from_millis(PUNCH_INC_DELAY - unix_millis() + m));
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
	holepunch
}

struct Cli {
	binary: String,
	command: String,
	port: u16,
	file: String,
	tag: String,
	helper: String,
	delay: u64,
	bitrate: usize,
	start: u64,
}

fn parse_cli(args: &[String]) -> Cli {
	let caller = args.first().expect("Invocation").to_string();
	let parts: Vec<_> = caller.split('/').collect();
	let mut cli = Cli {
		binary: parts[parts.len() - 1].to_string(),
		command: "h".to_string(),
		port: DEFAULT_PORT,
		file: String::new(),
		tag: String::new(),
		helper: String::new(),
		delay: DELAY,
		bitrate: BITRATE,
		start: 0,
	};
	let command = args.get(1).unwrap_or(&cli.command);
	match command.as_str() {
		"readme" => cli.command = "r".to_string(),
		"help" | "-h" | "--help" | "h" => cli.command = "h".to_string(),
		"helper" | "H" => cli.command = "H".to_string(),
		"send" | "S" | "s" => cli.command = "S".to_string(),
		"receive" | "R" | "r" => cli.command = "R".to_string(),
		"version" | "-V" | "v" | "V" | "--version" => cli.command = "V".to_string(),
		_ => usage(&cli, "Unrecognized command"),
	}
	let mut i = 2;
	while args.get(i).is_some() {
		match cli.command.as_str() {
			"H" => cli.port = args.get(i).map(|s| s.parse::<u16>()).unwrap().expect("PORT"),
			"S" | "R" => match args.get(i).unwrap().as_str() {
				"-d" | "-r" | "-s" => {
					if args.get(i + 1).is_none() {
						usage(&cli, format!("Flag {} has no argument", args.get(i).unwrap()).as_str());
					};
					match args.get(i).unwrap().as_str() {
						"-d" => cli.delay = args.get(i + 1).map(|s| s.parse::<u64>()).unwrap().expect("DELAY"),
						"-r" => cli.bitrate = args.get(i + 1).map(|s| s.parse::<usize>()).unwrap().expect("BITRATE"),
						_ => cli.start = args.get(i + 1).map(|s| s.parse::<u64>()).unwrap().expect("START"),
					};
					i += 1;
				}
				_ => {
					if cli.file.is_empty() {
						cli.file = args.get(i).expect("FILE").to_string();
					} else if cli.tag.is_empty() {
						cli.tag = args.get(i).expect("TAG").to_string();
					} else if cli.helper.is_empty() {
						cli.helper = args.get(i).expect("HELPER").to_string();
					} else {
						usage(&cli, "Too many arguments");
					}
				}
			},
			// V|version | h|help | r|reader
			_ => usage(&cli, "Too many arguments"),
		}
		i += 1;
	}
	cli
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let cli = parse_cli(&args);
	match cli.command.as_str() {
		"r" => print!("{}", include_str!("../README.md")),
		"V" => println!("qft v{VERSION}"),
		"h" => usage(&cli, ""),
		"H" => helper(&cli),
		// S|send | R\receive
		_ => {
			if cli.file.is_empty() {
				usage(&cli, "No FILE");
			}
			if cli.tag.is_empty() {
				usage(&cli, "No TAG");
			}
			if cli.command.as_str() == "S" {
				sender(&cli, |_| {});
			} else {
				receiver(&cli, |_| {});
			}
		}
	}
}

fn usage(cli: &Cli, msg: &str) {
	print!(
		"\
{} v{VERSION} - Quick file transfer
Usage:  {} [COMMAND [ARGUMENT...]]
COMMAND:
    help (default command)    Just output this help text.
    readme                    Output the repo's README.md file.
    V | version               Just output the version number.
    H | helper [PORT]         Start helper on PORT (default: {DEFAULT_PORT}).
    s | S | send FILE TAG [ADDRESS:PORT] [-d DELAY] [-r BITRATE] [-s START]
    r | R | receive FILE TAG [ADDRESS:PORT] [-r BITRATE] [-s START]
  Defaults: ADDRESS:PORT={HELPER} (env.variable QFT_HELPER overrides
  this, commandline overrides that), DELAY={DELAY}, BITRATE={BITRATE}, START=0
",
		cli.binary, cli.binary
	);
	if msg.is_empty() {
		std::process::exit(0);
	}
	println!(">>> {msg}");
	std::process::exit(1);
}

fn unix_millis() -> u64 {
	SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64
}
