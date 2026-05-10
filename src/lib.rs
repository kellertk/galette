//
// lib.rs: The Galette GAL assembly library.
//
// In short, Galette takes a set of equations representing the logic
// you want a GAL to implement, and generates a JEDEC file that can be
// programmed into the GAL in order to make it implement those
// equations.
//
// The galette binary is a thin wrapper around "assemble", but if you
// want to programmatically generate GAL assembly files, you should be
// able to use the publicly exposed members of the library, starting
// from a parser::Content or a blueprint::Blueprint, depending on what
// you want to start with.
//

use std::{
    fs,
    io::{self, BufRead, IsTerminal, Write},
};

use anyhow::Context;

pub mod blueprint;
pub mod chips;
pub mod errors;
pub mod gal;
pub mod gal_builder;
pub mod optimize;
pub mod parser;
pub mod writer;

pub fn assemble(file_name: &str, config: &writer::Config) -> Result<(), errors::FileError> {
    (|| {
        let content = parser::parse(file_name)?;
        let blueprint = blueprint::Blueprint::from(&content)?;

        if config.disable_unused_pt && !blueprint.chip.has_ptd_fuses() {
            eprintln!(
                "{}: warning: --ptd has no effect on {} (PTD fuses only exist on GAL16V8/GAL20V8)",
                file_name,
                blueprint.chip.name()
            );
        }

        let gal = gal_builder::build(&blueprint, config.disable_unused_pt)?;
        writer::write_files(file_name, config, &blueprint.pins, &blueprint.olmcs, &gal).unwrap();

        Ok(())
    })()
    .map_err(|err| errors::FileError {
        file: file_name.into(),
        err,
    })
}

pub enum OptimizeOutcome {
    Cancelled,
    NoChanges,
    Wrote(usize),
}

pub fn optimize_interactive(file_name: &str) -> anyhow::Result<OptimizeOutcome> {
    let content = parser::parse(file_name).map_err(|err| errors::FileError {
        file: file_name.into(),
        err,
    })?;
    let chip = content.chip;
    let pins = content.pins;
    let eqns = content.eqns;

    let raw = fs::read_to_string(file_name)
        .with_context(|| format!("reading {}", file_name))?;
    let mut lines: Vec<String> = raw.split('\n').map(|s| s.to_string()).collect();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let is_tty = stdin.is_terminal();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();

    let mut accepted: Vec<(errors::LineNum, errors::LineNum, String)> = Vec::new();
    let mut had_candidates = false;
    let mut accept_all = false;
    if !is_tty {
        eprintln!(
            "warning: stdin is not a terminal; auto-accepting all optimizer suggestions"
        );
        accept_all = true;
    }

    for eqn in &eqns {
        let term =
            blueprint::eqn_to_term(chip, eqn).map_err(|code| errors::FileError {
                file: file_name.into(),
                err: errors::Error {
                    code,
                    line: eqn.line_num,
                },
            })?;

        let allow_flip = matches!(eqn.lhs, parser::LHS::Pin(_));
        let Some(opt) = optimize::optimize_term(&term, allow_flip) else {
            continue;
        };
        had_candidates = true;

        let new_lhs = if opt.flipped_polarity {
            optimize::flip_lhs_polarity(&eqn.lhs)
        } else {
            eqn.lhs.clone()
        };
        let new_eqn = optimize::term_to_equation(
            &opt.term,
            new_lhs,
            chip,
            eqn.line_num,
            eqn.end_line,
        );

        let orig_text = optimize::format_equation(eqn, &pins);
        let new_text = optimize::format_equation(&new_eqn, &pins);

        writeln!(stdout_lock)?;
        writeln!(stdout_lock, "  line {}:", eqn.line_num)?;
        writeln!(stdout_lock, "    before: {orig_text}")?;
        writeln!(stdout_lock, "     after: {new_text}")?;
        writeln!(
            stdout_lock,
            "            {} -> {} products, {} -> {} literals{}",
            opt.products_before,
            opt.products_after,
            opt.literals_before,
            opt.literals_after,
            if opt.flipped_polarity {
                " (polarity flipped)"
            } else {
                ""
            },
        )?;

        if accept_all {
            writeln!(stdout_lock, "    [auto-accepted]")?;
            accepted.push((eqn.line_num, eqn.end_line, new_text));
            continue;
        }

        write!(
            stdout_lock,
            "    [u]se optimized / [a]ccept all / [k]eep / [c]ancel > "
        )?;
        stdout_lock.flush()?;

        let mut input = String::new();
        if stdin_lock.read_line(&mut input)? == 0 {
            writeln!(stdout_lock, "(EOF) cancelled")?;
            return Ok(OptimizeOutcome::Cancelled);
        }
        let choice = input
            .trim()
            .chars()
            .next()
            .map(|c| c.to_ascii_lowercase());
        match choice {
            Some('c') => {
                writeln!(stdout_lock, "Cancelled.")?;
                return Ok(OptimizeOutcome::Cancelled);
            }
            Some('a') => {
                accept_all = true;
                accepted.push((eqn.line_num, eqn.end_line, new_text));
            }
            Some('u') => {
                accepted.push((eqn.line_num, eqn.end_line, new_text));
            }
            _ => {}
        }
    }

    if accepted.is_empty() {
        if had_candidates {
            writeln!(stdout_lock, "No changes, continuing")?;
        } else {
            writeln!(stdout_lock, "Equations are already optimized.")?;
        }
        return Ok(OptimizeOutcome::NoChanges);
    }

    let confirmed = if is_tty {
        write!(
            stdout_lock,
            "\nWrite {} change{} to {file_name}? [y/N] ",
            accepted.len(),
            if accepted.len() == 1 { "" } else { "s" }
        )?;
        stdout_lock.flush()?;
        let mut input = String::new();
        stdin_lock.read_line(&mut input)?;
        input
            .trim()
            .chars()
            .next()
            .map(|c| c.to_ascii_lowercase())
            == Some('y')
    } else {
        true
    };
    if !confirmed {
        writeln!(stdout_lock, "Not written.")?;
        return Ok(OptimizeOutcome::NoChanges);
    }

    let bak_path = format!("{file_name}.bak");
    fs::copy(file_name, &bak_path)
        .with_context(|| format!("writing backup {bak_path}"))?;

    // Reverse splice order keeps earlier indices valid.
    accepted.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (start, end, new_text) in &accepted {
        let s = start - 1;
        let e = end - 1;
        lines.splice(s..=e, std::iter::once(new_text.clone()));
    }

    let joined = lines.join("\n");
    let tmp_path = format!("{file_name}.tmp");
    fs::write(&tmp_path, &joined)
        .with_context(|| format!("writing {tmp_path}"))?;
    fs::rename(&tmp_path, file_name)
        .with_context(|| format!("renaming {tmp_path} -> {file_name}"))?;

    writeln!(stdout_lock, "{file_name} backed up as {bak_path}")?;
    writeln!(stdout_lock, "Saved updated equations to source")?;
    Ok(OptimizeOutcome::Wrote(accepted.len()))
}
