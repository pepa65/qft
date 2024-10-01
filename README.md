# ![qft](https://raw.github.com/pepa65/qft/main/logo.png "Quick File Transfer")
[![Rust](https://github.com/pepa65/qft/actions/workflows/rust.yml/badge.svg)](https://github.com/pepa65/qft/actions/workflows/rust.yml) 
# qft v0.6.1
**Quick true peer-to-peer UDP File Transfer**

QFT is a small application for Quick (and really reliable) peer-to-peer UDP File Transfer.
UDP is a connectionless protocol, there are no handshakes, data just gets sent. There is no
proper start of the 'connection' and no 'disconnection', the exchange depends on the content
of the packages. This also makes bypassing NAT more challenging, but it is possible. Apart from
establishing the connection through an ultralight 'helper', the exchange is truly peer-to-peer.
That said, there are measures in place to ensure data integrity. Also (long!) pauses in
transmission are allowed for, up to hibernating one of the machines; it will resume on wake-up!
Packet loss/reorder rates of over 10% are tolerated (but will slow transmission speed down!) and
ping times of 1000ms are just as navigable as 10ms ones.

* Repo: https://github.com/pepa65/qft
* After (and compatible with): https://github.com/tudbut/qft
* License: GPLv3+
* Programmed in 100% Rust

## Build
### Static musl binary from repo
```
# After git-cloning the repo
rustup target add x86_64-unknown-linux-musl
cargo build --release --verbose --target=x86_64-unknown-linux-musl
```

## Usage
### Helper
* The QFT helper is specified by and internet address and port, and helps with
  connecting both parties, even when both are behind a NAT/router. (A helper can
  also be on the LAN if both sender and receiver are on the same LAN.) You can use
  and trust publicly provided QFT helpers. They only get to know the IP addresses
  and the temporary random ports, and the chosen PASSWORD for the exchange.
* The hardcoded default helper is `tudbut.de:4277`.
* The default helper can be specified in the environment variable `QFT_HELPER`:
  `export QFT_HELPER=qft.4e4.in:1999`.
* A helper can be run on any machine that has the chosen port open to the internet:
  `qft helper PORT` and if the PORT number is higher than 1024, no privilege is needed.
  Running the helper is very low on CPU and bandwith resources, it is not involved in the
  data transfer, it just establishes the connection based on the same PASSWORD and exchanges
  the IP address and port for each machine. This is different from `croc`, `portal` or all
  the `wormhole` applications that all need an actual transfer relay server (unless in some
  cases, both machines are on the same LAN).

### Sending / Receiving
* On the sending machine, enter `qft S FILE PASSWORD HELPER` (HELPER is optional, see above.
  FILE is a filename, and PASSWORD can be freely chosen, but must be the same on both sides.)
* On the receiving machine, enter `qft R FILE PASSWORD HELPER` (again, HELPER is optional,
  FILE can be a different name that the file will be saved as, and PASSWORD must be the same.)
* Both machines should start transferring after a short while. If they don't, try again.

### Full options
* `qft help` - Just displays a help text.
* `qft version` - Just displays the version number.
* `qft helper PORT` - See **Helper** above.
* `qft send FILE PASSWORD [HELPER [DELAY [BITRATE [SKIP]]]]`
* `qft receive FILE PASSWORD [HELPER [BITRATE [SKIP]]]`
  - `HELPER` is of the form `ADDRESS:PORT`, so `tudbut.de:4277` or `4e4.in:4444`.
  - The optional arguments of `send` & `receive` to the left need to be there for the optional
    argument more to the right to be properly identified. So if `DELAY` needs to be specified, then
    `HELPER` also needs to be supplied.
  - `DELAY` can be lowered from the default 500 to speed up transfers (lowering the delay between
    packets), but too low will increase unreliability of the data integrity!
  - `BITRATE` can be increased from the default 256 to increase packet size (but servers or routers
    set limits at various sizes!)

### Help text
```
qft v0.6.1 - Quick file transfer
Usage:  qft COMMAND ARGUMENT...
    COMMAND:
        H | helper  PORT
        S | send  FILE PASSWORD [ADDRESS:PORT [DELAY [BITRATE [SKIP]]]]
        R | receive  FILE PASSWORD [ADDRESS:PORT [BITRATE [SKIP]]]
        V | version
        help
```

### Environment variables
* Variables need to be explicitly `export`ed to be recognized as environment variables.
* If `QFT_STREAM` is set, the sender can use `/dev/stdin` as the FILE to be sending from and data
  can be directed in.
* Setting `QFT_HIDE_DROPS` suppresses reporting on drops at both the sending and the receiving end.
* When `QFT_USE_TIMED_HOLEPUNCH` is set **on both ends!** a different transfer mechanism is used,
  meant to help with bad connections. It is a fallback, not recommended for general use.

## Troubleshooting
### Resume a fully stopped transfer
You most likely never needed unless the transfer completely died due to a very long pause or a
computer reboot. Then: Ctrl-C whenever `qft` is still running, and start the same command while
specifying SKIP. This means HELPER, BITRATE, and DELAY on the sender also need to be supplied.
HELPER could be different from before (as long as it's the same on both ends), BITRATE **must**
be the same (the default is 256) and DELAY could be different (default is 500).

### It says `Connecting...` but doesn't connect
One of the ends was not properly connected to the helper. Stop `qft` on both ends and try again
(preferably with a different PASSWORD).

### [Relevant XKCD](https://xkcd.com/949)
![Relevant XKCD Image](https://imgs.xkcd.com/comics/file_transfer.png "Every time you email a file to yourself so you can pull it up on your friend&#39;s laptop, Tim Berners-Lee sheds a single tear.")

