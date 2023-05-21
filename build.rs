fn main() {
    // Download the preact source from the url
    let preact_src = reqwest::blocking::get("https://unpkg.com/preact?module")
        .unwrap()
        .text()
        .unwrap();
    // Write the preact source into a file
    let _ = std::fs::write("./src/assets/serve/preact.mjs", preact_src);
    // Download the htm source from the url
    let htm_src = reqwest::blocking::get("https://unpkg.com/htm?module")
        .unwrap()
        .text()
        .unwrap();
    // Write the htm source into a file
    let _ = std::fs::write("./src/assets/serve/htm.mjs", htm_src);
}
