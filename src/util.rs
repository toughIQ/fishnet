pub trait WhateverExt: Sized {
    fn whatever(self, msg: &str) {}
}

impl<T, E> WhateverExt for Result<T, E> {}
