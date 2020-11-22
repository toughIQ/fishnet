use crate::configure::Opt;

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
    dbg!(opt);
    dbg!(Cpuid::detect());
}
