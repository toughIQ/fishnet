use std::fmt;
use crate::configure::Opt;

struct Asset {
    name: &'static str,
    data: &'static [u8],
}

impl fmt::Debug for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asset")
            .field("name", &self.name)
            .finish()
    }
}

const NNUE: Asset = Asset {
    name: "nn-c3ca321c51c9.nnue",
    data: include_bytes!("../assets/nn-c3ca321c51c9.nnue"),
};

const STOCKFISH: &'static [Asset] = &[
    Asset {
        name: "stockfish-x86-64-bmi2",
        data: include_bytes!("../assets/stockfish-x86-64-bmi2"),
    },
    Asset {
        name: "stockfish-x86-64-avx2",
        data: include_bytes!("../assets/stockfish-x86-64-avx2"),
    },
    Asset {
        name: "stockfish-x86-64-sse41-popcnt",
        data: include_bytes!("../assets/stockfish-x86-64-sse41-popcnt"),
    },
    Asset {
        name: "stockfish-x86-64-ssse3",
        data: include_bytes!("../assets/stockfish-x86-64-ssse3"),
    },
    Asset {
        name: "stockfish-x86-64",
        data: include_bytes!("../assets/stockfish-x86-64"),
    },
];

const STOCKFISH_MV: &'static [Asset] = &[
    Asset {
        name: "stockfish-mv-x86-64-bmi2",
        data: include_bytes!("../assets/stockfish-mv-x86-64-bmi2"),
    },
    Asset {
        name: "stockfish-mv-x86-64-avx2",
        data: include_bytes!("../assets/stockfish-mv-x86-64-avx2"),
    },
    Asset {
        name: "stockfish-mv-x86-64-sse41-popcnt",
        data: include_bytes!("../assets/stockfish-mv-x86-64-sse41-popcnt"),
    },
    Asset {
        name: "stockfish-mv-x86-64-ssse3",
        data: include_bytes!("../assets/stockfish-mv-x86-64-ssse3"),
    },
    Asset {
        name: "stockfish-mv-x86-64",
        data: include_bytes!("../assets/stockfish-mv-x86-64"),
    },
];

#[derive(Debug)]
struct Cpuid {
    bmi2: bool,
    avx2: bool,
    sse41: bool,
    sse3: bool,
    ssse3: bool,
    popcnt: bool,
}

impl Cpuid {
    fn detect() -> Cpuid {
        Cpuid {
            bmi2: is_x86_feature_detected!("bmi2"),
            avx2: is_x86_feature_detected!("avx2"),
            sse3: is_x86_feature_detected!("sse3"),
            sse41: is_x86_feature_detected!("sse4.1"),
            ssse3: is_x86_feature_detected!("ssse3"),
            popcnt: is_x86_feature_detected!("popcnt"),
        }
    }
}

pub fn run(opt: Opt) {
    dbg!(NNUE);
    dbg!(STOCKFISH);
    dbg!(STOCKFISH_MV);
    dbg!(opt);
    dbg!(Cpuid::detect());
}
