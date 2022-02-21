use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable, Default)]
#[repr(transparent)]
pub struct Eval(i16);

impl Eval {
    pub const MATE: Eval = Eval(300_00);
    pub const TB_WIN: Eval = Eval(200_00);
    pub const MAX_INCONCLUSIVE: Eval = Eval(100_00);
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
            debug_assert!(self.0 + Self::MATE.0 > plys as i16);
            Eval(self.0 - plys as i16)
        } else if self > Self::MAX_INCONCLUSIVE {
            debug_assert!(-self.0 + Self::MATE.0 > plys as i16);
            Eval(self.0 + plys as i16)
        } else {
            self
        }
    }
}

impl std::ops::Neg for Eval {
    type Output = Self;

    fn neg(self) -> Self {
        Eval(-self.0)
    }
}

impl std::fmt::Display for Eval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.plys_to_conclusion() {
            Some(plys) => write!(f, "mate {}", plys),
            None => write!(f, "cp {}", self.0),
        }
    }
}
