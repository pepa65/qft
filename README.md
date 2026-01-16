[![version](https://img.shields.io/crates/v/qft.svg)](https://crates.io/crates/qft)
[![build](https://github.com/pepa65/qft/actions/workflows/rust.yml/badge.svg)](https://github.com/pepa65/qft/actions/workflows/rust.yml) 
[![dependencies](https://deps.rs/repo/github/pepa65/aegis-cli/status.svg)](https://deps.rs/repo/github/pepa65/aegis-cli)
[![docs](https://img.shields.io/badge/docs-qft-blue.svg)](https://docs.rs/crate/qft/latest)
[![license](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://github.com/pepa65/qft/blob/main/LICENSE)
[![downloads](https://img.shields.io/crates/d/qft.svg)](https://crates.io/crates/qft)

![qft](https://raw.github.com/pepa65/qft/main/logo.png "Quick File Transfer")

# qft v0.7.35
**Quick File Transfer, true peer-to-peer over UDP on CLI**

QFT is a small stand-alone binary for quick and reliable true peer-to-peer UDP file transfer.
As UDP is a connectionless protocol, there are no handshakes, data just gets sent. There is no
proper start of the 'connection' and no 'disconnection', the exchange depends on the content
of the packages. This also makes bypassing NAT more challenging, but it is possible. Apart from
establishing the connection through an ultralight __helper__, the exchange is truly peer-to-peer.
That said, there are measures in place to ensure data integrity. Also (long!) pauses in
transmission are allowed for, up to hibernating one of the machines (it will resume on wake-up!).
Packet loss/reorder rates of over 10% are tolerated (but will slow down transmission speed!) and
ping times of 1000ms are just as navigable as 10ms ones.

* Repo: https://github.com/pepa65/qft
* After (and compatible with): https://github.com/tudbut/qft
* License: GPLv3+
* Standalone single binary programmed in 100% Rust.

## Install
### Download static single-binary
```
wget https://github.com/pepa65/qft/releases/download/0.7.35/qft
sudo mv qft /usr/local/bin
sudo chown root:root /usr/local/bin/qft
sudo chmod +x /usr/local/bin/qft
```

### Install with cargo
#### Static musl build from cloned repo
```
# After git-cloning the repo
rustup target add x86_64-unknown-linux-musl
cargo build --release
```

#### Dynamic build with cargo
`cargo install --git https://github.com/pepa65/qft`

### Install with cargo-binstall
Even without a full Rust toolchain, rust binaries can be installed with the static binary `cargo-binstall`:

```
# Install cargo-binstall for Linux x86_64
# (Other versions are available at https://crates.io/crates/cargo-binstall)
wget github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz
tar xf cargo-binstall-x86_64-unknown-linux-musl.tgz
sudo chown root:root cargo-binstall
sudo mv cargo-binstall /usr/local/bin/
```

Only a linux-x86_64 (musl) binary available: `cargo-binstall qft`

It will be installed in `~/.cargo/bin/` which will need to be added to `PATH`!

## Usage
### Sending / Receiving
* On the sending machine, enter `qft s FILE TAG` where FILE is a filename being sent,
  and TAG can be freely chosen, but must be the same on both sides.
* On the receiving machine, enter `qft r FILE TAG` where FILE can be a different name
  that the data will be saved to, but TAG must be the same.
* Both machines should start transferring after a short while. If they don't, try again.

### Helper
* The QFT helper is specified by and internet address and port, and helps with
  connecting both parties, even when both are behind a NAT/router. (A helper can
  also be on the LAN if both sender and receiver are on the same LAN.) You can use
  and trust publicly provided QFT helpers. They only get to know the IP addresses
  and the temporary random ports, and the chosen TAG for the exchange.
* The hardcoded default helper is `tudbut.de:4277`.
* The default helper can be specified in the environment variable `QFT_HELPER`, like:
  `export QFT_HELPER=qft.4e4.in:4444`.
* A helper can be run on any machine that has the chosen port open to the internet:
  `qft helper [PORT]` and if the PORT number is higher than 1024, no privilege is needed.
  When not supplied, the port defaults to 4277. Running a helper is very low on CPU and
  bandwith resources, as it only listens and is not involved in the data transfer, it just
  establishes the connection based on the same TAG, and exchanges the IP address and port
  for each machine. This is different from `croc`, `portal` or all the `wormhole`
  applications that all need an actual transfer relay server (unless in some cases,
  both machines are on the same LAN).

### Full options
* `qft h|help|-h|--help` - Just outputs a help text.
* `qft readme` - Outputs this README.md.
* `qft V|version|-V|--version` - Just outputs the version number.
* `qft H|helper [PORT]` - See **Helper** above.
* `qft s|S|send FILE TAG [ADDRESS:PORT] [-d DELAY] [-r BITRATE] [-s START]`
* `qft r|R|receive FILE TAG [ADDRESS:PORT] [-r BITRATE] [-s START]`
* Arguments:
  - `PORT` in the `helper` command defaults to 4277 when not supplied.
  - The first 2 arguments after `send` and `receive` are always `FILE` and `TAG` (in that order).
  - The filename being sent is `FILE`, as is the filename being saved to on the receiving end.
  - `TAG` must be the same on both ends in order to for the helper to connect the exchange.
  - The helper's address & port is `ADDRESS:PORT`, for example `tudbut.de:4277` (the default).
  - `DELAY` can be lowered from the default 500 to speed up transfers (lowering the delay
    between packets), but too low will cause unreliability in the data transfer!
  - `BITRATE` can be increased from the default 256 to increase packet size (but servers or routers
    set limits at various sizes!).
  - `START` allows the transfer to start after a certain byte, skipping already transferred parts.
    Look at the terminal output to find out how many bytes were transferred already.
    See **Troubleshooting** below.

### Help text
```
qft v0.7.35 - Quick file transfer
Usage:  qft [COMMAND [ARGUMENT...]]
COMMAND:
    help (default command)    Just output this help text.
    readme                    Output the repo's README.md file.
    V | version               Just output the version number.
    H | helper [PORT]         Start helper on PORT (default: 4277).
    s | S | send FILE TAG [ADDRESS:PORT] [-d DELAY] [-r BITRATE] [-s START]
    r | R | receive FILE TAG [ADDRESS:PORT] [-r BITRATE] [-s START]
  Defaults: ADDRESS:PORT=tudbut.de:4277 (env.variable QFT_HELPER overrides
  this, commandline overrides that), DELAY=500, BITRATE=256, START=0 
```

### Environment variables
* Variables need to be explicitly `export`ed to be recognized as environment variables.
* If `QFT_STREAM` is set, the sender can use `/dev/stdin` as the FILE to be sending from and data
  can be directed in.
* Setting `QFT_HIDE_DROPS` suppresses reporting on drops at both the sending and the receiving end.
* When `QFT_USE_TIMED_HOLEPUNCH` is set **on both ends!**, a different transfer mechanism is used,
  meant to help with bad connections. This is meant as a fallback, not recommended for general use.

### Troubleshooting
#### Resume a fully stopped transfer
You most likely never needed unless the transfer completely died due to a very long pause or a
computer reboot. Then: Ctrl-C wherever `qft` is still running, and start the same command while
specifying `-s START`.

#### It says `Connecting...` but doesn't connect
One of the ends was not properly connected to the helper. Stop `qft` on both ends and try again
(preferably with a different TAG).

## Considerations
### Security
#### Helper
The helper is vulnerable to port-sniffing, and transfers could be 'snatched' by an agent quickly
deploying a used TAG. If the sender knocked first, the file could be received by the agent, if
the receiver knocked first, an agent could send (different) data.

#### Transfer
Transfers on QFT are not end-to-end encrypted, but then, the data only touches the sender and the receiver's machine, there is no man-in-the-middle. Well, there is internet routing... So before sending sensitive data, encrypting it before sending would be prudent.

### [Relevant XKCD](https://xkcd.com/949)
![Relevant XKCD Image](https://imgs.xkcd.com/comics/file_transfer.png "Every time you email a file to yourself so you can pull it up on your friend's laptop, Tim Berners-Lee sheds a single tear.")

