use super::exp;

/// The distance in ulps between two finite doubles of the same sign.
fn ulps(a: f64, b: f64) -> u64 {
    a.to_bits().abs_diff(b.to_bits())
}

#[test]
fn ends_of_the_range() {
    assert_eq!(exp(0.0), 1.0);
    assert_eq!(exp(-0.0), 1.0);
    assert_eq!(exp(1.0), std::f64::consts::E);
    assert_eq!(exp(f64::NEG_INFINITY), 0.0);
    assert_eq!(exp(f64::INFINITY), f64::INFINITY);
    assert!(exp(f64::NAN).is_nan());
    assert_eq!(exp(710.0), f64::INFINITY);
    assert_eq!(exp(-746.0), 0.0);
    assert_eq!(exp(709.0), 8.218407461554972e307);
    // Subnormal results.
    assert_eq!(exp(-740.0), 4.2e-322);
    assert_eq!(exp(1e-300), 1.0);
}

/// The platform's exp is close to correctly rounded (glibc's is musl's
/// algorithm), so the two agree almost always and are never an ulp apart
/// more than that.
#[test]
fn agrees_with_the_platforms_exp() {
    let mut z = 0x2545_f491_4f6c_dd1du64;
    let mut differ = 0;
    let n = 200_000;
    for _ in 0..n {
        z ^= z << 13;
        z ^= z >> 7;
        z ^= z << 17;
        let x = (z >> 11) as f64 / (1u64 << 53) as f64 * 1400.0 - 700.0;
        let (ours, theirs) = (exp(x), x.exp());
        assert!(ulps(ours, theirs) <= 1, "exp({x:e}): {ours:e} against {theirs:e}");
        differ += usize::from(ours != theirs);
    }
    assert!(differ * 100 < n, "{differ} of {n} differ");
}
