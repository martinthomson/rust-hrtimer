use std::cell::RefCell;
use std::cmp::{max, min};
use std::convert::TryFrom;
use std::rc::{Rc, Weak};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Period(u8);
impl Period {
    const MAX: Period = Period(16);
    const MIN: Period = Period(1);

    #[cfg(windows)]
    fn as_uint(&self) -> win::UINT {
        win::UINT::from(self.0)
    }

    #[cfg(target_os = "macos")]
    fn scaled(&self, scale: f64) -> f64 {
        scale * f64::from(self.0)
    }
}

impl From<Duration> for Period {
    fn from(p: Duration) -> Self {
        let rounded =
            u8::try_from((p + Duration::from_nanos(999_999)).as_millis()).unwrap_or(Self::MAX.0);
        Self(max(Self::MIN.0, min(rounded, Self::MAX.0)))
    }
}

/// This counts instances of `Period`, except those of `Period::MAX`.
#[derive(Default)]
struct PeriodSet {
    counts: [usize; (Period::MAX.0 - Period::MIN.0) as usize],
}
impl PeriodSet {
    fn idx(&mut self, p: Period) -> &mut usize {
        debug_assert!(p >= Period::MIN);
        &mut self.counts[usize::from(p.0 - Period::MIN.0)]
    }

    fn add(&mut self, p: Period) {
        if p != Period::MAX {
            *self.idx(p) += 1;
        }
    }

    fn remove(&mut self, p: Period) {
        if p != Period::MAX {
            debug_assert_ne!(*self.idx(p), 0);
            *self.idx(p) -= 1;
        }
    }

    fn min(&self) -> Option<Period> {
        for (i, v) in self.counts.iter().enumerate() {
            if *v > 0 {
                return Some(Period(u8::try_from(i).unwrap() + Period::MIN.0));
            }
        }
        None
    }
}

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
    // Why they are inaccessible is unknown, but they work as declared.
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

    pub fn get_scale() -> f64 {
        const NANOS_PER_MSEC: f64 = 1_000_000.0;
        let mut timebase_info = mach_timebase_info_data_t::default();
        unsafe {
            mach_timebase_info(&mut timebase_info);
        }
        f64::from(timebase_info.denom) * NANOS_PER_MSEC / f64::from(timebase_info.numer)
    }

    /// Create a realtime policy and set it.
    pub fn set_realtime(base: f64) {
        let policy = thread_time_constraint_policy {
            period: base as u32,               // Base interval
            computation: (base * 5.0) as u32,  // Generous allowance
            constraint: (base * 100.0) as u32, // Even more generous
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

/// A handle for a high-resolution timer of a specific period.
pub struct HrPeriod {
    period: Period,
    hrt: Rc<RefCell<HrTime>>,
}

impl HrPeriod {
    pub fn update(&mut self, period: Duration) {
        let new = Period::from(period);
        if new != self.period {
            let mut b = self.hrt.borrow_mut();
            b.periods.remove(self.period);
            self.period = new;
            b.periods.add(self.period);
            b.update();
        }
    }
}

impl Drop for HrPeriod {
    fn drop(&mut self) {
        self.hrt.borrow_mut().remove(self.period);
    }
}

/// Holding an instance of this indicates that high resolution timers are enabled.
pub struct HrTime {
    periods: PeriodSet,
    active: Option<Period>,

    #[cfg(target_os = "macos")]
    scale: f64,
    #[cfg(target_os = "macos")]
    deflt: mac::thread_time_constraint_policy,
}
impl HrTime {
    fn new() -> Self {
        let hrt = HrTime {
            periods: PeriodSet::default(),
            active: None,

            #[cfg(target_os = "macos")]
            scale: mac::get_scale(),
            #[cfg(target_os = "macos")]
            deflt: mac::get_default_policy(),
        };
        hrt
    }

    fn start(&self) {
        #[cfg(target_os = "macos")]
        if let Some(p) = self.active {
            mac::set_realtime(p.scaled(self.scale));
        } else {
            mac::set_thread_policy(self.deflt.clone());
        }

        #[cfg(windows)]
        if let Some(p) = self.active {
            assert_eq!(0, unsafe { win::timeBeginPeriod(p.as_uint()) });
        }
    }

    fn stop(&self) {
        #[cfg(windows)]
        if let Some(p) = self.active {
            assert_eq!(0, unsafe { win::timeEndPeriod(p.as_uint()) });
        }
    }

    fn update(&mut self) {
        let next = self.periods.min();
        if next != self.active {
            self.stop();
            self.active = next;
            self.start();
        }
    }

    fn add(&mut self, p: Period) {
        self.periods.add(p);
        self.update();
    }

    fn remove(&mut self, p: Period) {
        self.periods.remove(p);
        self.update();
    }

    /// Acquire a reference to the object.
    pub fn get(period: Duration) -> HrPeriod {
        thread_local! {
            static HR_TIME: RefCell<Weak<RefCell<HrTime>>> = RefCell::default();
        }

        HR_TIME.with(|r| {
            let mut b = r.borrow_mut();
            let hrt = if let Some(hrt) = b.upgrade() {
                hrt
            } else {
                let hrt = Rc::new(RefCell::new(HrTime::new()));
                *b = Rc::downgrade(&hrt);
                hrt
            };

            let p = Period::from(period);
            hrt.borrow_mut().add(p);
            HrPeriod { hrt, period: p }
        })
    }
}

impl Drop for HrTime {
    fn drop(&mut self) {
        self.stop();

        #[cfg(target_os = "macos")]
        if self.active.is_some() {
            mac::set_thread_policy(self.deflt);
        }
    }
}

#[cfg(test)]
mod test {
    use super::HrTime;
    use std::thread::{sleep, spawn};
    use std::time::{Duration, Instant};

    const ONE: Duration = Duration::from_millis(1);
    const ONE_AND_A_BIT: Duration = Duration::from_micros(1500);
    /// A limit for when high resolution timers are disabled.
    const GENEROUS: Duration = Duration::from_millis(30);

    fn check_delays(max_lag: Duration) {
        const DELAYS: &[u64] = &[1, 2, 3, 5, 8, 10, 12, 15, 20, 25, 30];
        let durations = DELAYS.iter().map(|&d| Duration::from_millis(d));

        let mut s = Instant::now();
        for d in durations {
            sleep(d);
            let e = Instant::now();
            let actual = e - s;
            let lag = actual - d;
            println!("sleep({:?}) → {:?} Δ{:?}", d, actual, lag);
            assert!(lag < max_lag);
            s = Instant::now();
        }
    }

    /// Note that you have to run this test alone or other tests will
    /// grab the high resolution timer and this will run faster.
    #[test]
    fn baseline() {
        check_delays(GENEROUS);
    }

    #[test]
    fn one_ms() {
        let _hrt = HrTime::get(ONE);
        check_delays(ONE_AND_A_BIT);
    }

    #[test]
    fn multithread_baseline() {
        let thr = spawn(move || {
            baseline();
        });
        baseline();
        thr.join().unwrap();
    }

    #[test]
    fn one_ms_multi() {
        let thr = spawn(move || {
            one_ms();
        });
        one_ms();
        thr.join().unwrap();
    }

    #[test]
    fn mixed_multi() {
        let thr = spawn(move || {
            one_ms();
        });
        let _hrt = HrTime::get(Duration::from_millis(4));
        check_delays(Duration::from_millis(5));
        thr.join().unwrap();
    }

    #[test]
    fn update() {
        let mut hrt = HrTime::get(Duration::from_millis(4));
        check_delays(Duration::from_millis(5));
        hrt.update(ONE);
        check_delays(ONE_AND_A_BIT);
    }

    #[test]
    fn update_multi() {
        let thr = spawn(move || {
            update();
        });
        update();
        thr.join().unwrap();
    }

    #[test]
    fn max() {
        let _hrt = HrTime::get(Duration::from_secs(1));
        check_delays(GENEROUS);
    }
}
