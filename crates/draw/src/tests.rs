use super::*;

#[test]
fn the_draws_are_spread_and_independent() {
    let n = 20_000;
    let mean = (0..n).map(|p| draw(1, 0, p, 2)).sum::<f64>() / n as f64;
    assert!((mean - 0.5).abs() < 0.01);
    let same = (0..n).filter(|&p| draw(1, 0, p, 2) == draw(1, 1, p, 2)).count();
    assert_eq!(same, 0);
    let apart = (0..n).filter(|&p| draw(1, 0, p, 2) == draw(1, 0, p, 3)).count();
    assert_eq!(apart, 0);
    assert!((0..n).all(|p| (0.0..1.0).contains(&draw(p, 3, p, 1))));
}

#[test]
fn the_same_arguments_the_same_number() {
    assert_eq!(draw(4417, 5, 12, 1), draw(4417, 5, 12, 1));
    assert_eq!(own_seed(4417, 0), 4417);
    assert_ne!(own_seed(4417, 9), own_seed(4417, 10));
}

#[test]
fn pick_never_picks_the_last_again() {
    for p in 0..10_000u64 {
        let u = draw(7, 0, p, 0);
        let last = (p % 4) as usize;
        let k = pick(4, Some(last), u);
        assert!(k < 4 && k != last, "{k} after {last}");
    }
    // Each of the others as often.
    let mut count = [0usize; 4];
    for p in 0..30_000u64 {
        count[pick(4, Some(0), draw(8, 0, p, 0))] += 1;
    }
    assert_eq!(count[0], 0);
    assert!(count[1..].iter().all(|c| (*c as f64 - 10_000.0).abs() < 400.0), "{count:?}");
    assert_eq!(pick(1, Some(0), 0.7), 0, "one: that one");
    assert_eq!(pick(3, None, 0.999_999), 2);
}

#[test]
fn spreads() {
    assert_eq!(spread(-6.0, 0.0, 0.3), -6.0);
    assert_eq!(spread(-6.0, 2.0, 0.0), -8.0);
    assert_eq!(spread(-6.0, 2.0, 0.5), -6.0);
    assert!((between(0.5, 0.9, 0.5) - 0.7).abs() < 1e-12);
}
