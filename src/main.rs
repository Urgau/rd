use anyhow::{Context, Result};
use rustdoc_types::*;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use structopt::StructOpt;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod html;
mod pp;

#[derive(StructOpt)]
struct Opt {
    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,

    /// Input file to process
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output directory to process
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

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

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

    /*let item = krate.index.get(&Id("0:547".to_string())).unwrap();
    println!("{}", pp::Tokens::from_item(item, &krate.index).unwrap());*/

    dump_to(
        format!("{}/style.css", &opt.output.display()),
        include_bytes!("static/css/style.css"),
    )?;
    dump_to(
        format!("{}/rust.svg", &opt.output.display()),
        include_bytes!("static/imgs/rust.svg"),
    )?;
    dump_to(
        format!("{}/search.js", &opt.output.display()),
        include_bytes!("static/js/search.js"),
    )?;

    if let ItemEnum::Module(krate_module) = &krate_item.inner {
        let mut global_context = html::html::GlobalContext {
            krate: &krate,
            output_dir: &opt.output,
            files: Default::default(),
            item_paths: Default::default(),
            krate_name: &krate_item.name.as_ref().context("expect a crate name")?,
        };

        html::html::module_page(&global_context, None, krate_item, krate_module)?;

        let mut search = String::new();

        search.push_str("\n\nconst INDEX = [\n");
        for item in global_context.item_paths.iter_mut() {
            search.push_str("  { components: [ ");
            for (index, c) in item.0.iter().enumerate() {
                if index != 0 {
                    search.push_str(", ");
                }
                search.push_str("{ name: \"");
                search.push_str(&c.name);
                search.push_str("\", lower_case_name: \"");
                search.push_str(&c.name.to_ascii_lowercase());
                search.push_str("\", kind: \"");
                search.push_str(&c.kind);
                search.push_str("\" }");
            }

            let last = item.0.last().unwrap();
            search.push_str(" ], filepath: \"");
            search.push_str(&format!("{}", last.filepath.display()));
            search.push_str("\" },\n");
        }
        search.push_str("\n];\n");

        dump_to(
            format!(
                "{}/{}/search-index.js",
                &opt.output.display(),
                &krate_item.name.as_ref().unwrap()
            ),
            search.as_bytes(),
        )?;
    }

    Ok(())
}

fn dump_to<P: AsRef<std::path::Path>>(path: P, buf: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    std::io::Write::write_all(&mut file, buf)?;
    Ok(())
}
