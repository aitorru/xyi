pub async fn entry(url: &str) {
    // Open a websocket connection to the url
    let (mut tx, _) = match tungstenite::connect(url) {
        Ok((tx, rx)) => (tx, rx),
        Err(error) => {
            eprintln!("Error connecting to websocket: {}", error);
            return;
        }
    };

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
}
