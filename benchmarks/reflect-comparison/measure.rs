use std::time::Instant;

fn measure<T>(name: &str, mut operation: impl FnMut() -> T) {
    let iterations = std::env::var("ITERATIONS")
        .ok()
        .map(|value| value.parse::<u64>().unwrap())
        .unwrap_or(2_000_000);
    for _ in 0..10_000 {
        black_box(operation());
    }
    let mut samples = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(operation());
        }
        samples.push(start.elapsed().as_secs_f64() * 1e9 / iterations as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!("{name}\t{:.3}\t{:.3}\t{:.3}", samples[4], samples[0], samples[8]);
}
