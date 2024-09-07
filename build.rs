#![forbid(unsafe_code)]

use std::{env, fs, fs::File, io::Write, path::Path, process::Command};

use glob::glob;
use zstd::stream::write::Encoder as ZstdEncoder;

const EVAL_FILE: &str = "nn-1111cefa1111.nnue";
const EVAL_FILE_SMALL: &str = "nn-37f18f62d772.nnue";

fn main() {
    println!(
        "cargo:rustc-env=FISHNET_TARGET={}",
        env::var("TARGET").unwrap()
    );

    hooks();

    let mut archive = ar::Builder::new(
        ZstdEncoder::new(
            File::create(Path::new(&env::var("OUT_DIR").unwrap()).join("assets.ar.xz")).unwrap(),
            6,
        )
        .unwrap(),
    );
    stockfish_build(&mut archive);
    stockfish_eval_file(EVAL_FILE, &mut archive);
    stockfish_eval_file(EVAL_FILE_SMALL, &mut archive);
    archive.into_inner().unwrap().finish().unwrap();

    // Resource compilation may fail when toolchain does not match target,
    // e.g. windows-msvc toolchain with windows-gnu target.
    #[cfg(target_family = "windows")]
    winres::WindowsResource::new()
        .set_icon("favicon.ico")
        .compile()
        .unwrap_or_else(|err| {
            println!("cargo:warning=Resource compiler not invoked: {}", err);
        });
}

fn hooks() {
    println!("cargo:rerun-if-env-changed=CXX");
    println!("cargo:rerun-if-env-changed=CXXFLAGS");
    println!("cargo:rerun-if-env-changed=DEPENDFLAGS");
    println!("cargo:rerun-if-env-changed=LDFLAGS");
    println!("cargo:rerun-if-env-changed=MAKE");
    println!("cargo:rerun-if-env-changed=SDE_PATH");

    println!("cargo:rerun-if-changed=Stockfish/src/Makefile");
    for entry in glob("Stockfish/src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
    for entry in glob("Stockfish/src/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }

    println!("cargo:rerun-if-changed=Fairy-Stockfish/src/Makefile");
    for entry in glob("Fairy-Stockfish/src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
    for entry in glob("Fairy-Stockfish/src/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }

    println!("cargo:rerun-if-changed=favicon.ico");
}

fn has_target_feature(feature: &str) -> bool {
    env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap()
        .split(',')
        .any(|f| f == feature)
}

macro_rules! has_x86_64_builder_feature {
    ($feature:tt) => {{
        #[cfg(target_arch = "x86_64")]
        {
            std::arch::is_x86_feature_detected!($feature)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }};
}

macro_rules! has_aarch64_builder_feature {
    ($feature:tt) => {{
        #[cfg(target_arch = "aarch64")]
        {
            std::arch::is_aarch64_feature_detected!($feature)
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            false
        }
    }};
}

#[allow(clippy::nonminimal_bool, clippy::eq_op)]
fn stockfish_build<W: Write>(archive: &mut ar::Builder<W>) {
    // Note: The target arch of the build script is the architecture of the
    // builder and decides if pgo is possible. It is not necessarily the same
    // as CARGO_CFG_TARGET_ARCH, the target arch of the fishnet binary.
    //
    // Can skip building more broadly compatible Stockfish binaries and return
    // early when building with something like -C target-cpu=native.

    match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => {
            let sde = env::var("SDE_PATH")
                .ok()
                .filter(|_| cfg!(target_arch = "x86_64"));

            Target {
                arch: "x86-64-vnni256",
                native: has_x86_64_builder_feature!("avx512dq")
                    && has_x86_64_builder_feature!("avx512vl")
                    && has_x86_64_builder_feature!("avx512vnni"),
                sde: sde.clone(),
            }
            .build_both(archive);

            if has_target_feature("avx512dq")
                && has_target_feature("avx512vl")
                && has_target_feature("avx512vnni")
            {
                return;
            }

            Target {
                arch: "x86-64-avx512",
                native: has_x86_64_builder_feature!("avx512f")
                    && has_x86_64_builder_feature!("avx512bw"),
                sde: sde.clone(),
            }
            .build_both(archive);

            if has_target_feature("avx512f") && has_target_feature("avx512bw") {
                return;
            }

            Target {
                arch: "x86-64-bmi2",
                native: has_x86_64_builder_feature!("bmi2"),
                sde: sde.clone(),
            }
            .build_both(archive);

            if has_target_feature("bmi2") {
                // Fast bmi2 can not be detected at compile time.
            }

            Target {
                arch: "x86-64-avx2",
                native: has_x86_64_builder_feature!("avx2"),
                sde: sde.clone(),
            }
            .build_both(archive);

            if has_target_feature("avx2") {
                return;
            }

            Target {
                arch: "x86-64-sse41-popcnt",
                native: has_x86_64_builder_feature!("sse4.1")
                    && has_x86_64_builder_feature!("popcnt"),
                sde: sde.clone(),
            }
            .build_both(archive);

            if has_target_feature("sse4.1") && has_target_feature("popcnt") {
                return;
            }

            Target {
                arch: "x86-64",
                native: cfg!(target_arch = "x86_64"),
                sde,
            }
            .build_both(archive);
        }
        "aarch64" => {
            let native = cfg!(target_arch = "aarch64");

            if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
                Target {
                    arch: "apple-silicon",
                    native,
                    sde: None,
                }
                .build_both(archive);
            } else {
                Target {
                    arch: "armv8-dotprod",
                    native: native && has_aarch64_builder_feature!("dotprod"),
                    sde: None,
                }
                .build_official(archive);

                Target {
                    arch: "armv8",
                    native,
                    sde: None,
                }
                .build_multi_variant(archive);

                if has_target_feature("dotprod") {
                    return;
                }

                Target {
                    arch: "armv8",
                    native,
                    sde: None,
                }
                .build_official(archive);
            }
        }
        target_arch => {
            unimplemented!("Stockfish build for {} not supported", target_arch);
        }
    }
}

struct Target {
    arch: &'static str,
    native: bool,
    sde: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Flavor {
    Official,
    MultiVariant,
}

impl Target {
    fn build<W: Write>(
        &self,
        flavor: Flavor,
        src_dir: &'static str,
        name: &'static str,
        archive: &mut ar::Builder<W>,
    ) {
        let release = env::var("PROFILE").unwrap() == "release";
        let windows = env::var("CARGO_CFG_TARGET_FAMILY").unwrap() == "windows";
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
        let sde = self.sde.as_ref().filter(|_| !self.native);
        let pgo = release && (self.native || sde.is_some());

        let exe = format!(
            "{}-{}{}",
            name,
            self.arch,
            if windows { ".exe" } else { "" }
        );
        if release && !pgo {
            println!("cargo:warning=Building {exe} without profile-guided optimization");
        }

        let (comp, default_cxx, default_make) = if windows {
            ("mingw", "g++", "mingw32-make")
        } else if target_os == "linux" {
            ("gcc", "g++", "make")
        } else if target_os == "freebsd" {
            ("clang", "clang++", "gmake")
        } else {
            ("clang", "clang++", "make")
        };

        let make = env::var("MAKE").unwrap_or_else(|_| default_make.to_owned());

        assert!(
            Command::new(&make)
                .arg("--version")
                .status()
                .unwrap_or_else(|err| panic!(
                    "{err}. Is `{make}` installed?\n\
                    * Debian: sudo apt install build-essential\n\
                    * Arch: sudo pacman -S base-devel\n\
                    * MSYS2: pacman -S mingw32-make\n"
                ))
                .success(),
            "$(MAKE) --version"
        );

        let cxx = env::var("CXX").unwrap_or_else(|_| default_cxx.to_owned());

        assert!(
            Command::new(&cxx)
                .arg("--version")
                .status()
                .unwrap_or_else(|err| panic!("{err}. Is `{cxx}` installed?"))
                .success(),
            "$(CXX) --version"
        );

        assert!(
            Path::new(src_dir).is_dir(),
            "Directory {src_dir:?} does not exist. Try: git submodule update --init",
        );

        assert!(
            Command::new(&make)
                .current_dir(src_dir)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .arg("clean")
                .status()
                .unwrap()
                .success(),
            "$(MAKE) clean"
        );

        if flavor == Flavor::Official
            && !Command::new(&make)
                .current_dir(src_dir)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .arg("-B")
                .arg("net")
                .status()
                .unwrap()
                .success()
        {
            let _ = fs::remove_file(Path::new(src_dir).join(EVAL_FILE));
            let _ = fs::remove_file(Path::new(src_dir).join(EVAL_FILE_SMALL));
            println!(
                "cargo:warning=Deleted corrupted network file {EVAL_FILE} or {EVAL_FILE_SMALL}"
            );
        }

        assert!(
            Command::new(&make)
                .current_dir(src_dir)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .env(
                    "CXXFLAGS",
                    format!(
                        "{} -DNNUE_EMBEDDING_OFF",
                        env::var("CXXFLAGS").unwrap_or_default()
                    ),
                )
                .env_remove("SDE_PATH")
                .env_remove("WINE_PATH")
                .args(sde.map(|e| format!("WINE_PATH={e} --")))
                .args(sde.map(|e| format!("SDE_PATH={e}")))
                .arg("-B")
                .arg(format!("COMP={comp}"))
                .arg(format!("CXX={cxx}"))
                .arg(format!("ARCH={}", self.arch))
                .arg(format!("EXE={exe}"))
                .arg(if pgo { "profile-build" } else { "build" })
                .status()
                .unwrap()
                .success(),
            "$(MAKE) build"
        );

        assert!(
            Command::new(&make)
                .current_dir(src_dir)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .arg(format!("EXE={exe}"))
                .arg("strip")
                .status()
                .unwrap()
                .success(),
            "$(MAKE) strip"
        );

        let exe_path = Path::new(src_dir).join(exe);
        append_file(archive, &exe_path, 0o755);
        fs::remove_file(&exe_path).unwrap();
    }

    fn build_official<W: Write>(&self, archive: &mut ar::Builder<W>) {
        self.build(Flavor::Official, "Stockfish/src", "stockfish", archive);
    }

    fn build_multi_variant<W: Write>(&self, archive: &mut ar::Builder<W>) {
        self.build(
            Flavor::MultiVariant,
            "Fairy-Stockfish/src",
            "fairy-stockfish",
            archive,
        );
    }

    fn build_both<W: Write>(&self, archive: &mut ar::Builder<W>) {
        self.build_official(archive);
        self.build_multi_variant(archive);
    }
}

fn stockfish_eval_file<W: Write>(name: &str, archive: &mut ar::Builder<W>) {
    append_file(
        archive,
        Path::new("Stockfish").join("src").join(name),
        0o644,
    );
}

fn append_file<W: Write, P: AsRef<Path>>(archive: &mut ar::Builder<W>, path: P, mode: u32) {
    let file = File::open(&path).unwrap();
    let metadata = file.metadata().unwrap();
    let mut header = ar::Header::new(
        path.as_ref()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .as_bytes()
            .to_vec(),
        metadata.len(),
    );
    header.set_mode(mode);
    archive.append(&header, file).unwrap();
}
