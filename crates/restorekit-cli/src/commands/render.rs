use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use restorekit::progress::Event;

/// Emit an event as a single NDJSON line.
pub fn emit_json(event: &Event) {
    println!("{}", serde_json::to_string(event).unwrap());
}

/// Render a firmware-download event: NDJSON in `--json` mode, otherwise a
/// human progress bar. Shared by the `download` and `restore`/`run` commands.
pub fn download(bar: &ProgressBar, event: Event, json: bool) {
    if json {
        emit_json(&event);
        return;
    }
    match event {
        Event::CacheHit { path } => println!("Using cached firmware: {path}"),
        Event::DownloadResumed { received } => {
            println!("Resuming download from {:.1} GB...", received as f64 / 1e9)
        }
        Event::DownloadProgress { received, total } => {
            if bar.length() != Some(total) && total > 0 {
                bar.set_length(total);
                bar.set_style(
                    ProgressStyle::with_template("  {bytes}/{total_bytes} {bar:30} ({eta})")
                        .unwrap(),
                );
                bar.set_draw_target(ProgressDrawTarget::stderr());
            }
            bar.set_position(received);
        }
        Event::Verifying => {
            bar.finish_and_clear();
            println!("Verifying checksum...");
        }
        _ => {}
    }
}
