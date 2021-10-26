use anyhow::{Context as _, Result};
use log::{info, LevelFilter};
use rustdoc_types::*;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use structopt::StructOpt;

mod html;
mod pp;

/// Commande-line options
#[derive(StructOpt)]
pub(crate) struct Opt {
    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,

    /// Open the generated documentation if successful
    #[structopt(long)]
    open: bool,

    /// Rustdoc json input file to process
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output directory of html files
    #[structopt(short, long, parse(from_os_str), default_value = ".")]
    output: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    env_logger::builder()
        .filter_level(match opt.verbose {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        })
        .try_init()
        .context("setting env logger failed")?;

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

    let krate_index_path = html::render::render(&opt, &krate, krate_item)?;

    if opt.open {
        open::that(krate_index_path)?;
    }

    Ok(())
}
