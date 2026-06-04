#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuFeatures {
    pub architecture: String,
    pub backend: &'static str,
    pub features: Vec<String>,
}

pub fn get_cpu_features() -> CpuFeatures {
    let mut features = Vec::new();
    collect_cpu_features(&mut features);
    features.sort();
    features.dedup();

    CpuFeatures {
        architecture: std::env::consts::ARCH.to_string(),
        backend: super::LOWLEVEL_BACKEND,
        features,
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn collect_cpu_features(features: &mut Vec<String>) {
    for (name, detected) in [
        ("sse2", std::is_x86_feature_detected!("sse2")),
        ("sse4.2", std::is_x86_feature_detected!("sse4.2")),
        ("avx", std::is_x86_feature_detected!("avx")),
        ("avx2", std::is_x86_feature_detected!("avx2")),
        ("aes", std::is_x86_feature_detected!("aes")),
    ] {
        if detected {
            features.push(name.to_string());
        }
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn collect_cpu_features(features: &mut Vec<String>) {
    for name in compile_time_features() {
        features.push(name.to_string());
    }
}

#[cfg(all(
    not(any(target_arch = "x86", target_arch = "x86_64")),
    target_arch = "aarch64"
))]
fn compile_time_features() -> &'static [&'static str] {
    &[
        #[cfg(target_feature = "aes")]
        "aes",
        #[cfg(target_feature = "neon")]
        "neon",
    ]
}

#[cfg(all(
    not(any(target_arch = "x86", target_arch = "x86_64")),
    not(target_arch = "aarch64")
))]
fn compile_time_features() -> &'static [&'static str] {
    &[]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_features_have_architecture_and_backend() {
        let features = get_cpu_features();

        assert!(!features.architecture.trim().is_empty());
        assert!(!features.backend.trim().is_empty());
    }
}
