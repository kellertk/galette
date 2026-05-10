//
// main.rs: Entry point for the galette binary.
//
// While galette is written to be usable as a library, it also
// provides a command-line interface that is intended to be largely
// compatible with galette's.
//

use std::{fs, path::PathBuf, process};

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

    /// Run interactive equation optimizer before assembly
    #[arg(short = 'o', long)]
    optimize: bool,

    /// Print .chp and .pin files after assembly
    #[arg(short = 'v', long)]
    verbose: bool,
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

    if cli.optimize {
        match galette::optimize_interactive(&cli.input) {
            Ok(galette::OptimizeOutcome::Cancelled) => process::exit(0),
            Ok(_) => {}
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    }

    if let Err(e) = galette::assemble(&cli.input, &config) {
        eprintln!("{}", e);
        process::exit(1);
    }

    if cli.verbose {
        let base = PathBuf::from(&cli.input);
        for ext in ["chp", "pin"] {
            let path = base.with_extension(ext);
            match fs::read_to_string(&path) {
                Ok(content) => {
                    println!("=== {} ===", path.display());
                    print!("{content}");
                    if !content.ends_with('\n') {
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("warning: could not read {}: {}", path.display(), e);
                }
            }
        }
    }

    println!("Assembly complete");
}
