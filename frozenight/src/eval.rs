use std::cmp::Ordering;

use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable, Default)]
#[repr(transparent)]
pub struct Eval(i16);

#[derive(Clone, Copy, Debug, Eq)]
pub enum InternalEval {
    Real(Eval),
    Repetition(u64),
    Fake(Eval),
}

impl Eval {
    pub const MATE: Eval = Eval(300_00);
    pub const TB_WIN: Eval = Eval(250_00);
    pub const MAX_INCONCLUSIVE: Eval = Eval(200_00);
    pub const DRAW: Eval = Eval(0);

    pub fn new(value: i16) -> Self {
        Eval(value).clamp(-Eval::MAX_INCONCLUSIVE, Eval::MAX_INCONCLUSIVE)
    }

    pub fn is_conclusive(self) -> bool {
        self.plys_to_conclusion().is_some()
    }

    /// Returns `p` if winning or `-p` if losing, where `p` is the number of plys until conclusion.
    ///
    /// ```
    /// use frozenight::Eval;
    ///
    /// assert_eq!(Eval::MATE.plys_to_conclusion(), Some(0));
    /// assert_eq!(Eval::MATE.add_time(4).plys_to_conclusion(), Some(4));
    /// assert_eq!((-Eval::MATE).add_time(4).plys_to_conclusion(), Some(-4));
    ///
    /// assert_eq!(Eval::TB_WIN.plys_to_conclusion(), Some(0));
    /// assert_eq!(Eval::TB_WIN.add_time(4).plys_to_conclusion(), Some(4));
    /// assert_eq!((-Eval::TB_WIN).add_time(4).plys_to_conclusion(), Some(-4));
    /// ```
    pub fn plys_to_conclusion(self) -> Option<i16> {
        if self > Eval::TB_WIN {
            Some(Self::MATE.0 - self.0)
        } else if self > Eval::MAX_INCONCLUSIVE {
            Some(Self::TB_WIN.0 - self.0)
        } else if self < -Eval::TB_WIN {
            Some(-Self::MATE.0 - self.0)
        } else if self < -Eval::MAX_INCONCLUSIVE {
            Some(-Self::TB_WIN.0 - self.0)
        } else {
            None
        }
    }

    /// If this eval is conclusive, decreases the score by the indicated number of plys.
    pub fn add_time(self, plys: u16) -> Self {
        if self < -Self::MAX_INCONCLUSIVE {
            Eval(self.0 + plys as i16)
        } else if self > Self::MAX_INCONCLUSIVE {
            Eval(self.0 - plys as i16)
        } else {
            self
        }
    }

    /// If this eval is conclusive, increases the score by the indicated number of plys.
    pub fn sub_time(self, plys: u16) -> Self {
        if self < -Self::MAX_INCONCLUSIVE {
            debug_assert!(self.0 - plys as i16 > -Eval::MATE.0);
            Eval(self.0 - plys as i16)
        } else if self > Self::MAX_INCONCLUSIVE {
            debug_assert!(self.0 + (plys as i16) < Eval::MATE.0);
            Eval(self.0 + plys as i16)
        } else {
            self
        }
    }

    pub fn raw(self) -> i16 {
        self.0
    }
}

impl std::ops::Neg for Eval {
    type Output = Self;

    fn neg(self) -> Self {
        Eval(-self.0)
    }
}

impl std::ops::Add<i16> for Eval {
    type Output = Eval;

    fn add(self, rhs: i16) -> Self::Output {
        Eval::new(self.0 + rhs)
    }
}

impl std::ops::Sub<i16> for Eval {
    type Output = Eval;

    fn sub(self, rhs: i16) -> Self::Output {
        Eval::new(self.0 - rhs)
    }
}

impl std::fmt::Display for Eval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.plys_to_conclusion() {
            Some(plys) => write!(f, "mate {}", (plys + plys.signum()) / 2),
            None => write!(f, "cp {}", self.0 / 5),
        }
    }
}

impl InternalEval {
    pub fn apparent_value(&self) -> Eval {
        match *self {
            InternalEval::Real(v) => v,
            InternalEval::Repetition(_) => Eval::DRAW,
            InternalEval::Fake(v) => v,
        }
    }
}

impl From<Eval> for InternalEval {
    fn from(v: Eval) -> Self {
        InternalEval::Real(v)
    }
}

impl PartialEq for InternalEval {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl PartialOrd for InternalEval {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InternalEval {
    fn cmp(&self, rhs: &Self) -> Ordering {
        self.apparent_value()
            .cmp(&rhs.apparent_value())
            .then_with(|| {
                let l = match self {
                    InternalEval::Real(_) => 0,
                    InternalEval::Repetition(_) => 1,
                    InternalEval::Fake(_) => 2,
                };
                let r = match rhs {
                    InternalEval::Real(_) => 0,
                    InternalEval::Repetition(_) => 1,
                    InternalEval::Fake(_) => 2,
                };
                l.cmp(&r)
            })
    }
}

impl std::ops::Neg for InternalEval {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            InternalEval::Real(v) => InternalEval::Real(-v),
            InternalEval::Repetition(v) => InternalEval::Repetition(v),
            InternalEval::Fake(v) => InternalEval::Fake(-v),
        }
    }
}
