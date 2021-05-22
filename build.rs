use std::env;
use std::process::Command;
use std::fs;
use std::fs::File;
use std::io;
use std::path::Path;
use glob::glob;

#[cfg(target_arch = "x86_64")]
fn not_cross_compiled() -> bool {
    env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "x86_64"
}

#[cfg(target_arch = "aarch64")]
fn not_cross_compiled() -> bool {
    env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64"
}

struct Target {
    arch: &'static str,
    pgo: bool,
}

impl Target {
    fn build(&self, src_dir: &'static str, name: &'static str) {
        let release = env::var("PROFILE").unwrap() == "release";
        let pgo = release && not_cross_compiled() && (self.pgo || env::var("SDE_PATH").is_ok());
        let exe = format!("{}-{}{}", name, self.arch, if env::var("CARGO_CFG_TARGET_FAMILY").unwrap() == "windows" { ".exe" } else { "" });
        if release && !pgo {
            println!("cargo:warning=Building {} without profile-guided optimization", exe);
        }

        let arg_comp = format!("COMP={}", if env::var("CARGO_CFG_TARGET_FAMILY").unwrap() == "windows" {
            "mingw"
        } else if env::var("CARGO_CFG_TARGET_OS").unwrap() == "linux" {
            "gcc"
        } else {
            "clang"
        });
        let arg_arch = format!("ARCH={}", self.arch);
        let arg_exe = format!("EXE={}", exe);
        let arg_cxx = env::var("CXX").ok().map(|cxx| format!("CXX={}", cxx));

        let mut args = vec!["-B", &arg_comp, &arg_arch, &arg_exe];
        if let Some(ref arg_cxx) = arg_cxx {
            args.push(arg_cxx);
        }
        args.push(if pgo { "profile-build" } else { "build" });

        let make = if env::var("CARGO_CFG_TARGET_OS").unwrap() == "freebsd" { "gmake" } else { "make" };

        assert!(Command::new(make)
            .current_dir(src_dir)
            .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
            .env("CXXFLAGS", format!("{} -DNNUE_EMBEDDING_OFF", env::var("CXXFLAGS").unwrap_or_default()))
            .args(&args)
            .status()
            .unwrap()
            .success());

        assert!(Command::new(make)
            .current_dir(src_dir)
            .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
            .args(&["clean"])
            .status()
            .unwrap()
            .success());

        assert!(Command::new("strip")
            .current_dir(src_dir)
            .args(&[&exe])
            .status()
            .unwrap()
            .success());

        compress(src_dir, &exe);
    }

    fn build_official(&self) {
        self.build("Stockfish/src", "stockfish");
    }

    fn build_mv(&self) {
        // TODO: Switch to Fairy-Stockfish.
        self.build("Variant-Stockfish/src", "stockfish-mv");
    }

    fn build_both(&self) {
        self.build_official();
        self.build_mv();
    }
}

fn stockfish_build() {
    if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "x86_64" {
        Target {
            arch: "x86-64",
            pgo: true,
        }.build_both();

        Target {
            arch: "x86-64-sse41-popcnt",
            pgo: is_x86_feature_detected!("sse4.1") && is_x86_feature_detected!("popcnt"),
        }.build_both();

        Target {
            arch: "x86-64-avx2",
            pgo: is_x86_feature_detected!("avx2"),
        }.build_both();

        Target {
            arch: "x86-64-bmi2",
            pgo: is_x86_feature_detected!("bmi2"),
        }.build_both();

        // TODO: Could support:
        // - x86-64-avx512
        // - x86-64-vnni256
        // - x86-64-vnni512
    } else if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64" {
        if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
            Target {
                arch: "apple-silicon",
                pgo: true,
            }.build_both();
        } else {
            Target {
                arch: "aarch64",
                pgo: true,
            }.build_both();
        }
    } else {
        unimplemented!("Stockfish build for {} not supported", env::var("CARGO_CFG_TARGET_ARCH").unwrap());
    }
}

fn compress(dir: &str, file: &str) {
    let compressed = File::create(Path::new(&env::var("OUT_DIR").unwrap()).join(&format!("{}.xz", file))).unwrap();
    let mut encoder = xz2::write::XzEncoder::new(compressed, 9);

    let uncompressed_path = Path::new(dir).join(file);
    let mut uncompressed = File::open(&uncompressed_path).unwrap();
    io::copy(&mut uncompressed, &mut encoder).unwrap();
    encoder.finish().unwrap();

    fs::remove_file(uncompressed_path).unwrap();
}

fn hooks() {
    println!("cargo:rerun-if-changed=Cargo.lock");

    println!("cargo:rerun-if-env-changed=CXX");
    println!("cargo:rerun-if-env-changed=SDE_PATH");

    println!("cargo:rerun-if-changed=Stockfish/src/Makefile");
    for entry in glob("Stockfish/src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
    for entry in glob("Stockfish/src/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }

    println!("cargo:rerun-if-changed=Variant-Stockfish/src/Makefile");
    for entry in glob("Variant-Stockfish/src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
    for entry in glob("Variant-Stockfish/src/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
}

fn main() {
    hooks();
    stockfish_build();
    compress("Stockfish/src", "nn-7756374aaed3.nnue");
    auditable_build::collect_dependency_list();
}
