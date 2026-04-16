use hotfix_codegen as codegen;
use hotfix_dictionary::Dictionary;
use std::env::var;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let spec_path = "spec/FIX44-custom.xml";
    println!("cargo:rerun-if-changed={spec_path}");

    let dict =
        Dictionary::load_from_file(spec_path).expect("failed to load custom FIX 4.4 dictionary");

    let mut settings = codegen::Settings::default();
    // The generated code uses `<crate>::dict::FieldLocation`, `<crate>::FieldType`,
    // and `<crate>::HardCodedFixFieldDefinition` — re-exported by `hotfix-message`
    // but not by `hotfix`, so we point codegen at `hotfix_message`.
    settings.hotfix_crate_name = "hotfix_message".to_string();

    let code = codegen::gen_definitions(&dict, &settings);

    let out_dir = PathBuf::from(var("OUT_DIR").expect("OUT_DIR not set by cargo"));
    let out_path = out_dir.join("custom_fix.rs");
    let mut file = File::create(&out_path)?;
    file.write_all(code.as_bytes())?;

    Ok(())
}
