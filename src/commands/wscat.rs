use std::sync::{Arc, Mutex};

pub async fn entry(url: &str) {
    // enable_raw_mode().unwrap();
    // // Initialize the terminal
    // let stdout = std::io::stdout();
    // let backend = tui::backend::CrosstermBackend::new(stdout);
    // let mut terminal = match tui::Terminal::new(backend) {
    //     Ok(terminal) => terminal,
    //     Err(error) => {
    //         eprint!("Error initializing terminal: {}", error);
    //         return;
    //     }
    // };
    // // Draw a big block displaying the incoming connections and a small block allowing the user to type messages to send
    // terminal.clear().unwrap();

    let messages = Arc::new(Mutex::new(vec![String::from("")]));

    // Open a websocket connection to the url
    let (mut tx, _) = match tungstenite::connect(url) {
        Ok((tx, rx)) => (tx, rx),
        Err(error) => {
            // TODO: Handle error in tui
            eprintln!("Error connecting to websocket: {}", error);
            return;
        }
    };

    // Clone the messages vector to be able to use it in the tokio tasks
    let messages_input = messages.clone();
    let messages_std_output = messages.clone();

    // Spawn an empty tokio task and store it in a variable
    let message_input_task = tokio::spawn(async move {
        // read messages from the websocket
        loop {
            if !tx.can_read() {
                println!("Socket closed");
                return;
            }

            let message = match tx.read_message() {
                Ok(message) => message,
                Err(error) => {
                    // TODO: Handle error in tui
                    eprintln!("Error reading message: {}", error);
                    return;
                }
            };
            messages_input
                .lock()
                .unwrap()
                .push(message.to_text().unwrap().to_string());
            // println!("{}", message.to_text().unwrap());
        }
    });

    let terminal_ui_task = tokio::spawn(async move {
        loop {
            let message_to_display = messages_std_output.lock().unwrap().join("\n");
            println!("{}", message_to_display);
            // terminal
            //     .draw(|f| {
            //         let chunks = tui::layout::Layout::default()
            //             .direction(tui::layout::Direction::Vertical)
            //             .constraints(
            //                 [
            //                     tui::layout::Constraint::Percentage(75),
            //                     tui::layout::Constraint::Percentage(25),
            //                 ]
            //                 .as_ref(),
            //             )
            //             .split(f.size());
            //         let block = tui::widgets::Block::default()
            //             .title("Incoming connections")
            //             .title_alignment(tui::layout::Alignment::Center)
            //             .borders(tui::widgets::Borders::ALL)
            //             .border_type(tui::widgets::BorderType::Rounded);
            //         let paragraph = tui::widgets::Paragraph::new(message_to_display.as_str());
            //         f.render_widget(paragraph, block.inner(chunks[0]));
            //         f.render_widget(block, chunks[0]);
            //         let block = tui::widgets::Block::default()
            //             .title("Sender")
            //             .title_alignment(tui::layout::Alignment::Left)
            //             .borders(tui::widgets::Borders::ALL)
            //             .border_type(tui::widgets::BorderType::Rounded);
            //         f.render_widget(block, chunks[1]);
            //     })
            //     .unwrap();
            // Wait for 200ms before redrawing the UI
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });

    ctrlc::set_handler(move || {
        // Cancel the task
        message_input_task.abort();
        terminal_ui_task.abort();
        // Exit the program
        std::process::exit(0);
    })
    .expect("Unable to set Ctrl-C handler");
}
