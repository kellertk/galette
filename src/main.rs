//
// main.rs: Entry point for the galette binary.
//
// While galette is written to be usable as a library, it also
// provides a command-line interface that is intended to be largely
// compatible with galette's.
//

extern crate clap;
extern crate galette;

use clap::{Arg, ArgAction, Command};

use std::process;

use galette::writer;

fn main() {
    let matches = Command::new("Galette")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Simon Frankau <sgf@arbitrary.name>")
        .about("GALasm-compatible GAL assembler")
        .arg(Arg::new("INPUT.pld").help("Input file").required(true))
        .arg(
            Arg::new("secure")
                .short('s')
                .long("secure")
                .help("Enable security fuse")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("nochip")
                .short('c')
                .long("nochip")
                .help("Disable .chp file output")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("nofuse")
                .short('f')
                .long("nofuse")
                .help("Disable .fus file output")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("nopin")
                .short('p')
                .long("nopin")
                .help("Disable .pin file output")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let file_name = matches.get_one::<String>("INPUT.pld").unwrap();

    let config = writer::Config {
        gen_fuse: !matches.get_flag("nofuse"),
        gen_chip: !matches.get_flag("nochip"),
        gen_pin: !matches.get_flag("nopin"),
        jedec_sec_bit: matches.get_flag("secure"),
    };

    if let Err(e) = galette::assemble(file_name, &config) {
        eprintln!("{}", e);
        process::exit(1);
    }
}
