use crate::Eval;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    lb: Eval,
    ub: Eval,
}

impl Window {
    pub fn test_raise_lb(lb: Eval) -> Self {
        Window { lb, ub: lb + 1 }
    }

    pub fn test_lower_ub(ub: Eval) -> Self {
        Window { lb: ub - 1, ub }
    }

    pub fn lb(&self) -> Eval {
        self.lb
    }

    pub fn ub(&self) -> Eval {
        self.ub
    }

    pub fn fail_low(&self, v: Eval) -> bool {
        v <= self.lb
    }

    pub fn fail_high(&self, v: Eval) -> bool {
        v >= self.ub
    }

    pub fn raise_lb(&mut self, v: Eval) -> bool {
        debug_assert!(v < self.ub);
        let raised = v > self.lb;
        if raised {
            self.lb = v;
        }
        raised
    }

    pub fn lower_ub(&mut self, v: Eval) -> bool {
        debug_assert!(v > self.lb);
        let lowered = v < self.ub;
        if lowered {
            self.ub = v;
        }
        lowered
    }

    pub fn is_null(&self) -> bool {
        self.lb + 1 == self.ub
    }
}

impl Default for Window {
    fn default() -> Self {
        Self {
            lb: -Eval::MATE,
            ub: Eval::MATE,
        }
    }
}

impl std::ops::Neg for Window {
    type Output = Self;

    fn neg(self) -> Self {
        Window {
            lb: -self.ub,
            ub: -self.lb,
        }
    }
}
