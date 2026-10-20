//! One isolated runtime feature boundary; arithmetic and slice accesses stay checked.
#[inline(always)]
pub(crate) fn portable(fields: [&[i64]; 5], output: &mut [i64]) {
    assert!(fields.iter().all(|f| f.len() == output.len()));
    for (i, out) in output.iter_mut().enumerate() {
        *out = fields[0][i]
            .saturating_add(fields[1][i])
            .saturating_add(fields[2][i])
            .saturating_sub(fields[3][i])
            .saturating_sub(fields[4][i]);
    }
}
pub fn vector_backend() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        return "avx2";
    }
    "portable"
}
pub(crate) fn dispatch(fields: [&[i64]; 5], output: &mut [i64]) {
    #[cfg(target_arch = "x86_64")]
    if output.len() >= 32 && std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: runtime detection establishes the only target-feature precondition.
        // The implementation uses checked slices and normal Rust integer arithmetic.
        unsafe {
            avx2(fields, output);
        }
        return;
    }
    portable(fields, output);
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline(never)]
unsafe fn avx2(fields: [&[i64]; 5], output: &mut [i64]) {
    portable(fields, output);
}
