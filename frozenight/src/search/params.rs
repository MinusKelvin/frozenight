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
    RFP_MARGIN_M: 0..=5000 = 255;
    RFP_MARGIN_C: 0..=5000 = 11;
    RFP_MAX_DEPTH: 1..=20 = 8;

    NMP_MIN_DEPTH: 1..=20 = 1;
    NMP_REDUCTION_M: 0..=128 = 77;
    NMP_REDUCTION_C: 0..=1024 = 38;

    LMR_I1_M: 0..=256 = 92;
    LMR_I1_C: 0..=1024 = 15;
    LMR_I2_M: 0..=256 = 17;
    LMR_I2_C: 0..=1024 = 8;
    LMR_D_M: 0..=256 = 28;
    LMR_D_C: 0..=1024 = 8;
    PV_LMR_FACTOR: 0..=128 = 74;
}

#[inline(always)]
pub fn rfp_margin(depth: i16) -> i16 {
    RFP_MARGIN_M.get() * depth + RFP_MARGIN_C.get()
}

#[inline(always)]
pub fn nmp_reduction(depth: i16) -> i16 {
    trunc(linear(depth, NMP_REDUCTION_M.get(), NMP_REDUCTION_C.get()))
}

#[inline(always)]
pub fn null_lmr(depth: i16, movenum: usize) -> i16 {
    trunc(raw_lmr(depth, movenum as i16))
}

#[inline(always)]
pub fn pv_lmr(depth: i16, movenum: usize) -> i16 {
    trunc(raw_lmr(depth, movenum as i16) * PV_LMR_FACTOR.get() as i32 / 128)
}

#[inline(always)]
fn raw_lmr(depth: i16, movenum: i16) -> i32 {
    let movenum_effect = linear(movenum, LMR_I2_M.get(), LMR_I2_C.get());
    let depth_effect = linear(depth, LMR_D_M.get(), LMR_D_C.get());
    let movenum_limit = linear(movenum, LMR_I1_M.get(), LMR_I1_C.get());
    movenum_limit.min(movenum_effect + depth_effect)
}

#[inline(always)]
fn linear(x: i16, m: i16, c: i16) -> i32 {
    x as i32 * m as i32 + c as i32
}

fn trunc(v: i32) -> i16 {
    (v / 128) as i16
}
