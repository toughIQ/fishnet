use std::env;
use std::process::Command;
use glob::glob;

struct Target {
    arch: &'static str,
    pgo: bool,
}

impl Target {
    fn build(&self, src_dir: &'static str, name: &'static str) {
        let pgo = self.pgo || env::var("SDE_PATH").is_ok();
        let exe = format!("{}-{}{}", name, self.arch, if cfg!(windows) { ".exe" } else { "" });

        Command::new(if cfg!(target_os = "freebsd") { "gmake" } else { "make" })
            .current_dir(src_dir)
            .env("CXXFLAGS", format!("-DNNUE_EMBEDDING_OFF{}", if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                " -target arm64-apple-macos11"
            } else {
                ""
            }))
            .args(&[
                  "-B",
                  &format!("COMP={}", if cfg!(windows) { "mingw" } else if cfg!(any(target_os = "macos", target_os = "freebsd")) { "clang" } else { "gcc" }),
                  &format!("ARCH={}", self.arch),
                  &format!("EXE={}", exe),
                  if pgo { "profile-build" } else { "build" },
            ])
            .status()
            .unwrap();

        Command::new("strip")
            .current_dir(src_dir)
            .args(&[&exe])
            .status()
            .unwrap();

        compress(src_dir, &exe);
    }

    fn build_official(&self) {
        self.build("Stockfish/src", "stockfish");
    }

    fn build_mv(&self) {
        self.build("Variant-Stockfish/src", "stockfish-mv");
    }

    fn build_both(&self) {
        self.build_official();
        self.build_mv();
    }
}

#[cfg(target_arch = "x86_64")]
fn stockfish_build() {
    Target {
        arch: "x86-64",
        pgo: true,
    }.build_both();

    Target {
        arch: "x86-64-sse3-popcnt",
        pgo: is_x86_feature_detected!("sse3") && is_x86_feature_detected!("popcnt"),
    }.build_both();

    Target {
        arch: "x86-64-ssse3",
        pgo: is_x86_feature_detected!("ssse3"),
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

    // TODO: Switch to Fairy-Stockfish.
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn stockfish_build() {
    Target {
        arch: "aarch64",
        pgo: true,
    }.build_both();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn stockfish_build() {
    Target {
        arch: "apple-silicon",
        pgo: true,
    }.build_both();
}

fn compress(dir: &str, file: &str) {
    Command::new("xz")
        .current_dir(dir)
        .args(&["--keep", "--force", file])
        .status()
        .unwrap();
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
