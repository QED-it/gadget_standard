extern crate cc;
extern crate cmake;

use std::fs::rename;
use std::path::Path;
use std::process::Command;


fn main() {
    match Command::new("flatc").args(&[
        "--rust",
        "--cpp",
        "-o", "src/",
        "../gadget.fbs",
    ]).output() {
        Ok(flatc) => {
            if !
                flatc.status.success() {
                panic!("\n\nFlatBuffers code generation failed.\n{}\n{}\n",
                       String::from_utf8_lossy(&flatc.stdout),
                       String::from_utf8_lossy(&flatc.stderr));
            }

            // Move C++ file.
            rename(
                Path::new("src").join("gadget_generated.h"),
                Path::new("..").join("cpp").join("gadget_generated.h"),
            ).expect("Failed to rename");

            // Fix an issue in generated code.
            // The lifetime 'a should be on the return value, not on &self.
            // Published at https://github.com/google/flatbuffers/pull/5140
            {
                let file = &Path::new("src").join("gadget_generated.rs");
                let code = std::fs::read_to_string(file).expect("could not read file");

                let re = regex::Regex::new(
                    r"pub fn (\w+)_as_(\w+)\(&'a self\) -> Option<(\w+)> \{"
                ).unwrap();
                let fixed = re.replace_all(
                    &code,
                    r"pub fn ${1}_as_${2}(&self) -> Option<${3}<'a>> {",
                ).to_string();

                let re2 = regex::Regex::new(
                    r"\(&self\) -> Option<flatbuffers::Vector<flatbuffers::ForwardsUOffset<"
                ).unwrap();
                let fixed2 = re2.replace_all(
                    &fixed,
                    r"(&self) -> Option<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<",
                ).to_string();

                std::fs::write(file, fixed2).expect("could not write file");
            }
        }
        Err(_) => {
            println!("cargo:warning=Install FlatBuffers (flatc) if you modify `gadget.fbs`. Code was not regenerated.");
        }
    }

    let dst = cmake::Config::new("../cpp").build();
    println!("cargo:rustc-link-search=native={}", dst.display());
    println!("cargo:rustc-link-lib=zkcomponent");
    println!("cargo:rustc-link-lib=stdc++");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!("cargo:include={}", out_dir);

    // To use the C++ part, include the environment variable DEP_ZKSTANDARD_INCLUDE
    // In Rust CC, add:
    //   .include(std::env::var("DEP_ZKSTANDARD_INCLUDE").unwrap())
}