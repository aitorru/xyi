use std::path::Path;

/// Vendored frontend dependencies that the `serve` command embeds via
/// `include_str!`. They are downloaded at build time and cached in
/// `src/assets/serve` (the files are git-ignored).
const ASSETS: &[(&str, &str)] = &[
    (
        "https://unpkg.com/preact?module",
        "./src/assets/serve/preact.mjs",
    ),
    ("https://unpkg.com/htm?module", "./src/assets/serve/htm.mjs"),
];

fn main() {
    for (url, dest) in ASSETS {
        fetch_asset(url, dest);
    }
}

/// Download `url` into `dest`. If the download fails we keep an already cached
/// copy when present, or fall back to an empty placeholder so that the
/// `include_str!` calls still compile (e.g. offline builds). This keeps the
/// build from breaking when unpkg.com is unreachable.
fn fetch_asset(url: &str, dest: &str) {
    match reqwest::blocking::get(url).and_then(|response| response.text()) {
        Ok(body) => {
            if let Err(error) = std::fs::write(dest, body) {
                println!("cargo:warning=could not write {dest}: {error}");
            }
        }
        Err(error) => {
            println!("cargo:warning=could not download {url}: {error}");
            if !Path::new(dest).exists() {
                println!("cargo:warning=writing empty placeholder for {dest}");
                let _ = std::fs::write(dest, "");
            }
        }
    }
}
