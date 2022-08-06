#[cfg(feature = "tweakable")]
mod params {
    use std::sync::atomic::{AtomicI16, Ordering};

    pub struct Parameter {
        name: &'static str,
        array_elem: Option<usize>,
        pub min: i16,
        pub max: i16,
        pub default: i16,
        value: AtomicI16,
    }

    impl Parameter {
        pub const fn new(
            name: &'static str,
            array_elem: Option<usize>,
            min: i16,
            max: i16,
            default: i16,
        ) -> Self {
            Parameter {
                name,
                array_elem,
                min,
                max,
                default,
                value: AtomicI16::new(default),
            }
        }

        #[inline(always)]
        pub fn get(&self) -> i16 {
            self.value.load(Ordering::Relaxed)
        }

        pub fn set(&self, v: i16) {
            self.value.store(v, Ordering::Relaxed);
        }

        pub fn name(&self) -> String {
            match self.array_elem {
                Some(index) => format!("{}[{}]", self.name, index),
                None => self.name.to_string(),
            }
        }
    }
}

#[cfg(not(feature = "tweakable"))]
mod params {
    pub struct Parameter {
        value: i16,
    }

    impl Parameter {
        pub const fn new(_: &'static str, _: Option<usize>, _: i16, _: i16, default: i16) -> Self {
            Parameter { value: default }
        }

        #[inline(always)]
        pub fn get(&self) -> i16 {
            self.value
        }
    }
}

macro_rules! params {
    (@values $name:ident: $min:tt ..= $max:tt = $default:expr; $($rest:tt)*) => {
        pub static $name: params::Parameter = params::Parameter::new(
            stringify!($name), None, $min, $max, $default
        );
        params!(@values $($rest)*);
    };
    (@values $name:ident[$len:expr]: $min:tt ..= $max:tt = [$($default:expr),*]; $($rest:tt)*) => {
        #[allow(unused_assignments)]
        pub static $name: [params::Parameter; $len] = {
            let mut i = 0;
            [$(params::Parameter::new(
                stringify!($name),
                { let r = Some(i); i += 1; r },
                $min, $max, $default
            )),*]
        };
        params!(@values $($rest)*);
    };
    (@values) => {};

    (@list $iter:ident $name:ident: $min:tt ..= $max:tt = $default:expr; $($rest:tt)*) => {
        {
            let iter = $iter.chain(std::iter::once(&$name));
            params!(@list iter $($rest)*)
        }
    };
    (@list $iter:ident $name:ident[$len:expr]: $min:tt ..= $max:tt = [$($default:expr),*]; $($rest:tt)*) => {
        {
            let iter = $iter.chain($name.iter());
            params!(@list iter $($rest)*)
        }
    };
    (@list $iter:ident) => { $iter };

    (@$($err:tt)*) => { compile_error!(concat!("unexpected trailing characters", stringify!($($err)*))); };
    ($($rest:tt)*) => {
        params!(@values $($rest)*);
        pub fn all_parameters() -> impl Iterator<Item=&'static params::Parameter> {
            let iter = std::iter::empty();
            params!(@list iter $($rest)*)
        }
    };
}

params! {
    RFP_MARGINS[8]: 0..=10000 = [250, 500, 750, 1000, 1250, 1500, 1750, 2000];
}
