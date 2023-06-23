pub async fn entry(url: &str) {
    // Open a websocket connection to the url
    let (mut tx, _) = match tungstenite::connect(url) {
        Ok((tx, rx)) => (tx, rx),
        Err(error) => {
            eprintln!("Error connecting to websocket: {}", error);
            return;
        }
    };

    // Spawn an empty tokio task and store it in a variable
    let task = tokio::spawn(async move {
        // read messages from the websocket
        loop {
            if !tx.can_read() {
                println!("Socket closed");
                return;
            }

            let message = match tx.read_message() {
                Ok(message) => message,
                Err(error) => {
                    eprintln!("Error reading message: {}", error);
                    return;
                }
            };
            println!("{}", message.to_text().unwrap());
        }
    });

    ctrlc::set_handler(move || {
        // Cancel the task
        task.abort();
        // Exit the program
        std::process::exit(0);
    })
    .expect("Unable to set Ctrl-C handler");
}
