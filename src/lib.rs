#![cfg_attr(feature = "unstable", feature(atomic_access))]
#![cfg_attr(test, feature(fn_traits, unboxed_closures))]

mod memo;
mod aliasable_memo;
mod threadsafe_memo;

pub use memo::Memo;
pub use aliasable_memo::AliasableMemo;
pub use threadsafe_memo::ThreadsafeMemo;
