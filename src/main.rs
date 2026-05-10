//
// main.rs: Entry point for the galette binary.
//
// While galette is written to be usable as a library, it also
// provides a command-line interface that is intended to be largely
// compatible with galette's.
//

use std::process;

use clap::Parser;

use galette::writer;

#[derive(Parser)]
#[command(
    name = "Galette (galette-tk)",
    version,
    about = "GALasm-compatible GAL assembler"
)]
struct Cli {
    /// Input file
    #[arg(name = "INPUT.pld")]
    input: String,

    /// Enable security fuse
    #[arg(short, long)]
    secure: bool,

    /// Disable .chp file output
    #[arg(short = 'c', long)]
    nochip: bool,

    /// Disable .fus file output
    #[arg(short = 'f', long)]
    nofuse: bool,

    /// Disable .pin file output
    #[arg(short = 'p', long)]
    nopin: bool,

    /// Set product term disable bits (16V8/20V8 only)
    #[arg(long)]
    ptd: bool,
}

fn main() {
    let cli = Cli::parse();

    let config = writer::Config {
        gen_fuse: !cli.nofuse,
        gen_chip: !cli.nochip,
        gen_pin: !cli.nopin,
        jedec_sec_bit: cli.secure,
        disable_unused_pt: cli.ptd,
    };

    if let Err(e) = galette::assemble(&cli.input, &config) {
        eprintln!("{}", e);
        process::exit(1);
    }
}
