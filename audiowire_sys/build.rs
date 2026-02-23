use std::{
    env::{self, var},
    path::PathBuf,
};

fn main() {
    let manifest_dir = var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-lib=audiowire2");
    println!(
        "cargo:rustc-link-search={}/../libaudiowire/builddir",
        manifest_dir
    );

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
