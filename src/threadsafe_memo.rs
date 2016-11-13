use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::ptr;
use std::thread::{self, Thread};
use std::marker::Sync;
use std::panic::{UnwindSafe, RefUnwindSafe};

const UNCALCULATED: usize = 1;
const WORKING: usize = 0; // either calculating or unpoisoning
const CALCULATED: usize = 2;
const POISONED: usize = 3;
const STATE_MASK: usize = 3;

struct SpinState {
    thread: Thread,
    signaled: AtomicBool,
    next: *const SpinState,
}

struct Finish<'a> {
    destination_state: usize,
    state: &'a AtomicUsize,
}

struct ThreadsafeMemoCore<T, F: FnOnce() -> T> {
    func: Option<F>,
    value: Option<T>,
}

pub struct ThreadsafeMemo<T, F: FnOnce() -> T> {
    state: AtomicUsize,
    core: UnsafeCell<ThreadsafeMemoCore<T, F>>,
}

impl<T, F: FnOnce() -> T> ThreadsafeMemo<T, F> {
    pub fn new(func: F) -> ThreadsafeMemo<T, F> {
        ThreadsafeMemo {
            state: AtomicUsize::new(UNCALCULATED),
            core: UnsafeCell::new(ThreadsafeMemoCore {
                func: Some(func),
                value: None,
            }),
        }
    }

    pub fn with_value(value: T) -> ThreadsafeMemo<T, F> {
        ThreadsafeMemo {
            state: AtomicUsize::new(CALCULATED),
            core: UnsafeCell::new(ThreadsafeMemoCore {
                func: None,
                value: Some(value),
            }),
        }
    }
}

impl<'a, T, F: FnOnce() -> T> ThreadsafeMemo<T, F> {
    pub fn get(&self) -> Result<&T, ()> {
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            match state {
                POISONED => return Err(()),
                CALCULATED => return unsafe { Ok((*self.core.get()).value.as_ref().unwrap()) },
                UNCALCULATED => {
                    if let Err(new_state) = self.state.compare_exchange(UNCALCULATED,
                                                                        WORKING,
                                                                        Ordering::AcqRel,
                                                                        Ordering::Acquire) {
                        state = new_state;
                        continue;
                    }
                    let mut finish = Finish {
                        destination_state: POISONED,
                        state: &self.state,
                    };
                    let core = unsafe { &mut *self.core.get() };
                    core.value = Some(core.func.take().unwrap()());
                    let out = Ok(core.value.as_ref().unwrap());
                    finish.destination_state = CALCULATED;
                    return out;
                },
                _ => {
                    assert_eq!(state & STATE_MASK, WORKING);
                    let mut spin_state = SpinState {
                        thread: thread::current(),
                        signaled: AtomicBool::new(false),
                        next: ptr::null(),
                    };
                    let spin_state_ptr = &spin_state as *const SpinState as usize;
                    assert_eq!(spin_state_ptr & STATE_MASK, 0);

                    while state & STATE_MASK == WORKING {
                        spin_state.next = (state & !STATE_MASK) as *const SpinState;

                        if let Err(new_state) = self.state.compare_exchange(state,
                                                                            spin_state_ptr | WORKING,
                                                                            Ordering::AcqRel,
                                                                            Ordering::Acquire) {
                            state = new_state;
                            continue;
                        }

                        while !spin_state.signaled.load(Ordering::Acquire) {
                            thread::park();
                        }

                        state = self.state.load(Ordering::Acquire);
                        break;
                    }
                }
            }
        }
    }

    pub fn try_get(&self) -> Result<Option<&T>, ()> {
        match self.state.load(Ordering::Acquire) {
            POISONED => Err(()),
            CALCULATED => unsafe { Ok((*self.core.get()).value.as_ref()) },
            _ => Ok(None)
        }
    }

    pub fn take(self) -> Result<T, ()> {
        match (self.state.into_inner(), unsafe { self.core.into_inner() }) {
            (POISONED, _) => Err(()),
            (UNCALCULATED, ThreadsafeMemoCore { func: Some(func), value: None }) => Ok(func()),
            (CALCULATED, ThreadsafeMemoCore { func: None, value: Some(value) }) => Ok(value),
            _ => panic!("ThreadsafeMemo had an invalid state!")
        }
    }

    pub fn try_take(self) -> Result<Option<T>, ()> {
        match (self.state.into_inner(), unsafe { self.core.into_inner() }) {
            (POISONED, _) => Err(()),
            (UNCALCULATED, _) => Ok(None),
            (CALCULATED, ThreadsafeMemoCore { func: None, value: Some(value) }) => Ok(Some(value)),
            _ => panic!("ThreadsafeMemo had an invalid state!")
        }
    }

    pub fn unpoison(&self, func: F) -> bool {
        match self.state.compare_exchange(POISONED,
                                          WORKING,
                                          Ordering::AcqRel,
                                          Ordering::Acquire) {
            Ok(_) => {
                let mut finish = Finish {
                    destination_state: POISONED,
                    state: &self.state,
                };
                unsafe {
                    *self.core.get() = ThreadsafeMemoCore {
                        func: Some(func),
                        value: None,
                    };
                }
                finish.destination_state = UNCALCULATED;
                true
            },
            Err(_) => false,
        }
    }

    pub fn unpoison_with_value(&self, value: T) -> bool {
        match self.state.compare_exchange(POISONED,
                                          WORKING,
                                          Ordering::AcqRel,
                                          Ordering::Acquire) {
            Ok(_) => {
                let mut finish = Finish {
                    destination_state: POISONED,
                    state: &self.state,
                };
                unsafe {
                    *self.core.get() = ThreadsafeMemoCore {
                        func: None,
                        value: Some(value),
                    };
                }
                finish.destination_state = CALCULATED;
                true
            },
            Err(_) => false,
        }
    }
}

unsafe impl<'a, T, F: FnOnce() -> T> Sync for ThreadsafeMemo<T, F> where T: Sync, F: Sync {  }
impl<'a, T, F: FnOnce() -> T> UnwindSafe for ThreadsafeMemo<T, F> where T: UnwindSafe, F: UnwindSafe {  }
impl<'a, T, F: FnOnce() -> T> RefUnwindSafe for ThreadsafeMemo<T, F> where T: RefUnwindSafe, F: RefUnwindSafe {  }

impl<'a> Drop for Finish<'a> {
    fn drop(&mut self) {
        let state = self.state.swap(self.destination_state, Ordering::Release);
        assert_eq!(state & STATE_MASK, WORKING);

        let mut head = (state & !STATE_MASK) as *const SpinState;
        while !head.is_null() {
            let spin_state = unsafe { &*head };
            head = spin_state.next;
            spin_state.signaled.store(true, Ordering::Release);
            spin_state.thread.unpark();
        }
    }
}

#[cfg(test)]
#[allow(unused_assignments)]
mod tests {
    mod new {
        use super::super::ThreadsafeMemo;

        #[test]
        fn get() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212
                });
                assert_eq!(*memo.get().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn try_get() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212
                });
                assert!(memo.try_get().unwrap().is_none());
            }
            assert_eq!(times, 0);
        }

        #[test]
        fn take() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212
                });
                assert_eq!(memo.take().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn try_take() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212
                });
                assert!(memo.try_take().unwrap().is_none());
            }
            assert_eq!(times, 0);
        }

        #[test]
        fn get_get() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get().unwrap(), 212);
                assert_eq!(*memo.get().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn get_try_get() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get().unwrap(), 212);
                assert_eq!(*memo.try_get().unwrap().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn get_take() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get().unwrap(), 212);
                assert_eq!(memo.take().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn get_try_take() {
            let mut times = 0;
            {
                let memo = ThreadsafeMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get().unwrap(), 212);
                assert_eq!(memo.try_take().unwrap().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }
    }

    mod with_value {
        use super::super::ThreadsafeMemo;

        #[test]
        fn get() {
            let mut memo = ThreadsafeMemo::new(|| { 200 });
            memo = ThreadsafeMemo::with_value(212);
            assert_eq!(*memo.get().unwrap(), 212);
        }

        #[test]
        fn try_get() {
            let mut memo = ThreadsafeMemo::new(|| { 200 });
            memo = ThreadsafeMemo::with_value(212);
            assert_eq!(*memo.try_get().unwrap().unwrap(), 212);
        }

        #[test]
        fn take() {
            let mut memo = ThreadsafeMemo::new(|| { 200 });
            memo = ThreadsafeMemo::with_value(212);
            assert_eq!(memo.take().unwrap(), 212);
        }

        #[test]
        fn try_take() {
            let mut memo = ThreadsafeMemo::new(|| { 200 });
            memo = ThreadsafeMemo::with_value(212);
            assert_eq!(memo.try_take().unwrap().unwrap(), 212);
        }
    }

    mod concurrency {
        use super::super::ThreadsafeMemo;
        use std::sync::mpsc::channel;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;
        use std::panic::{self, RefUnwindSafe};
        use std::time::Duration;

        #[test]
        fn stampede() {
            let (tx, rx) = channel();
            let times = Arc::new(AtomicUsize::new(0));
            let memo = {
                let times = times.clone();
                Arc::new(ThreadsafeMemo::new(move || {
                    for _ in 0..3 {
                        thread::yield_now();
                    }
                    times.fetch_add(1, Ordering::Release);
                    212
                }))
            };
            for _ in 0..12 {
                let tx = tx.clone();
                let memo = memo.clone();
                thread::spawn(move || {
                    for _ in 0..6 {
                        thread::yield_now();
                    }
                    assert_eq!(*memo.get().unwrap(), 212);
                    tx.send(()).unwrap();
                });
            }
            for _ in 0..12 {
                rx.recv().unwrap();
            }
            assert_eq!(times.load(Ordering::Acquire), 1);
        }

        #[test]
        fn race() {
            let (tx, rx) = channel();
            let times = Arc::new(AtomicUsize::new(0));
            let memo = {
                let times = times.clone();
                Arc::new(ThreadsafeMemo::new(move || {
                    for _ in 0..3 {
                        thread::yield_now();
                    }
                    times.fetch_add(1, Ordering::Release);
                    212
                }))
            };
            for _ in 0..12 {
                let tx = tx.clone();
                let memo = memo.clone();
                thread::spawn(move || {
                    assert_eq!(*memo.get().unwrap(), 212);
                    tx.send(()).unwrap();
                });
            }
            for _ in 0..12 {
                rx.recv().unwrap();
            }
            assert_eq!(times.load(Ordering::Acquire), 1);
        }

        #[test]
        #[allow(unused_must_use)]
        fn poison() {
            let memo = ThreadsafeMemo::new(|| {
                panic!();
            });
            panic::catch_unwind(|| {
                memo.get();
            }).unwrap_err();
            memo.get().unwrap_err();
        }

        #[test]
        fn poison_race() {
            let (tx, rx) = channel();
            let memo = Arc::new(ThreadsafeMemo::new(|| {
                for _ in 0..3 {
                    thread::yield_now();
                }
                panic!();
            }));
            for i in 0..12 {
                let tx = tx.clone();
                let memo = memo.clone();
                thread::spawn(move || {
                    if i >= 6 {
                        for _ in 0..6 {
                            thread::yield_now();
                        }
                    }
                    memo.get().unwrap_err();
                    tx.send(()).unwrap();
                });
            }
            for _ in 0..11 {
                rx.recv().unwrap();
            }
            rx.recv_timeout(Duration::from_millis(500)).unwrap_err();
            memo.get().unwrap_err();
        }

        struct PoisonCallback {
            times: Arc<AtomicUsize>,
            panic: bool,
            value: u32,
        }

        impl FnOnce<()> for PoisonCallback {
            type Output = u32;
            extern "rust-call" fn call_once(self, _args: ()) -> Self::Output {
                for _ in 0..3 {
                    thread::yield_now();
                }
                self.times.fetch_add(1, Ordering::SeqCst);
                if self.panic {
                    panic!();
                } else {
                    self.value
                }
            }
        }

        impl RefUnwindSafe for PoisonCallback {  }

        #[test]
        #[allow(unused_must_use)]
        fn unpoison() {
            let times = Arc::new(AtomicUsize::new(0));
            let memo = ThreadsafeMemo::new(PoisonCallback {
                times: times.clone(),
                panic: true,
                value: 0,
            });
            assert!(!memo.unpoison(PoisonCallback {
                times: times.clone(),
                panic: false,
                value: 0,
            }));
            panic::catch_unwind(|| {
                memo.get();
            }).unwrap_err();
            memo.get().unwrap_err();
            assert!(memo.unpoison(PoisonCallback {
                times: times.clone(),
                panic: false,
                value: 212,
            }));
            assert_eq!(*memo.get().unwrap(), 212);
            assert_eq!(times.load(Ordering::SeqCst), 2);
        }

        #[test]
        fn unpoison_race() {
            let (tx, rx) = channel();
            let times = Arc::new(AtomicUsize::new(0));
            let memo = Arc::new(ThreadsafeMemo::new(PoisonCallback {
                times: times.clone(),
                panic: true,
                value: 0,
            }));
            for i in 0..12 {
                let tx = tx.clone();
                let memo = memo.clone();
                let times = times.clone();
                thread::spawn(move || {
                    if i >= 6 {
                        for _ in 0..6 {
                            thread::yield_now();
                        }
                    }
                    let mut got = memo.get();
                    let mut out = false;
                    if got.is_err() {
                        out = memo.unpoison(PoisonCallback {
                            times: times,
                            panic: false,
                            value: 212,
                        });
                        got = memo.get();
                    }
                    assert_eq!(*got.unwrap(), 212);
                    tx.send(out).unwrap();
                });
            }
            let mut got_one = false;
            for _ in 0..11 {
                let one = rx.recv().unwrap();
                if one {
                    if got_one {
                        panic!();
                    }
                    got_one = true;
                }
            }
            assert!(got_one);
            rx.recv_timeout(Duration::from_millis(500)).unwrap_err();
            assert_eq!(*memo.get().unwrap(), 212);
            assert_eq!(times.load(Ordering::SeqCst), 2);
        }

        #[test]
        #[allow(unused_must_use)]
        fn unpoison_with_value() {
            let memo = ThreadsafeMemo::new(|| {
                panic!();
            });
            assert!(!memo.unpoison_with_value(0));
            panic::catch_unwind(|| {
                memo.get();
            }).unwrap_err();
            memo.get().unwrap_err();
            assert!(memo.unpoison_with_value(212));
            assert_eq!(*memo.get().unwrap(), 212);
        }

        #[test]
        fn unpoison_with_value_race() {
            let (tx, rx) = channel();
            let memo = Arc::new(ThreadsafeMemo::new(|| {
                panic!();
            }));
            for i in 0..12 {
                let tx = tx.clone();
                let memo = memo.clone();
                thread::spawn(move || {
                    if i >= 6 {
                        for _ in 0..6 {
                            thread::yield_now();
                        }
                    }
                    let mut got = memo.get();
                    let mut out = false;
                    if got.is_err() {
                        out = memo.unpoison_with_value(212);
                        got = memo.get();
                    }
                    assert_eq!(*got.unwrap(), 212);
                    tx.send(out).unwrap();
                });
            }
            let mut got_one = false;
            for _ in 0..11 {
                let one = rx.recv().unwrap();
                if one {
                    if got_one {
                        panic!();
                    }
                    got_one = true;
                }
            }
            assert!(got_one);
            rx.recv_timeout(Duration::from_millis(500)).unwrap_err();
            assert_eq!(*memo.get().unwrap(), 212);
        }

        #[test]
        fn unpoison_and_unpoison_with_value_race() {
            let (tx, rx) = channel();
            let times = Arc::new(AtomicUsize::new(0));
            let memo = Arc::new(ThreadsafeMemo::new(PoisonCallback {
                times: times.clone(),
                panic: true,
                value: 0,
            }));
            for i in 0..12 {
                let tx = tx.clone();
                let memo = memo.clone();
                let times = times.clone();
                thread::spawn(move || {
                    if i >= 6 {
                        for _ in 0..6 {
                            thread::yield_now();
                        }
                    }
                    let mut got = memo.get();
                    let mut out = false;
                    if got.is_err() {
                        if i & 1 == 0 {
                            out = memo.unpoison_with_value(212);
                        } else {
                            out = memo.unpoison(PoisonCallback {
                                times: times.clone(),
                                panic: false,
                                value: 212,
                            });
                        }
                        got = memo.get();
                    }
                    assert_eq!(*got.unwrap(), 212);
                    tx.send(out).unwrap();
                });
            }
            let mut got_one = false;
            for _ in 0..11 {
                let one = rx.recv().unwrap();
                if one {
                    if got_one {
                        panic!();
                    }
                    got_one = true;
                }
            }
            assert!(got_one);
            rx.recv_timeout(Duration::from_millis(500)).unwrap_err();
            assert_eq!(*memo.get().unwrap(), 212);
        }
    }
}
