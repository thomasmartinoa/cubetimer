#[path = "../src/cube.rs"]
#[allow(dead_code)]
mod cube;
#[path = "../src/twophase.rs"]
#[allow(dead_code)]
mod twophase;
use std::time::Instant;
fn main() {
    let t = Instant::now();
    twophase::warm_up();
    println!("tables: {:?}", t.elapsed());
    let mut rng = rand::rng();
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1000);
    let mut times = Vec::new();
    let mut lens = [0usize; 23];
    for _ in 0..n {
        let c = cube::Cube::random(&mut rng);
        let t = Instant::now();
        let sol = twophase::solve(&c, 21).unwrap();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(c.apply_all(&sol), cube::SOLVED);
        lens[sol.len()] += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = times.iter().sum::<f64>() / n as f64;
    println!(
        "n={n} mean={mean:.2}ms median={:.2}ms p99={:.2}ms max={:.2}ms",
        times[n / 2],
        times[n * 99 / 100],
        times[n - 1]
    );
    println!(
        "lengths: {:?}",
        lens.iter()
            .enumerate()
            .filter(|&(_, &k)| k > 0)
            .collect::<Vec<_>>()
    );
}
