#![feature(atomic_access)]

mod memo;
mod threadsafe_memo;

pub use memo::Memo;
pub use threadsafe_memo::ThreadsafeMemo;
