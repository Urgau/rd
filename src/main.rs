use anyhow::{Context as _, Result};
use rustdoc_types::*;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use structopt::StructOpt;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod html;
mod pp;

/// Commande-line options
#[derive(StructOpt)]
pub(crate) struct Opt {
    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,

    /// Rustdoc josn input file to process
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output directory of html files
    #[structopt(short, long, parse(from_os_str), default_value = ".")]
    output: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(match opt.verbose {
            0 => Level::INFO,
            1 => Level::DEBUG,
            _ => Level::TRACE,
        })
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .context("setting default subscriber failed")?;

    info!("opening input file: {:?}", &opt.input);
    let reader = File::open(&opt.input).context("The file provided doesn't exists")?;
    let bufreader = BufReader::new(reader);

    info!("starting deserialize of the file");
    let krate: Crate = serde_json::from_reader(bufreader)
        .context("Unable to deseriliaze the content of the file")?;

    let krate_item = krate
        .index
        .get(&krate.root)
        .context("Unable to find the crate item")?;

    html::render::render(&opt, &krate, krate_item)?;
    Ok(())
}
