use std::{env, fs, path::PathBuf};

fn main() {
    let target = env::var("TARGET").expect("TARGET is provided by Cargo");
    if !target.starts_with("thumb") {
        return;
    }

    let memory_file = if target == "thumbv8m.main-none-eabihf" {
        "memory-rp235x.x"
    } else {
        "memory-rp2040.x"
    };

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is provided by Cargo"));
    fs::copy(memory_file, out_dir.join("memory.x")).expect("copy memory.x");
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
    println!("cargo:rerun-if-changed={memory_file}");
}
