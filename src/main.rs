use std::thread::sleep;
use std::time::{Duration, Instant};

mod lib;

fn main() {
    const DELAYS: &[u64] = &[1, 2, 3, 5, 8, 10, 12, 15, 20, 25, 30];
    let durations = DELAYS.iter().map(|&d| Duration::from_millis(d));

    let _hrt = lib::HrTime::get();

    let mut s = Instant::now();
    for i in durations {
        sleep(i);
        let e = Instant::now();
        println!("sleep({:?}) → {:?} Δ{:?})", i, e - s, e - s - i);
        s = Instant::now();
    }
}
