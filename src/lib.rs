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

#[cfg(target_os = "macos")]
#[allow(non_camel_case_types)]
mod mac {
    use std::mem::size_of;

    // These are manually extracted from the many bindings generated
    // by bindgen when provided with the simple header:
    // #include <mach/mach_init.h>
    // #include <mach/mach_time.h>
    // #include <mach/thread_policy.h>
    // #include <pthread.h>

    type __darwin_natural_t = ::std::os::raw::c_uint;
    type __darwin_mach_port_name_t = __darwin_natural_t;
    type __darwin_mach_port_t = __darwin_mach_port_name_t;
    type mach_port_t = __darwin_mach_port_t;
    type thread_t = mach_port_t;
    type natural_t = __darwin_natural_t;
    type thread_policy_flavor_t = natural_t;
    type integer_t = ::std::os::raw::c_int;
    type thread_policy_t = *mut integer_t;
    type mach_msg_type_number_t = natural_t;
    type boolean_t = ::std::os::raw::c_uint;
    type kern_return_t = ::std::os::raw::c_int;

    #[repr(C)]
    #[derive(Debug, Copy, Clone, Default)]
    struct mach_timebase_info {
        numer: u32,
        denom: u32,
    }
    type mach_timebase_info_t = *mut mach_timebase_info;
    type mach_timebase_info_data_t = mach_timebase_info;
    extern "C" {
        fn mach_timebase_info(info: mach_timebase_info_t) -> kern_return_t;
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone, Default)]
    pub struct thread_time_constraint_policy {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: boolean_t,
    }

    const THREAD_TIME_CONSTRAINT_POLICY: thread_policy_flavor_t = 2;
    const THREAD_TIME_CONSTRAINT_POLICY_COUNT: mach_msg_type_number_t =
        (size_of::<thread_time_constraint_policy>() / size_of::<integer_t>())
            as mach_msg_type_number_t;

    // These function definitions are taken from a comment in <thread_policy.h>.
    // Why they are inaccessible is unknown, but they can be called still.
    extern "C" {
        fn thread_policy_set(
            thread: thread_t,
            flavor: thread_policy_flavor_t,
            policy_info: thread_policy_t,
            count: mach_msg_type_number_t,
        ) -> kern_return_t;
        fn thread_policy_get(
            thread: thread_t,
            flavor: thread_policy_flavor_t,
            policy_info: thread_policy_t,
            count: *mut mach_msg_type_number_t,
            get_default: *mut boolean_t,
        ) -> kern_return_t;
    }

    enum _opaque_pthread_t {} // An opaque type is fine here.
    type __darwin_pthread_t = *mut _opaque_pthread_t;
    type pthread_t = __darwin_pthread_t;

    extern "C" {
        fn pthread_self() -> pthread_t;
    }
    extern "C" {
        fn pthread_mach_thread_np(thread: pthread_t) -> mach_port_t;
    }

    /// Set a thread time policy.
    pub fn set_thread_policy(mut policy: thread_time_constraint_policy) {
        let r = unsafe {
            thread_policy_set(
                pthread_mach_thread_np(pthread_self()),
                THREAD_TIME_CONSTRAINT_POLICY,
                &mut policy as *mut thread_time_constraint_policy as *mut _,
                THREAD_TIME_CONSTRAINT_POLICY_COUNT,
            )
        };
        assert_eq!(r, 0);
    }

    /// Create a realtime policy and set it.
    pub fn set_realtime() {
        const NANOS_PER_MSEC: f64 = 1_000_000.0;
        let mut timebase_info = mach_timebase_info_data_t::default();
        unsafe {
            mach_timebase_info(&mut timebase_info);
        }
        let scale =
            f64::from(timebase_info.denom) * NANOS_PER_MSEC / f64::from(timebase_info.numer);

        let policy = thread_time_constraint_policy {
            period: scale as u32,               // 1ms interval
            computation: (scale * 5.0) as u32,  // 5ms of processing expected
            constraint: (scale * 100.0) as u32, // maximum of 100ms processing
            preemptible: 1,
        };
        set_thread_policy(policy);
    }

    /// Get the default policy.
    pub fn get_default_policy() -> thread_time_constraint_policy {
        let mut policy = thread_time_constraint_policy::default();
        let mut count = THREAD_TIME_CONSTRAINT_POLICY_COUNT;
        let mut get_default = 0;
        let r = unsafe {
            thread_policy_get(
                pthread_mach_thread_np(pthread_self()),
                THREAD_TIME_CONSTRAINT_POLICY,
                &mut policy as *mut thread_time_constraint_policy as *mut _,
                &mut count,
                &mut get_default,
            )
        };
        assert_eq!(r, 0);
        policy
    }
}

/// Holding an instance of this indicates that high resolution timers are enabled.
pub struct HrTime {
    #[cfg(target_os = "macos")]
    deflt: mac::thread_time_constraint_policy,
}
impl HrTime {
    fn init() -> Self {
        let hrt = HrTime {
            #[cfg(target_os = "macos")]
            deflt: mac::get_default_policy(),
        };

        #[cfg(target_os = "macos")]
        mac::set_realtime();

        #[cfg(windows)]
        assert_eq!(0, unsafe { win::timeBeginPeriod(1) });

        hrt
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
        #[cfg(target_os = "macos")]
        mac::set_thread_policy(self.deflt);

        #[cfg(windows)]
        assert_eq!(0, unsafe { win::timeEndPeriod(1) });
    }
}

#[cfg(test)]
mod test {
    use super::HrTime;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    fn check_delays(max_lag: Duration) {
        const DELAYS: &[u64] = &[1, 2, 3, 5, 8, 10, 12, 15, 20, 25, 30];
        let durations = DELAYS.iter().map(|&d| Duration::from_millis(d));

        let mut s = Instant::now();
        for d in durations {
            sleep(d);
            let e = Instant::now();
            let actual = e - s;
            let lag = actual - d;
            println!("sleep({:?}) → {:?} Δ{:?})", d, actual, lag);
            assert!(lag < max_lag);
            s = Instant::now();
        }
    }

    /// Note that you have to run this test alone or other tests will
    /// grab the high resolution timer and this will run faster.
    #[test]
    fn baseline_timer() {
        check_delays(Duration::from_millis(30)); // a generous limit
    }

    #[test]
    fn hr_timer() {
        let _hrt = HrTime::get();
        check_delays(Duration::from_micros(1500)); // not a generous limit
    }
}
