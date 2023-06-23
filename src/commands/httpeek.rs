use tokio::time::Instant;

pub async fn entry(url: &str, user_agent: &str) {
    // Meassure the time it takes to make the request
    let start = Instant::now();
    // Create a client with a custom user agent
    let client = reqwest::Client::builder()
        .user_agent(user_agent)
        .build()
        .unwrap();
    // Make a request to the url using reqwest and print the http method and status code
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Server error: {}", error);
            return;
        }
    };
    println!(
        "URL: {}\nStatus: {} {}\nHTTP/2: {}\nHSTS: {}\nTime: {}ms\n",
        url,
        response.status().as_str(),
        response.status().canonical_reason().unwrap(),
        response.version() == reqwest::Version::HTTP_2,
        response
            .headers()
            .get("strict-transport-security")
            .is_some(),
        start.elapsed().as_millis()
    );
}
