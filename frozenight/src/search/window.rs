use crate::Eval;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    lb: Eval,
    ub: Eval,
}

impl Window {
    pub fn null(lb: Eval) -> Self {
        Window { lb, ub: lb + 1 }
    }

    pub fn lb(&self) -> Eval {
        self.lb
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

    pub fn new(lb: Eval, ub: Eval) -> Self {
        assert!(lb < ub);
        Window { lb, ub }
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
