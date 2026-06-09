# xyi

A collection of cross-platform coreutils you may need, bundled into a single binary.

## Install

```bash
cargo install --git https://github.com/aitorru/xyi
```

Pre-built binaries (Linux and Windows 32/64-bit) are also published on the
[releases page](https://github.com/aitorru/xyi/releases).

## Coreutils

A collection of utils that you may need.

- [x] Copy files
- [x] Serve static files
- [x] Download files from the internet. Like wget.
- [x] Send telegram messages
- [x] Peek into HTTP servers
- [x] Read from a websocket server (wscat)

### copy - (respect the origin)

The `copy` command is a wrapper around `cp` that will respect the origin of the file.

It comes with lots of options that can help you achive your task.

```
copy files and directories respecting existing files but comparing them

Usage: xyi.exe {copy|--copy|-C} [OPTIONS] --from <from> --to <to>

    Options:
    -f, --from <from>        Where to copy from
    -t, --to <to>            Where to copy to
    -F, --force              Force copy even if the file exists
    -s, --skip               Skip copy if file skip but does not check if the file is the same
    -T, --threads <threads>  Number of threads to use
    -H, --hash               Check the hash of the local file and the remote file before copying
    -l, --log <log>          Destination of logs
    -i, --index <index>      Path to an index cache file
    -h, --help               Print help

```

#### Index cache

The first thing `copy` does is scan the source tree to build the index of files
to copy. With `--index <file>` that index is persisted: if the file already
exists it is reused as-is and the scan is skipped, which speeds up repeated runs
over a large source tree. Otherwise the freshly scanned index is written there
for next time.

The cache is trusted as-is and is **not** revalidated against the source, so
files added to or removed from the source after the index was written will not
be detected. Delete the index file to force a fresh scan.

### serve

Static file server.

```
serve files in the current directory using HTTP

Usage: xyi {serve|--serve|-S} [OPTIONS]

Options:
  -p, --port <port>  Port to serve
  -d, --dir <dir>    Directory to start serving
  -h, --help         Print help
```

### download

Download files from the internet.

```
download files from a remote server

Usage: xyi.exe {download|--download|-D} [OPTIONS] --url <url>

Options:
  -u, --url <url>  URL to download from
  -t, --to <to>    Where to download to
  -h, --help       Print help
```

### telegram

Send a message to a telegram chat.

```
send a message to a telegram chat

Usage: xyi.exe {telegram|--telegram|-T} --token <token> --chat <chat> --message <message>

Options:
  -t, --token <token>      Telegram bot token
  -c, --chat <chat>        Telegram chat id
  -m, --message <message>  Message to send
  -h, --help               Print help
```

### httpeek

Peek into HTTP servers and report status, HTTP/2 support, HSTS and response time.

```
peek into http servers

Usage: xyi {httpeek|--httpeek|-H} [OPTIONS] --url <url>

Options:
  -u, --url <url>      URL to peek into
  -a, --agent <agent>  User agent to use
  -o, --print          Print output to stdout
  -h, --help           Print help
```

### wscat

Print to stdout the output of a websocket server.

```
Print to stdout the output of a websocket server

Usage: xyi {wscat|--wscat|-W} --url <url>

Options:
  -u, --url <url>  URL to connect to
  -h, --help       Print help
```