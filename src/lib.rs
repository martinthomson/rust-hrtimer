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

/// Marker type that indicates that high resolution timers are enabled.
pub struct HrTime {}
impl HrTime {
    fn init() -> Self {
        #[cfg(windows)]
        assert_eq!(0, unsafe { win::timeBeginPeriod(1) });
        HrTime {}
    }

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
