use std::{
    fmt,
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use bitflags::bitflags;
use serde::Serialize;
use tempfile::TempDir;
use xz2::read::XzDecoder;

struct Asset {
    name: &'static str,
    data: &'static [u8],
    needs: Cpu,
    executable: bool,
}

impl Asset {
    #[cfg(unix)]
    fn open_executable_file(path: &Path) -> io::Result<File> {
        use std::os::unix::fs::OpenOptionsExt as _;
        OpenOptions::new()
            .create(true)
            .write(true)
            .mode(0o700)
            .open(path)
    }

    #[cfg(not(unix))]
    fn open_executable_file(path: &Path) -> io::Result<File> {
        Asset::open_file(path)
    }

    fn open_file(path: &Path) -> io::Result<File> {
        OpenOptions::new().create(true).write(true).open(path)
    }

    fn create(&self, base: &Path) -> io::Result<PathBuf> {
        let path = base.join(self.name);
        let mut file = if self.executable {
            Asset::open_executable_file(&path)
        } else {
            Asset::open_file(&path)
        }?;

        let mut decoder = XzDecoder::new(self.data);
        io::copy(&mut decoder, &mut file)?;

        file.sync_all()?;
        Ok(path)
    }
}

impl fmt::Debug for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asset")
            .field("name", &self.name)
            .field("needs", &self.needs)
            .field("executable", &self.executable)
            .field("data", &"..")
            .finish()
    }
}

bitflags! {
    pub struct Cpu: u32 {
        const SSE2      = 1 << 0;
        const POPCNT    = 1 << 1;
        const SSE41     = 1 << 2;
        const AVX2      = 1 << 3;
        const FAST_BMI2 = 1 << 4;
        const AVX512    = 1 << 5;
        const VNNI512   = 1 << 6;

        const SF_SSE2         = Cpu::SSE2.bits;
        const SF_SSE41_POPCNT = Cpu::SSE41.bits | Cpu::POPCNT.bits;
        const SF_AVX2         = Cpu::SF_SSE41_POPCNT.bits | Cpu::AVX2.bits;
        const SF_BMI2         = Cpu::SF_AVX2.bits | Cpu::FAST_BMI2.bits;
        const SF_AVX512       = Cpu::SF_BMI2.bits | Cpu::AVX512.bits;
        const SF_VNNI512      = Cpu::SF_AVX512.bits | Cpu::VNNI512.bits;
    }
}

impl Cpu {
    #[cfg(target_arch = "x86_64")]
    pub fn detect() -> Cpu {
        let mut cpu = Cpu::empty();
        cpu.set(Cpu::SSE2, is_x86_feature_detected!("sse2"));
        cpu.set(Cpu::POPCNT, is_x86_feature_detected!("popcnt"));
        cpu.set(Cpu::SSE41, is_x86_feature_detected!("sse4.1"));
        cpu.set(Cpu::AVX2, is_x86_feature_detected!("avx2"));
        cpu.set(
            Cpu::FAST_BMI2,
            is_x86_feature_detected!("bmi2") && {
                let cpuid = raw_cpuid::CpuId::new();
                match cpuid.get_vendor_info() {
                    // Intel was implementing BMI2 in hardware from the beginning.
                    Some(vendor) if vendor.as_str() == "GenuineIntel" => true,
                    // Due to patents, AMD was using slow software emulation
                    // for PEXT for a long time. The Zen 3 family (0x19) is the
                    // first to implement it in hardware.
                    Some(vendor) if vendor.as_str() == "AuthenticAMD" => {
                        cpuid.get_feature_info().map_or(false, |f| {
                            let family = if f.family_id() == 15 {
                                f.extended_family_id() + f.family_id()
                            } else {
                                f.family_id()
                            };
                            family >= 0x19
                        })
                    }
                    _ => false,
                }
            },
        );
        cpu.set(Cpu::AVX512,
            is_x86_feature_detected!("avx512f") &&
            is_x86_feature_detected!("avx512bw")
        );
        cpu.set(Cpu::VNNI512,
            is_x86_feature_detected!("avx512dq") &&
            is_x86_feature_detected!("avx512vl") &&
            is_x86_feature_detected!("avx512vnni")
        );
        cpu
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub fn detect() -> Cpu {
        Cpu::empty()
    }
}

const NNUE: Asset = Asset {
    name: env!("EVAL_FILE"),
    data: include_bytes!(concat!(env!("OUT_DIR"), "/", env!("EVAL_FILE"), ".xz")),
    needs: Cpu::empty(),
    executable: false,
};

#[cfg(all(unix, target_arch = "x86_64"))]
const STOCKFISH: &[Asset] = &[
    Asset {
        name: "stockfish-x86-64-vnni512",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-vnni512.xz")),
        needs: Cpu::SF_VNNI512,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-avx512",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-avx512.xz")),
        needs: Cpu::SF_AVX512,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-bmi2",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-bmi2.xz")),
        needs: Cpu::SF_BMI2,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-avx2",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-avx2.xz")),
        needs: Cpu::SF_AVX2,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-sse41-popcnt",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/stockfish-x86-64-sse41-popcnt.xz"
        )),
        needs: Cpu::SF_SSE41_POPCNT,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64.xz")),
        needs: Cpu::SF_SSE2,
        executable: true,
    },
];

#[cfg(all(unix, target_arch = "x86_64"))]
const STOCKFISH_MV: &[Asset] = &[
    Asset {
        name: "fairy-stockfish-x86-64-vnni512",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-x86-64-vnni512.xz")),
        needs: Cpu::SF_VNNI512,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-avx512",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-x86-64-avx512.xz")),
        needs: Cpu::SF_AVX512,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-bmi2",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-x86-64-bmi2.xz")),
        needs: Cpu::SF_BMI2,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-avx2",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-x86-64-avx2.xz")),
        needs: Cpu::SF_AVX2,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-sse41-popcnt",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fairy-stockfish-x86-64-sse41-popcnt.xz"
        )),
        needs: Cpu::SF_SSE41_POPCNT,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-x86-64.xz")),
        needs: Cpu::SF_SSE2,
        executable: true,
    },
];

#[cfg(all(windows, target_arch = "x86_64"))]
const STOCKFISH: &[Asset] = &[
    Asset {
        name: "stockfish-x86-64-vnni512.exe",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-vnni512.exe.xz")),
        needs: Cpu::SF_VNNI512,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-avx512.exe",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-avx512.exe.xz")),
        needs: Cpu::SF_AVX512,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-bmi2.exe",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-bmi2.exe.xz")),
        needs: Cpu::SF_BMI2,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-avx2.exe",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64-avx2.exe.xz")),
        needs: Cpu::SF_AVX2,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64-sse41-popcnt.exe",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/stockfish-x86-64-sse41-popcnt.exe.xz"
        )),
        needs: Cpu::SF_SSE41_POPCNT,
        executable: true,
    },
    Asset {
        name: "stockfish-x86-64.exe",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-x86-64.exe.xz")),
        needs: Cpu::SF_SSE2,
        executable: true,
    },
];

#[cfg(all(windows, target_arch = "x86_64"))]
const STOCKFISH_MV: &[Asset] = &[
    Asset {
        name: "fairy-stockfish-x86-64-vnni512.exe",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fairy-stockfish-x86-64-vnni512.exe.xz"
        )),
        needs: Cpu::SF_VNNI512,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-avx512.exe",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fairy-stockfish-x86-64-avx512.exe.xz"
        )),
        needs: Cpu::SF_AVX512,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-bmi2.exe",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fairy-stockfish-x86-64-bmi2.exe.xz"
        )),
        needs: Cpu::SF_BMI2,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-avx2.exe",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fairy-stockfish-x86-64-avx2.exe.xz"
        )),
        needs: Cpu::SF_AVX2,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64-sse41-popcnt.exe",
        data: include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fairy-stockfish-x86-64-sse41-popcnt.exe.xz"
        )),
        needs: Cpu::SF_SSE41_POPCNT,
        executable: true,
    },
    Asset {
        name: "fairy-stockfish-x86-64.exe",
        data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-x86-64.exe.xz")),
        needs: Cpu::SF_SSE2,
        executable: true,
    },
];

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const STOCKFISH: &[Asset] = &[Asset {
    name: "stockfish-armv8",
    data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-armv8.xz")),
    needs: Cpu::empty(),
    executable: true,
}];

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const STOCKFISH_MV: &[Asset] = &[Asset {
    name: "fairy-stockfish-armv8",
    data: include_bytes!(concat!(env!("OUT_DIR"), "/fairy-stockfish-armv8.xz")),
    needs: Cpu::empty(),
    executable: true,
}];

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const STOCKFISH: &[Asset] = &[Asset {
    name: "stockfish-apple-silicon",
    data: include_bytes!(concat!(env!("OUT_DIR"), "/stockfish-apple-silicon.xz")),
    needs: Cpu::empty(),
    executable: true,
}];

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const STOCKFISH_MV: &[Asset] = &[Asset {
    name: "fairy-stockfish-apple-silicon",
    data: include_bytes!(concat!(
        env!("OUT_DIR"),
        "/fairy-stockfish-apple-silicon.xz"
    )),
    needs: Cpu::empty(),
    executable: true,
}];

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EngineFlavor {
    Official,
    MultiVariant,
}

impl EngineFlavor {
    pub fn eval_flavor(self) -> EvalFlavor {
        match self {
            EngineFlavor::Official => EvalFlavor::Nnue,
            EngineFlavor::MultiVariant => EvalFlavor::Hce,
        }
    }
}

#[derive(Debug)]
pub struct ByEngineFlavor<T> {
    pub official: T,
    pub multi_variant: T,
}

impl<T> ByEngineFlavor<T> {
    pub fn get(&self, flavor: EngineFlavor) -> &T {
        match flavor {
            EngineFlavor::Official => &self.official,
            EngineFlavor::MultiVariant => &self.multi_variant,
        }
    }

    pub fn get_mut(&mut self, flavor: EngineFlavor) -> &mut T {
        match flavor {
            EngineFlavor::Official => &mut self.official,
            EngineFlavor::MultiVariant => &mut self.multi_variant,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
pub enum EvalFlavor {
    #[serde(rename = "classical")]
    Hce,
    #[serde(rename = "nnue")]
    Nnue,
}

impl EvalFlavor {
    pub fn is_nnue(self) -> bool {
        matches!(self, EvalFlavor::Nnue)
    }

    pub fn is_hce(self) -> bool {
        matches!(self, EvalFlavor::Hce)
    }
}

#[derive(Debug)]
pub struct Assets {
    pub sf_name: &'static str,
    pub nnue: String,
    pub stockfish: ByEngineFlavor<PathBuf>,
    _dir: TempDir, // Will be deleted when dropped
}

impl Assets {
    pub fn prepare(cpu: Cpu) -> io::Result<Assets> {
        let dir = tempfile::Builder::new().prefix("fishnet-").tempdir()?;
        let sf = STOCKFISH
            .iter()
            .find(|a| cpu.contains(a.needs))
            .expect("compatible stockfish");
        Ok(Assets {
            nnue: NNUE
                .create(dir.path())?
                .to_str()
                .expect("nnue path printable")
                .to_owned(),
            sf_name: sf.name,
            stockfish: ByEngineFlavor {
                official: sf.create(dir.path())?,
                multi_variant: STOCKFISH_MV
                    .iter()
                    .find(|a| cpu.contains(a.needs))
                    .expect("compatible stockfish")
                    .create(dir.path())?,
            },
            _dir: dir,
        })
    }
}
