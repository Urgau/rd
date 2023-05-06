use anyhow::{Context as _, Result};
use clap::Parser;
use log::{info, LevelFilter};
use rustdoc_types::*;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

mod html;
mod pp;

/// Experimental frontend for the rustdoc json output format
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Opt {
    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Open the generated documentation if successful
    #[arg(long)]
    open: bool,

    /// Output directory of html files
    #[arg(short, long)]
    output: PathBuf,

    /// Rustdoc json input file to process
    #[arg(name = "FILE", required = true)]
    files: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let opt = Opt::parse();

    env_logger::builder()
        .filter_level(match opt.verbose {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        })
        .try_init()
        .context("setting env logger failed")?;

    info!("creating the output directory: {:?}", &opt.output);
    let _ = std::fs::create_dir(&opt.output);

    let outputs = opt
        .files
        .iter()
        .map(|file| {
            info!("opening input file: {:?}", &file);
            let reader = File::open(&file).context("The file provided doesn't exists")?;
            let bufreader = BufReader::new(reader);

            info!("starting deserialize of the file");
            let krate: Crate = serde_json::from_reader(bufreader)
                .context("Unable to deseriliaze the content of the file")?;

            let krate_item = krate
                .index
                .get(&krate.root)
                .context("Unable to find the crate item")?;

            html::render::render(&opt, &krate, krate_item)
        })
        .collect::<Result<Vec<_>>>()?;

    let global_index = html::render::render_global(&opt, &outputs)
        .context("Unable to write the global context (js, css, imgs, ...)")?;

    if opt.open {
        open::that(match outputs[..] {
            [ref module_index] => module_index,
            _ => &global_index,
        })?;
    }

    Ok(())
}
