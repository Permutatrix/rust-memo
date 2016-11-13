use std::cell::{Cell, UnsafeCell};
use memo::Memo;

pub struct AliasableMemo<T, F: FnOnce() -> T> {
    calculating: Cell<bool>,
    memo: UnsafeCell<Memo<T, F>>,
}

impl<T, F: FnOnce() -> T> AliasableMemo<T, F> {
    pub fn new(func: F) -> AliasableMemo<T, F> {
        AliasableMemo {
            calculating: Cell::new(false),
            memo: UnsafeCell::new(Memo::new(func)),
        }
    }

    pub fn with_value(value: T) -> AliasableMemo<T, F> {
        AliasableMemo {
            calculating: Cell::new(false),
            memo: UnsafeCell::new(Memo::with_value(value)),
        }
    }
}

impl<'a, T, F: FnOnce() -> T> AliasableMemo<T, F> {
    fn forbid_calculating(&self) {
        if self.calculating.get() {
            panic!("AliasableMemo's callback tried to access its own result!");
        }
    }

    pub fn get(&self) -> &T {
        self.forbid_calculating();
        self.calculating.set(true);
        let out = unsafe { (*self.memo.get()).get() };
        self.calculating.set(false);
        out
    }

    pub fn try_get(&self) -> Option<&T> {
        self.forbid_calculating();
        unsafe { (*self.memo.get()).try_get() }
    }

    pub fn take(self) -> T {
        unsafe { self.memo.into_inner().take() }
    }

    pub fn try_take(self) -> Option<T> {
        unsafe { self.memo.into_inner().try_take() }
    }
}

#[cfg(test)]
#[allow(unused_assignments)]
mod tests {
    mod new {
        use super::super::AliasableMemo;

        #[test]
        fn get() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212
                });
                assert_eq!(*memo.get(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn try_get() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212
                });
                assert!(memo.try_get().is_none());
            }
            assert_eq!(times, 0);
        }

        #[test]
        fn take() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212
                });
                assert_eq!(memo.take(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn try_take() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212
                });
                assert!(memo.try_take().is_none());
            }
            assert_eq!(times, 0);
        }

        #[test]
        fn get_get() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get(), 212);
                assert_eq!(*memo.get(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn get_try_get() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get(), 212);
                assert_eq!(*memo.try_get().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn get_take() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get(), 212);
                assert_eq!(memo.take(), 212);
            }
            assert_eq!(times, 1);
        }

        #[test]
        fn get_try_take() {
            let mut times = 0;
            {
                let memo = AliasableMemo::new(|| {
                    times += 1;
                    212 + times - 1
                });
                assert_eq!(*memo.get(), 212);
                assert_eq!(memo.try_take().unwrap(), 212);
            }
            assert_eq!(times, 1);
        }
    }

    mod with_value {
        use super::super::AliasableMemo;

        #[test]
        fn get() {
            let mut memo = AliasableMemo::new(|| { 200 });
            memo = AliasableMemo::with_value(212);
            assert_eq!(*memo.get(), 212);
        }

        #[test]
        fn try_get() {
            let mut memo = AliasableMemo::new(|| { 200 });
            memo = AliasableMemo::with_value(212);
            assert_eq!(*memo.try_get().unwrap(), 212);
        }

        #[test]
        fn take() {
            let mut memo = AliasableMemo::new(|| { 200 });
            memo = AliasableMemo::with_value(212);
            assert_eq!(memo.take(), 212);
        }

        #[test]
        fn try_take() {
            let mut memo = AliasableMemo::new(|| { 200 });
            memo = AliasableMemo::with_value(212);
            assert_eq!(memo.try_take().unwrap(), 212);
        }
    }
}
