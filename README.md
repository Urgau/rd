rd
==

[<img alt="github" src="https://img.shields.io/badge/github-urgau/rd-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/urgau/rd)
[<img alt="license" src="https://img.shields.io/badge/license-BSD%2BPatent-blue?style=for-the-badge" height="20">](https://github.com/urgau/rd/)
[<img alt="build status" src="https://img.shields.io/github/workflow/status/uegau/rd/CI/master?style=for-the-badge" height="20">](https://github.com/urgau/rd/actions?query=branch%3Amaster)

This project is a POC of an frontend fo the [rustdoc json](https://rust-lang.github.io/rfcs/2963-rustdoc-json.html) output format to generate html documentation.

Demo: [regex](http://urgau.rf.gd/rd/regex/index.html) or [anyhow](http://urgau.rf.gd/rd/anyhow/index.html)

## Features

- [X] Pretty pritting of items (methods, structs, traits, ...)
- [X] Minimal self-contained search engine
- [X] HTML output with Bootstrap 5
- [X] Responsive HTML pages
- [X] Syntax highlighting of items
- [X] Navigation between items (even external if possible)
- [X] Improved markdown output (similar to rustdoc)
- [X] Table of content (markdown + items)
- [ ] No `doc` `cfg` parsing or pritting
- [ ] Deprecated and stability notice
- [ ] Source code inclusion
- [ ] No themes or options

## Usage

```bash
rd 0.1.0

USAGE:
    rd [FLAGS] [OPTIONS] <input>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    Verbose mode (-v, -vv, -vvv, etc.)

OPTIONS:
    -o, --output <output>    Output directory of html files [default: .]

ARGS:
    <input>    Rustdoc josn input file to process
```

```bash
$ # Generating the json output format currently requires a nightly toolchain
$ RUSTDOCFLAGS="-Z unstable-options --output-format json" cargo +nightly doc
$ rd --output html/ my_crate.json
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
