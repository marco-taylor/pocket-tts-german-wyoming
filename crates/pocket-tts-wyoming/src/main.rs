use anyhow::{Context, Result};
use pocket_tts_wyoming::{server, settings::Settings};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "healthcheck") {
        let host = args.get(1).map(String::as_str).unwrap_or("127.0.0.1");
        let port = args
            .get(2)
            .cloned()
            .unwrap_or_else(|| std::env::var("WYOMING_PORT").unwrap_or_else(|_| "10204".into()))
            .parse()
            .context("healthcheck port")?;
        return server::healthcheck(host, port);
    }
    anyhow::ensure!(
        args.is_empty(),
        "usage: pocket-tts-wyoming [healthcheck [host] [port]]"
    );
    anyhow::ensure!(
        std::is_x86_feature_detected!("avx2"),
        "N100 build requires AVX2"
    );
    server::run(Settings::from_env()?)
}
