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
                )
                .arg(
                    Arg::new("log")
                        .short('l')
                        .long("log")
                        .help("Destination of logs")
                        .required(false)
                        .num_args(1),
                ),
        )
        .subcommand(
            Command::new("serve")
                .about("serve files in the current directory using HTTP")
                .short_flag('S')
                .long_flag("serve")
                .arg(
                    Arg::new("port")
                        .short('p')
                        .long("port")
                        .help("Port to serve")
                        .required(false)
                        .num_args(1),
                )
                .arg(
                    Arg::new("dir")
                        .short('d')
                        .long("dir")
                        .help("Directory to start serving")
                        .required(false)
                        .num_args(1),
                ),
        )
        .subcommand(
            Command::new("download")
                .about("download files from a remote server")
                .short_flag('D')
                .long_flag("download")
                .arg(
                    Arg::new("url")
                        .short('u')
                        .long("url")
                        .help("URL to download from")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("to")
                        .short('t')
                        .long("to")
                        .help("Where to download to")
                        .required(false)
                        .num_args(1),
                ),
        )
        .subcommand(
            Command::new("telegram")
                .about("send a message to a telegram chat")
                .short_flag('T')
                .long_flag("telegram")
                .arg(
                    Arg::new("token")
                        .short('t')
                        .long("token")
                        .help("Telegram bot token")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("chat")
                        .short('c')
                        .long("chat")
                        .help("Telegram chat id")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("message")
                        .short('m')
                        .long("message")
                        .help("Message to send")
                        .required(true)
                        .num_args(1),
                ),
        )
        .subcommand(
            Command::new("httpeek")
                .about("peek into http servers")
                .short_flag('H')
                .long_flag("httpeek")
                .arg(
                    Arg::new("url")
                        .short('u')
                        .long("url")
                        .help("URL to peek into")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("agent")
                        .short('a')
                        .long("agent")
                        .help("User agent to use")
                        .required(false)
                        .num_args(1),
                )
                .arg(
                    Arg::new("print")
                        .short('o')
                        .long("print")
                        .help("Print output to stdout")
                        .required(false)
                        .num_args(0),
                ),
        )
        .subcommand(
            Command::new("wscat")
                .about("Print to stdout the output of a websocket server")
                .short_flag('W')
                .long_flag("wscat")
                .arg(
                    Arg::new("url")
                        .short('u')
                        .long("url")
                        .help("URL to connect to")
                        .required(true)
                        .num_args(1),
                ),
        )
        // ws chat room
        .get_matches();

    // Set backtrace to short if not debug
    #[cfg(not(debug_assertions))]
    env::set_var("RUST_BACKTRACE", "0");

    #[cfg(debug_assertions)]
    env::set_var("RUST_BACKTRACE", "full");

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
            let logs = copy_match.get_one::<String>("log");
            let logs = match logs {
                Some(logs) => {
                    // Check if the logs path exists and if it is a directory
                    let logs_path = std::path::PathBuf::from(logs);
                    if logs_path.exists() && logs_path.is_dir() {
                        // Add xyi-copy-[date].log to the logs path
                        let date = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
                        let log_file = logs_path.join(format!("xyi-copy-{}.log", date));
                        // Create the file
                        let _ = std::fs::File::create(&log_file);
                        Some(log_file.to_str().unwrap().to_string())
                    } else if logs_path.exists() && logs_path.is_file() {
                        // If the path does not exist or is not a directory just use the current directory
                        Some(logs_path.to_str().unwrap().to_string())
                    } else {
                        println!("The logs path does not exist or is not a directory");
                        None
                    }
                },
                None => None,
            };
            env::set_var("RAYON_NUM_THREADS", threads.to_string());
            commands::copy::entry(from.to_string(), to.to_string(), force, skip, hash_check, logs).await;
        }
        Some(("serve", serve_match)) => {
            let port = match serve_match.get_one::<String>("port") {
                Some(port) => port,
                None => "8000",
            };
            let starting_dir = match serve_match.get_one::<String>("dir") {
                Some(dir) => dir,
                None => ".",
            };
            commands::serve::entry(port, starting_dir).await;
        }
        Some(("download", download_match)) => {
            let url = download_match.get_one::<String>("url").unwrap();
            let to = download_match.get_one::<String>("to");
            commands::download::entry(url, to).await;
        }
        Some(("telegram", telegram_match)) => {
            let token = telegram_match.get_one::<String>("token").unwrap();
            let chat = telegram_match.get_one::<String>("chat").unwrap();
            let message = telegram_match.get_one::<String>("message").unwrap();
            commands::telegram::entry(token, chat, message).await;
        }
        Some(("httpeek", httpeek_match)) => {
            let url = httpeek_match.get_one::<String>("url").unwrap();
            let user_agent = httpeek_match.get_one::<String>("agent");
            let print_output = *httpeek_match.get_one::<bool>("print").unwrap();
            // If user agent not set just use xyi/v(program version)
            let user_agent = match user_agent {
                Some(user_agent) => user_agent.to_owned(),
                None => {
                    let version = env!("CARGO_PKG_VERSION");
                    format!("xyi/{}", version)
                }
            };
            commands::httpeek::entry(url, &user_agent, print_output).await;
        }
        Some(("wscat", wscat_match)) => {
            let url = wscat_match.get_one::<String>("url").unwrap();
            commands::wscat::entry(url).await;
        }
        _ => {}
    }
}
