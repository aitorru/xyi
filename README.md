# xyi

## Install

```bash
cargo install --git https://github.com/aitorru/xyi
```

## Coreutils

A collection of utils that you may need.

- [x] Copy files
- [x] Serve static files

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
    -h, --help               Print help

```

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
