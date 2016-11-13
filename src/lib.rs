#![feature(atomic_access)]
#![feature(fn_traits)]
#![feature(unboxed_closures)]

mod memo;
mod aliasable_memo;
mod threadsafe_memo;

pub use memo::Memo;
pub use aliasable_memo::AliasableMemo;
pub use threadsafe_memo::ThreadsafeMemo;
