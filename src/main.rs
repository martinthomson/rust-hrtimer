use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(windows)]
mod win {
    // TODO (generate bindings properly)
    pub type UINT = ::std::os::raw::c_uint;
    pub type MMRESULT = UINT;
    extern "C" {
        pub fn timeBeginPeriod(uPeriod: UINT) -> MMRESULT;
    }
    extern "C" {
        pub fn timeEndPeriod(uPeriod: UINT) -> MMRESULT;
    }
}

fn main() {
    const DELAYS: &[u64] = &[1, 2, 3, 5, 8, 10, 12, 15, 20, 25, 30];
    let durations = DELAYS.iter().map(|&d| Duration::from_millis(d));

    #[cfg(windows)]
    unsafe {
        win::timeBeginPeriod(1)
    };

    let mut s = Instant::now();
    for i in durations {
        sleep(i);
        let e = Instant::now();
        println!("sleep({:?}) → {:?} Δ{:?})", i, e - s, e - s - i);
        s = Instant::now();
    }

    #[cfg(windows)]
    unsafe {
        win::timeEndPeriod(1)
    };
}
