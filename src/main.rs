use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tracing::*;

use std::sync::{
    atomic::AtomicBool,
    mpsc::{Sender, channel, sync_channel},
};

mod fft;
mod pipewire;
mod render;

pub const SAMPLE_RATE: usize = 44100;

pub const QUANTUM: usize = 1024;

#[derive(Parser)]
struct Args {
    #[clap(short, long, action(clap::ArgAction::Count))]
    verbose: u8,

    /// DFT width
    #[clap(short, long, default_value_t = 1024)]
    width: usize,

    /// History length
    #[clap(long, default_value_t = 1024)]
    height: usize,

    /// Upper frequency to display
    #[clap(short, long, default_value_t = 20000)]
    upper: usize,

    // Render in mel scale instead of Hertz
    #[clap(short, long)]
    mels: bool,
}

static FFTW_READY: AtomicBool = AtomicBool::new(false);

fn main() {
    let args = Args::parse();
    let level = match args.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
    if let Err(e) = run(args) {
        error!("{e:?}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    let Args {
        width,
        height,
        upper,
        mels,
        ..
    } = args;

    let (err_tx, err_rx) = channel();
    let (audio_tx, audio_rx) = sync_channel(8);
    let (rows_tx, rows_rx) = sync_channel(8);
    fanout(&err_tx, "fft", move || {
        fft::run(width, &FFTW_READY, audio_rx, rows_tx)
    });
    fanout(&err_tx, "pipewire", move || {
        pipewire::run(&FFTW_READY, audio_tx)
    });
    fanout(&err_tx, "SDL render", move || {
        render::run(width, height, upper, mels, rows_rx)
    });
    drop(err_tx);

    // Return whatever the first guy does.
    err_rx.recv().unwrap() // No way all senders would hang up without sending, see fanout()
}

fn fanout<F>(s: &Sender<Result<()>>, name: &'static str, f: F)
where
    F: FnOnce() -> Result<()> + Send + 'static,
{
    let s2 = s.clone();
    std::thread::spawn(move || {
        let s3 = s2.clone();
        std::panic::set_hook(Box::new(move |p| {
            let _ = s3.send(Err(anyhow!("{name} thread panicked: {p}")));
        }));
        s2.send(f().with_context(|| format!("{name} thread failed")))
            .unwrap();
    });
}
