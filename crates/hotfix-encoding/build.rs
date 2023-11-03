#![allow(dead_code)]
use hotfix_codegen as codegen;
use hotfix_dictionary::Dictionary;
use std::env::var;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::PathBuf;

fn main() -> io::Result<()> {
    // TODO: add other FIX versions
    #[cfg(feature = "fix42")]
    codegen(Dictionary::fix42(), "fix42.rs")?;
    // FIX 4.4 is always enabled.
    codegen(Dictionary::fix44(), "fix44.rs")?;
    Ok(())
}

fn codegen(fix_dictionary: Dictionary, filename: &str) -> io::Result<()> {
    // All generated code must go in `OUT_DIR`. We avoid writing directly to
    // `src/` to avoid compilation issues on `crates.io`, which disallows
    // writing.
    let dir = PathBuf::from(var("OUT_DIR").unwrap());
    let codegen_settings = &mut codegen::Settings::default();
    codegen_settings.fefix_crate_name = "crate".to_string();
    let code = codegen::gen_definitions(&fix_dictionary, codegen_settings);
    let path = dir.join(filename);
    let file = &mut File::create(path)?;
    file.write_all(code.as_bytes())?;
    Ok(())
}
