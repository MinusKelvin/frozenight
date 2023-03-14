#[cfg(feature = "tweakable")]
mod imp {
    use std::sync::atomic::{AtomicI16, Ordering};

    pub struct Parameter {
        name: &'static str,
        pub min: i16,
        pub max: i16,
        pub default: i16,
        value: AtomicI16,
    }

    impl Parameter {
        pub const fn new(name: &'static str, min: i16, max: i16, default: i16) -> Self {
            Parameter {
                name,
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
            self.name.to_string()
        }
    }
}

#[cfg(not(feature = "tweakable"))]
mod imp {
    pub struct Parameter {
        value: i16,
    }

    impl Parameter {
        pub const fn new(_: &'static str, _: i16, _: i16, default: i16) -> Self {
            Parameter { value: default }
        }

        #[inline(always)]
        pub fn get(&self) -> i16 {
            self.value
        }
    }
}

macro_rules! tweakables {
    (@values $name:ident: $min:literal ..= $max:literal = $default:expr; $($rest:tt)*) => {
        pub static $name: imp::Parameter = imp::Parameter::new(
            stringify!($name), $min, $max, $default
        );
        tweakables!(@values $($rest)*);
    };
    (@values) => {};

    (@list $iter:ident $name:ident: $min:literal ..= $max:literal = $default:expr; $($rest:tt)*) => {
        {
            let iter = $iter.chain(std::iter::once(&$name));
            tweakables!(@list iter $($rest)*)
        }
    };
    (@list $iter:ident) => { $iter };

    (@$case:ident $($err:tt)*) => {
        compile_error!(concat!("unexpected trailing characters", stringify!($($err)*)));
    };
    ($($rest:tt)*) => {
        tweakables!(@values $($rest)*);
        pub fn all_parameters() -> impl Iterator<Item=&'static imp::Parameter> {
            let iter = std::iter::empty();
            tweakables!(@list iter $($rest)*)
        }
    };
}

tweakables! {
    NMP_MIN_DEPTH: 1..=20 = 1;
    NMP_DEPTH_FACTOR: 0..=1000 = 333;
    NMP_BASE_REDUCTION: 0..=20 = 1;

    LMR_MOVE_FACTOR: 0..=2000 = 100;
    LMR_DEPTH_FACTOR: 0..=2000 = 70;

    RFP_MAX_DEPTH: 0..=10 = 3;
    RFP_MARGIN: 1..=1000 = 350;

    DELTA_PRUNING_MARGIN: 0..=10000 = 1000;
}

pub fn fp_mul(a: i16, b: i16) -> i16 {
    (a as i32 * b as i32 / 1000) as i16
}

pub fn base_lmr(i: usize, depth: i16) -> i16 {
    let base =
        i as i32 * LMR_MOVE_FACTOR.get() as i32 + depth as i32 * LMR_DEPTH_FACTOR.get() as i32;
    (base / 1000) as i16
}
