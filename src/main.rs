use std::env;

use clap::{Arg, Command};

mod commands;

#[tokio::main]
async fn main() {
    env::set_var("RUST_BACKTRACE", "1");
    let command_match = Command::new("xyi")
        .about("coreutils collection")
        .version("0.1.0")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .author("aitorru")
        // Copy command
        .subcommand(
            Command::new("copy")
                .short_flag('C')
                .long_flag("copy")
                .about("copy files and directories respecting existing files but comparing them")
                .arg(
                    Arg::new("from")
                        .short('f')
                        .long("from")
                        .help("Where to copy from")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("to")
                        .short('t')
                        .long("to")
                        .help("Where to copy to")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("force")
                        .short('F')
                        .long("force")
                        .help("Force copy even if the file exists")
                        .required(false)
                        .num_args(0),
                )
                .arg(
                    Arg::new("skip")
                        .short('s')
                        .long("skip")
                        .help("Skip copy if file skip but does not check if the file is the same")
                        .required(false)
                        .num_args(0),
                )
                .arg(
                    Arg::new("threads")
                        .short('T')
                        .long("threads")
                        .help("Number of threads to use")
                        .required(false)
                        .num_args(1),
                )
                .arg(
                    Arg::new("hash")
                        .short('H')
                        .long("hash")
                        .help("Check the hash of the local file and the remote file before copying")
                        .required(false)
                        .num_args(0),
                ),
        )
        .get_matches();

    match command_match.subcommand() {
        Some(("copy", copy_match)) => {
            let from = copy_match.get_one::<String>("from").unwrap();
            let to = copy_match.get_one::<String>("to").unwrap();
            let force = *copy_match.get_one::<bool>("force").unwrap();
            let skip = *copy_match.get_one::<bool>("skip").unwrap();
            let hash_check = *copy_match.get_one::<bool>("hash").unwrap();
            let threads = copy_match.get_one::<String>("threads");
            let threads = match threads {
                Some(threads) => threads,
                None => "2",
            };
            env::set_var("RAYON_NUM_THREADS", threads.to_string());
            commands::copy::entry(from.to_string(), to.to_string(), force, skip, hash_check).await;
        }
        _ => {}
    }
}
