rd
==

[<img alt="github" src="https://img.shields.io/badge/github-urgau/rd-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/urgau/rd)
[<img alt="license" src="https://img.shields.io/badge/license-BSD%2BPatent-blue?style=for-the-badge" height="20">](https://github.com/urgau/rd/)
[<img alt="build status" src="https://img.shields.io/github/workflow/status/Urgau/rd/Continuous integration/main?style=for-the-badge" height="20">](https://github.com/urgau/rd/actions?query=branch%3Amain)

This project is a POC/experimental frontend for the [rustdoc json](https://rust-lang.github.io/rfcs/2963-rustdoc-json.html) output format to generate html documentation.

## Demos

 - [regex](http://urgau.rf.gd/rd/regex/index.html) ([docs.rs](https://docs.rs/regex/1.5.4/regex/))
 - [anyhow](http://urgau.rf.gd/rd/anyhow/index.html) ([docs.rs](https://docs.rs/anyhow/1.0.43/anyhow/))

## Features

- [X] Pretty pritting of items (methods, structs, traits, ...)
- [X] Minimal self-contained search engine with index
- [X] Bootstrap 5 html pages
- [X] Syntax highlighting of items
- [X] Navigation between items (even external if available)
- [X] Improved markdown output (similar to rustdoc)
- [X] Table of contents (markdown + items)
- [X] Deprecation notice and attributes filtering
- [ ] `doc(cfg)` parsing and pritting
- [ ] Source code inclusion
- [ ] Themes or options

## Usage

```bash
rd 0.1.0

USAGE:
    rd [FLAGS] [OPTIONS] <input>

FLAGS:
    -h, --help       Prints help information
        --open       Open the generated documentation if successful
    -V, --version    Prints version information
    -v, --verbose    Verbose mode (-v, -vv, -vvv, etc.)

OPTIONS:
    -o, --output <output>    Output directory of html files [default: .]

ARGS:
    <input>    Rustdoc json input file to process
```

### Generating a rustdoc-json output

Generating the json output format currently requires a nightly toolchain.

```
$ RUSTDOCFLAGS="-Z unstable-options --output-format json" cargo +nightly doc
```

You should see in the `target/doc` directory a file called `MY_CRATE.json`, that's the json rustdoc output. This file will be used by `rd` to generate the documentation.

### Generating the HTML output with rd

```
$ cargo run -- -v --output html/ --open my_crate.json
```

#### License

<sup>
Licensed under the <a href="LICENSE">BSD+Patent</a> license.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, shall be licensed under the BSD+Patent license
without any additional terms or conditions.
</sub>
