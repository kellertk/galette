//
// optimize_test.rs: End-to-end coverage for `galette -o`.
//
// Piped stdin is non-TTY, so these tests exercise the auto-accept
// path. Interactive prompt branches need a real terminal.
//

use std::fs::{self, create_dir_all, read_to_string, remove_dir_all};
use std::path::Path;
use std::process::Stdio;

use anyhow::{Result, bail};
use test_bin::get_test_bin;

const FIXTURE_DIR: &str = "testcases/optimize";
const FIXTURE_NAME: &str = "non_minimal.pld";
const EXPECTED_NAME: &str = "non_minimal.expected.pld";

fn ensure_dir_exists(name: &str) -> Result<()> {
    if Path::new(name).exists() {
        remove_dir_all(name)?;
    }
    create_dir_all(name)?;
    Ok(())
}

#[test]
fn optimize_auto_accepts_writes_and_assembles() -> Result<()> {
    let dir = "test_temp_opt_auto";
    ensure_dir_exists(dir)?;
    fs::copy(
        format!("{FIXTURE_DIR}/{FIXTURE_NAME}"),
        format!("{dir}/{FIXTURE_NAME}"),
    )?;

    let out = get_test_bin("galette")
        .current_dir(dir)
        .args(["-o", FIXTURE_NAME])
        .stdin(Stdio::null())
        .output()?;
    if !out.status.success() {
        bail!(
            "command failed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("auto-accepting"),
        "expected auto-accept warning on stderr; got: {stderr:?}"
    );

    let after = read_to_string(format!("{dir}/{FIXTURE_NAME}"))?;
    let expected = read_to_string(format!("{FIXTURE_DIR}/{EXPECTED_NAME}"))?;
    assert_eq!(after, expected, "rewritten .pld did not match expected");

    let bak = read_to_string(format!("{dir}/{FIXTURE_NAME}.bak"))?;
    let original = read_to_string(format!("{FIXTURE_DIR}/{FIXTURE_NAME}"))?;
    assert_eq!(bak, original, ".bak did not preserve the original");

    let optimize_jed = read_to_string(format!("{dir}/non_minimal.jed"))?;

    let direct_dir = format!("{dir}_direct");
    ensure_dir_exists(&direct_dir)?;
    fs::copy(
        format!("{FIXTURE_DIR}/{EXPECTED_NAME}"),
        format!("{direct_dir}/{FIXTURE_NAME}"),
    )?;
    let direct_out = get_test_bin("galette")
        .current_dir(&direct_dir)
        .arg(FIXTURE_NAME)
        .output()?;
    assert!(direct_out.status.success(), "direct assembly failed");
    let direct_jed = read_to_string(format!("{direct_dir}/non_minimal.jed"))?;
    assert_eq!(
        optimize_jed, direct_jed,
        "optimize-then-assemble .jed differs from direct assembly"
    );

    remove_dir_all(dir)?;
    remove_dir_all(&direct_dir)?;
    Ok(())
}
