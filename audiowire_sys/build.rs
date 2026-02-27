use std::{env, path::PathBuf};

fn main() {
    if env::var("PROFILE")
        .map(|s| s != "release")
        .unwrap_or_default()
    {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        println!(
            "cargo:rustc-link-search={}/../libaudiowire/builddir",
            manifest_dir
        );
    }
    println!("cargo:rustc-link-lib=audiowire2");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
