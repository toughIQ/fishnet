use std::fmt;
use bitflags::bitflags;
use crate::configure::Opt;
use tracing::info;

struct Asset {
    name: &'static str,
    data: &'static [u8],
    needs: Cpu,
}

impl fmt::Debug for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asset")
            .field("name", &self.name)
            .finish()
    }
}

bitflags! {
    struct Cpu: u32 {
        const POPCNT = 1 << 0;
        const SSE    = 1 << 1;
        const SSE2   = 1 << 2;
        const SSSE3  = 1 << 3;
        const SSE41  = 1 << 4;
        const AVX2   = 1 << 5;
        const BMI2   = 1 << 6;
        const INTEL  = 1 << 7; // amd supports bmi2, but pext is too slow

        const SF_BMI2 = Cpu::SSE.bits | Cpu::SSE2.bits | Cpu::SSSE3.bits | Cpu::POPCNT.bits | Cpu::SSE41.bits | Cpu::AVX2.bits | Cpu::BMI2.bits | Cpu::INTEL.bits;
        const SF_AVX2 = Cpu::SSE.bits | Cpu::SSE2.bits | Cpu::SSSE3.bits | Cpu::POPCNT.bits | Cpu::SSE41.bits | Cpu::AVX2.bits;
        const SF_SSE
    }
}

impl Cpu {
    fn detect() -> Cpu {
        let mut cpu = Cpu::empty();
        cpu.set(Cpu::POPCNT, is_x86_feature_detected!("popcnt"));
        cpu.set(Cpu::SSE, is_x86_feature_detected!("sse"));
        cpu.set(Cpu::SSE2, is_x86_feature_detected!("sse"));
        cpu.set(Cpu::SSSE3, is_x86_feature_detected!("ssse3"));
        cpu.set(Cpu::SSE41, is_x86_feature_detected!("sse4.1"));
        cpu.set(Cpu::AVX2, is_x86_feature_detected!("avx2"));
        cpu.set(Cpu::BMI2, is_x86_feature_detected!("bmi2"));
        cpu.set(Cpu::INTEL, false); // TODO
        cpu
    }
}

const NNUE: Asset = Asset {
    name: "nn-c3ca321c51c9.nnue",
    data: include_bytes!("../assets/nn-c3ca321c51c9.nnue"),
    needs: Cpu::empty(),
};

#[cfg(all(unix, target_arch = "x86_64", not(target_os = "macos")))]
const STOCKFISH: &'static [Asset] = &[
    Asset {
        name: "stockfish-x86-64-bmi2",
        data: include_bytes!("../assets/stockfish-x86-64-bmi2"),
        needs: Cpu::SF_BMI2,
        needs: Cpu::from_bits_truncate(Cpu::SSE.bits | Cpu::SSE2.bits | Cpu::SSSE3.bits | Cpu::POPCNT.bits | Cpu::SSE41.bits | Cpu::AVX2.bits | Cpu::BMI2.bits | Cpu::INTEL.bits),
    },
    Asset {
        name: "stockfish-x86-64-avx2",
        data: include_bytes!("../assets/stockfish-x86-64-avx2"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2,
    },
    Asset {
        name: "stockfish-x86-64-sse41-popcnt",
        data: include_bytes!("../assets/stockfish-x86-64-sse41-popcnt"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41,
    },
    Asset {
        name: "stockfish-x86-64-ssse3",
        data: include_bytes!("../assets/stockfish-x86-64-ssse3"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3,
    },
    Asset {
        name: "stockfish-x86-64",
        data: include_bytes!("../assets/stockfish-x86-64"),
        needs: Cpu::empty(),
    },
];

#[cfg(all(unix, target_arch = "x86_64", not(target_os = "macos")))]
const STOCKFISH_MV: &'static [Asset] = &[
    Asset {
        name: "stockfish-mv-x86-64-bmi2",
        data: include_bytes!("../assets/stockfish-mv-x86-64-bmi2"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2 | Cpu::BMI2 | Cpu::INTEL,
    },
    Asset {
        name: "stockfish-mv-x86-64-avx2",
        data: include_bytes!("../assets/stockfish-mv-x86-64-avx2"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2,
    },
    Asset {
        name: "stockfish-mv-x86-64-sse41-popcnt",
        data: include_bytes!("../assets/stockfish-mv-x86-64-sse41-popcnt"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41,
    },
    Asset {
        name: "stockfish-mv-x86-64-ssse3",
        data: include_bytes!("../assets/stockfish-mv-x86-64-ssse3"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3,
    },
    Asset {
        name: "stockfish-mv-x86-64",
        data: include_bytes!("../assets/stockfish-mv-x86-64"),
        needs: Cpu::empty(),
    },
];

#[cfg(all(windows, target_arch = "x86_64"))]
const STOCKFISH: &'static [Asset] = &[
    Asset {
        name: "stockfish-x86-64-bmi2.exe",
        data: include_bytes!("../assets/stockfish-x86-64-bmi2.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2 | Cpu::BMI2 | Cpu::INTEL,
    },
    Asset {
        name: "stockfish-x86-64-avx2.exe",
        data: include_bytes!("../assets/stockfish-x86-64-avx2.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2,
    },
    Asset {
        name: "stockfish-x86-64-sse41-popcnt.exe",
        data: include_bytes!("../assets/stockfish-x86-64-sse41-popcnt.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41,
    },
    Asset {
        name: "stockfish-x86-64-ssse3.exe",
        data: include_bytes!("../assets/stockfish-x86-64-ssse3.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3,
    },
    Asset {
        name: "stockfish-x86-64.exe",
        data: include_bytes!("../assets/stockfish-x86-64.exe"),
        needs: Cpu::empty(),
    },
];

#[cfg(all(windows, target_arch = "x86_64"))]
const STOCKFISH_MV: &'static [Asset] = &[
    Asset {
        name: "stockfish-mv-x86-64-bmi2.exe",
        data: include_bytes!("../assets/stockfish-mv-x86-64-bmi2.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2 | Cpu::BMI2 | Cpu::INTEL,
    },
    Asset {
        name: "stockfish-mv-x86-64-avx2.exe",
        data: include_bytes!("../assets/stockfish-mv-x86-64-avx2.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41 | Cpu::AVX2,
    },
    Asset {
        name: "stockfish-mv-x86-64-sse41-popcnt.exe",
        data: include_bytes!("../assets/stockfish-mv-x86-64-sse41-popcnt.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3 | Cpu::POPCNT | Cpu::SSE41,
    },
    Asset {
        name: "stockfish-mv-x86-64-ssse3.exe",
        data: include_bytes!("../assets/stockfish-mv-x86-64-ssse3.exe"),
        needs: Cpu::SSE | Cpu::SSE2 | Cpu::SSSE3,
    },
    Asset {
        name: "stockfish-mv-x86-64.exe",
        data: include_bytes!("../assets/stockfish-mv-x86-64.exe"),
        needs: Cpu::empty(),
    },
];

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const STOCKFISH: &'static [Asset] = &[
    Asset {
        name: "stockfish-macos-x86-64",
        data: include_bytes!("../assets/stockfish-macos-x86-64"),
        needs: Cpu::empty(),
    },
];

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const STOCKFISH_MV: &'static [Asset] = &[
    Asset {
        name: "stockfish-mv-macos-x86-64",
        data: include_bytes!("../assets/stockfish-mv-macos-x86-64"),
        needs: Cpu::empty(),
    },
];

pub fn run(opt: Opt) {
    let cpu = Cpu::detect();
    info!("Detected CPU features: {:?}", cpu);

    dbg!(NNUE);
    dbg!(STOCKFISH);
    dbg!(STOCKFISH_MV);
    dbg!(opt);
}
