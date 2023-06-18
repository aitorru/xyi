use teloxide::prelude::*;

pub async fn entry(token: &str, chat: &str, message: &str) {
    let bot = Bot::new(token);

    bot.send_message(chat.to_owned(), message)
        .send()
        .await
        .unwrap();
}
