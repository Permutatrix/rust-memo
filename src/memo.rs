pub struct Memo<T, F: FnOnce() -> T> {
    func: Option<F>,
    value: Option<T>,
}

impl<T, F: FnOnce() -> T> Memo<T, F> {
    pub fn new(func: F) -> Memo<T, F> {
        Memo {
            func: Some(func),
            value: None,
        }
    }

    pub fn with_value(value: T) -> Memo<T, F> {
        Memo {
            func: None,
            value: Some(value),
        }
    }
}

impl<'a, T, F: FnOnce() -> T> Memo<T, F> {
    pub fn get(&mut self) -> &T {
        if let Some(func) = self.func.take() {
            self.value = Some(func());
        }
        self.value.as_ref().unwrap()
    }

    pub fn try_get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    pub fn take(self) -> T {
        match self {
            Memo { func: Some(func), value: None } => func(),
            Memo { func: None, value: Some(value) } => value,
            _ => panic!("Memo had an invalid state!")
        }
    }

    pub fn try_take(self) -> Option<T> {
        self.value
    }
}

#[cfg(test)]
#[allow(unused_assignments)]
mod tests {
    mod new {
        use super::super::Memo;

        #[test]
        fn get() {
            let mut times = 0;
            {
                let mut memo = Memo::new(|| {
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
                let memo = Memo::new(|| {
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
                let memo = Memo::new(|| {
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
                let memo = Memo::new(|| {
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
                let mut memo = Memo::new(|| {
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
                let mut memo = Memo::new(|| {
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
                let mut memo = Memo::new(|| {
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
                let mut memo = Memo::new(|| {
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
        use super::super::Memo;

        #[test]
        fn get() {
            let mut memo = Memo::new(|| { 200 });
            memo = Memo::with_value(212);
            assert_eq!(*memo.get(), 212);
        }

        #[test]
        fn try_get() {
            let mut memo = Memo::new(|| { 200 });
            memo = Memo::with_value(212);
            assert_eq!(*memo.try_get().unwrap(), 212);
        }

        #[test]
        fn take() {
            let mut memo = Memo::new(|| { 200 });
            memo = Memo::with_value(212);
            assert_eq!(memo.take(), 212);
        }

        #[test]
        fn try_take() {
            let mut memo = Memo::new(|| { 200 });
            memo = Memo::with_value(212);
            assert_eq!(memo.try_take().unwrap(), 212);
        }
    }
}
