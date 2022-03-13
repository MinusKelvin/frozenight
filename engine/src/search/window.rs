use crate::eval::Eval;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub alpha: Eval,
    pub beta: Eval
}

impl Window {
    pub const INFINITY: Window = Window {
        alpha: Eval::MIN,
        beta: Eval::MAX
    };

    pub fn around(eval: Eval, bounds: Eval) -> Self {
        Window {
            alpha: eval.saturating_sub(bounds),
            beta: eval.saturating_add(bounds)
        }
    }

    pub fn narrow_alpha(&mut self, eval: Eval) {
        self.alpha = self.alpha.max(eval);
    }

    pub fn narrow_beta(&mut self, eval: Eval) {
        self.beta = self.beta.min(eval);
    }

    pub fn contains(&self, eval: Eval) -> bool {
        self.alpha < eval && eval < self.beta
    }

    ///Scout window to test for moves that can raise alpha
    pub fn null_window_alpha(&self) -> Self {
        Self {
            alpha: self.alpha,
            beta: self.alpha + Eval::UNIT
        }
    }

    ///Scout window to test for beta cutoffs
    pub fn null_window_beta(&self) -> Self {
        Self {
            alpha: self.beta - Eval::UNIT,
            beta: self.beta
        }
    }

    pub fn empty(self) -> bool {
        self.alpha >= self.beta
    }
}

impl std::ops::Neg for Window {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            alpha: -self.beta,
            beta: -self.alpha
        }
    }
}
