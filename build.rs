use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use vergen_git2::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    /*
    === ENV VARIABLES: ===
    VERGEN_BUILD_TIMESTAMP
    VERGEN_CARGO_TARGET_TRIPLE
    VERGEN_GIT_BRANCH
    VERGEN_GIT_COMMIT_TIMESTAMP
    VERGEN_GIT_SHA
    VERGEN_RUSTC_CHANNEL
    VERGEN_RUSTC_COMMIT_DATE
    VERGEN_RUSTC_COMMIT_HASH
    VERGEN_RUSTC_SEMVER
    */

    let build = BuildBuilder::default().build_timestamp(true).build()?;
    let cargo = CargoBuilder::default().target_triple(true).build()?;
    let git2 = Git2Builder::default()
        .branch(true)
        .commit_timestamp(true)
        .sha(false)
        .build()?;
    let rustc = RustcBuilder::default()
        .channel(true)
        .commit_date(true)
        .commit_hash(true)
        .semver(true)
        .build()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&git2)?
        .add_instructions(&rustc)?
        .emit_and_set()?;

    let env_list: Vec<&str> = vec![
        "CARGO_PKG_VERSION",
        "VERGEN_BUILD_TIMESTAMP",
        "VERGEN_CARGO_TARGET_TRIPLE",
        "VERGEN_GIT_SHA",
        "VERGEN_GIT_COMMIT_TIMESTAMP",
        "VERGEN_GIT_BRANCH",
        "VERGEN_RUSTC_CHANNEL",
        "VERGEN_RUSTC_COMMIT_DATE",
        "VERGEN_RUSTC_COMMIT_HASH",
        "VERGEN_RUSTC_SEMVER",
    ];

    let path: PathBuf = Path::new(&env::var("OUT_DIR").unwrap()).join("codegen.rs");
    let mut file: BufWriter<File> = BufWriter::new(File::create(&path).unwrap());
    let mut codegen: phf_codegen::Map<&str> = phf_codegen::Map::new();

    for environ in env_list {
        match env::var(environ) {
            Ok(value) => codegen.entry(environ, format!("\"{}\"", value).as_str()),
            Err(_) => continue,
        };
    }

    write!(
        &mut file,
        "pub static ENV_VARS: phf::Map<&'static str, &'static str> = {};\n",
        codegen.build()
    )
    .unwrap();
    Ok(())
}
