use std::sync::{LazyLock, RwLock, mpsc};
use std::thread;

// How many lines we keep before dropping the oldest.
const CAPACITY: usize = 1000;

// The lines the UI renders. Written only by the worker thread below
static LINES: LazyLock<RwLock<Vec<String>>> = LazyLock::new(|| RwLock::new(Vec::new()));

// This is similar logic to the memory interface but here we are using
// it for debug purposes
static SINK: LazyLock<mpsc::Sender<String>> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel::<String>();
    thread::Builder::new()
        .name("logger".to_string())
        .spawn(move || {
            for line in rx {
                let mut lines = LINES.write().unwrap();
                lines.push(line);
                let overflow = lines.len().saturating_sub(CAPACITY);
                if overflow > 0 {
                    lines.drain(..overflow);
                }
            }
        })
        .expect("spawn logger thread");
    tx
});

// Queue a line for the log
pub fn line(msg: impl Into<String>) {
    let _ = SINK.send(msg.into());
}

// Number of lines currently held (after trimming to CAPACITY).
pub fn len() -> usize {
    LINES.read().unwrap().len()
}

// A copy of every line currently held
pub fn snapshot() -> Vec<String> {
    LINES.read().unwrap().clone()
}