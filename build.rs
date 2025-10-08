#![forbid(unsafe_code)]

use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
    time::SystemTime,
};

use glob::glob;
use zstd::stream::write::Encoder as ZstdEncoder;

static OUT_PATH: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from(&env::var("OUT_DIR").unwrap()));
static STOCKFISH_OUT_PATH: LazyLock<PathBuf> = LazyLock::new(|| OUT_PATH.join("Stockfish"));
static FAIRY_STOCKFISH_OUT_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| OUT_PATH.join("Fairy-Stockfish"));

const EVAL_FILE_NAME: &str = "nn-1c0000000000.nnue";
static EVAL_FILE_OUT_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| STOCKFISH_OUT_PATH.join("src").join(EVAL_FILE_NAME));

const EVAL_FILE_SMALL_NAME: &str = "nn-37f18f62d772.nnue";
static EVAL_FILE_SMALL_OUT_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| STOCKFISH_OUT_PATH.join("src").join(EVAL_FILE_SMALL_NAME));

fn main() {
    println!(
        "cargo:rustc-env=FISHNET_TARGET={}",
        env::var("TARGET").unwrap()
    );

    hooks();

    // If there are eval files from a previous build, back them up into a temp
    // file before overwriting the submodule copies. This avoids re-downloading.
    let eval_file_backup = backup_file_if_exists(&EVAL_FILE_OUT_PATH);
    let eval_file_small_backup = backup_file_if_exists(&EVAL_FILE_SMALL_OUT_PATH);

    // Copy submodule folders to OUT_DIR because their Makefiles create files,
    // which would make the build non-idempotent.
    copy_overwrite_submodule_folder(Path::new("Stockfish"), &STOCKFISH_OUT_PATH);
    copy_overwrite_submodule_folder(Path::new("Fairy-Stockfish"), &FAIRY_STOCKFISH_OUT_PATH);

    // Restore backed-up eval files (if we had them)
    if let Some(eval_file_backup) = eval_file_backup {
        move_file(&eval_file_backup, &EVAL_FILE_OUT_PATH);
    };
    if let Some(eval_file_small_backup) = eval_file_small_backup {
        move_file(&eval_file_small_backup, &EVAL_FILE_SMALL_OUT_PATH);
    };

    // Build Stockfish and Fairy-Stockfish and archive them (along with eval files)
    let mut archive = ar::Builder::new(
        ZstdEncoder::new(File::create(OUT_PATH.join("assets.ar.zst")).unwrap(), 6).unwrap(),
    );
    stockfish_build(&mut archive);
    append_file(&mut archive, &*EVAL_FILE_OUT_PATH, 0o644);
    append_file(&mut archive, &*EVAL_FILE_SMALL_OUT_PATH, 0o644);
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
                arch: "x86-64-avx512icl",
                native: has_x86_64_builder_feature!("avx512f")
                    && has_x86_64_builder_feature!("avx512cd")
                    && has_x86_64_builder_feature!("avx512vl")
                    && has_x86_64_builder_feature!("avx512dq")
                    && has_x86_64_builder_feature!("avx512bw")
                    && has_x86_64_builder_feature!("avx512ifma")
                    && has_x86_64_builder_feature!("avx512vbmi")
                    && has_x86_64_builder_feature!("avx512vbmi2")
                    && has_x86_64_builder_feature!("avx512vpopcntdq")
                    && has_x86_64_builder_feature!("avx512bitalg")
                    && has_x86_64_builder_feature!("avx512vnni")
                    && has_x86_64_builder_feature!("vpclmulqdq")
                    && has_x86_64_builder_feature!("gfni")
                    && has_x86_64_builder_feature!("vaes"),
                sde: sde.clone(),
            }
            .build_official(archive);

            let vnni512 = Target {
                arch: "x86-64-vnni512",
                native: has_x86_64_builder_feature!("avx512vnni")
                    && has_x86_64_builder_feature!("avx512dq")
                    && has_x86_64_builder_feature!("avx512f")
                    && has_x86_64_builder_feature!("avx512bw")
                    && has_x86_64_builder_feature!("avx512vl"),
                sde: sde.clone(),
            };
            vnni512.build_multi_variant(archive);
            if has_target_feature("avx512f")
                && has_target_feature("avx512cd")
                && has_target_feature("avx512vl")
                && has_target_feature("avx512dq")
                && has_target_feature("avx512bw")
                && has_target_feature("avx512ifma")
                && has_target_feature("avx512vbmi")
                && has_target_feature("avx512vbmi2")
                && has_target_feature("avx512vpopcntdq")
                && has_target_feature("avx512bitalg")
                && has_target_feature("avx512vnni")
                && has_target_feature("vpclmulqdq")
                && has_target_feature("gfni")
                && has_target_feature("vaes")
            {
                return;
            }
            vnni512.build_official(archive);

            if has_target_feature("avx512vnni")
                && has_target_feature("avx512dq")
                && has_target_feature("avx512f")
                && has_target_feature("avx512bw")
                && has_target_feature("avx512vl")
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
        src_path: &Path,
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
            Command::new(&make)
                .current_dir(src_path)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .arg("clean")
                .status()
                .unwrap()
                .success(),
            "$(MAKE) clean"
        );

        if flavor == Flavor::Official
            && !Command::new(&make)
                .current_dir(src_path)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .arg("-B")
                .arg("net")
                .status()
                .unwrap()
                .success()
        {
            let _ = fs::remove_file(src_path.join(EVAL_FILE_NAME));
            let _ = fs::remove_file(src_path.join(EVAL_FILE_SMALL_NAME));
            println!(
                "cargo:warning=Deleted corrupted network file {EVAL_FILE_NAME} or {EVAL_FILE_SMALL_NAME}"
            );
        }

        assert!(
            Command::new(&make)
                .current_dir(src_path)
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
                .current_dir(src_path)
                .env("MAKEFLAGS", env::var("CARGO_MAKEFLAGS").unwrap())
                .arg(format!("EXE={exe}"))
                .arg("strip")
                .status()
                .unwrap()
                .success(),
            "$(MAKE) strip"
        );

        let exe_path = Path::new(src_path).join(exe);
        append_file(archive, &exe_path, 0o755);
        fs::remove_file(&exe_path).unwrap();
    }

    fn build_official<W: Write>(&self, archive: &mut ar::Builder<W>) {
        self.build(
            Flavor::Official,
            &STOCKFISH_OUT_PATH.join("src"),
            "stockfish",
            archive,
        );
    }

    fn build_multi_variant<W: Write>(&self, archive: &mut ar::Builder<W>) {
        self.build(
            Flavor::MultiVariant,
            &FAIRY_STOCKFISH_OUT_PATH.join("src"),
            "fairy-stockfish",
            archive,
        );
    }

    fn build_both<W: Write>(&self, archive: &mut ar::Builder<W>) {
        self.build_official(archive);
        self.build_multi_variant(archive);
    }
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

fn copy_overwrite_submodule_folder(src: &Path, dst: &Path) {
    assert!(
        src.is_dir(),
        "Directory {src:?} does not exist. Try: git submodule update --init",
    );
    if dst.exists() {
        fs::remove_dir_all(dst).unwrap();
    }
    fs::create_dir_all(dst).unwrap();
    fs_extra::dir::copy(
        src,
        dst,
        &fs_extra::dir::CopyOptions::new().content_only(true),
    )
    .unwrap();
}

/// If `src` exists, copy it to a temporary file.
///
/// Returns the path to the temporary file, or `None` if `src` did not exist.
fn backup_file_if_exists(src: &Path) -> Option<PathBuf> {
    if !src.exists() {
        return None;
    }

    // Scope backup files by timestamp to avoid collisions with parallel builds
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let filename = src.file_name().unwrap().to_str().unwrap();
    let tmp_file_path = env::temp_dir().join(format!("{timestamp}-{filename}"));

    fs::copy(src, &tmp_file_path).unwrap();

    Some(tmp_file_path)
}

/// Move a file from `src` to `dst` by copying and deleting the original.
///
/// Needed because `fs::rename` doesn't work across different filesystems.
fn move_file(src: &Path, dst: &Path) {
    fs::copy(src, dst).unwrap();
    let _ = fs::remove_file(src);
}
