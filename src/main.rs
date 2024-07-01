use anyhow::{anyhow, Context, Result};
use clap::Parser;
use tracing::*;

use std::sync::{atomic::AtomicBool, mpsc::{channel, sync_channel, Sender}};

mod fft;
mod pipewire;
mod render;

#[derive(Parser)]
struct Args {
    #[clap(short, long, action(clap::ArgAction::Count))]
    verbose: u8,

    #[clap(short, long)]
    width: Option<usize>,

    #[clap(short, long)]
    height: Option<usize>,
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
    let width = args.width.unwrap_or(512);
    let height = args.height.unwrap_or(512);

    let (err_tx, err_rx) = channel();
    let (audio_tx, audio_rx) = sync_channel(8);
    let (rows_tx, rows_rx) = sync_channel(0);
    fanout(&err_tx, "fft", move || fft::run(width, &FFTW_READY, audio_rx, rows_tx));
    fanout(&err_tx, "pipewire", move || pipewire::run(&FFTW_READY, audio_tx));
    fanout(&err_tx, "SDL render", move || {
        render::run(width, height, rows_rx)
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
            s3.send(Err(anyhow!("{name} thread panicked: {p}"))).unwrap();
        }));
        s2.send(f().with_context(|| format!("{name} thread failed"))).unwrap();
    });
}
