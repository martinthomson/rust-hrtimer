use lazy_static::lazy_static;
use std::sync::{Arc, Mutex, Weak};

#[cfg(windows)]
mod win {
    // These are manually extracted from the 10Mb bindings generated
    // by bindgen when provided with the simple header:
    // #include <windows.h>
    // #include <timeapi.h>
    // The complete bindings don't compile and filtering them is work.
    pub type UINT = ::std::os::raw::c_uint;
    pub type MMRESULT = UINT;
    extern "C" {
        pub fn timeBeginPeriod(uPeriod: UINT) -> MMRESULT;
        pub fn timeEndPeriod(uPeriod: UINT) -> MMRESULT;
    }
}

/// Holding an instance of this indicates that high resolution timers are enabled.
pub struct HrTime {}
impl HrTime {
    fn init() -> Self {
        #[cfg(windows)]
        assert_eq!(0, unsafe { win::timeBeginPeriod(1) });
        HrTime {}
    }

    /// Acquire a reference to the object.
    pub fn get() -> Arc<Self> {
        lazy_static! {
            static ref HR_TIME: Mutex<Weak<HrTime>> = Mutex::default();
        }

        let mut hrt = HR_TIME.lock().unwrap();
        if let Some(r) = hrt.upgrade() {
            r
        } else {
            let r = Arc::new(Self::init());
            *hrt = Arc::downgrade(&r);
            r
        }
    }
}

impl Drop for HrTime {
    fn drop(&mut self) {
        #[cfg(windows)]
        assert_eq!(0, unsafe { win::timeEndPeriod(1) });
    }
}

#[cfg(test)]
mod test {
    use super::HrTime;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    #[test]
    fn check_delays() {
        const DELAYS: &[u64] = &[1, 2, 3, 5, 8, 10, 12, 15, 20, 25, 30];
        let durations = DELAYS.iter().map(|&d| Duration::from_millis(d));

        let _hrt = HrTime::get();

        let mut s = Instant::now();
        for d in durations {
            sleep(d);
            let e = Instant::now();
            let actual = e - s;
            let lag = actual - d;
            println!("sleep({:?}) → {:?} Δ{:?})", d, actual, lag);
            assert!(lag < Duration::from_millis(2));
            s = Instant::now();
        }
    }
}
