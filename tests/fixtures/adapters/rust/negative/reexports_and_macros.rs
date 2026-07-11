pub use std::collections::HashMap;
pub use crate::foo::Bar;

pub(crate) use std::vec::Vec;

macro_rules! my_macro {
    () => {};
}

mod foo {
    struct Bar;
}
