use super::EvalTrace;

pub trait TraceTarget {
    fn trace(&mut self, term: impl FnMut(&mut EvalTrace));
}

impl TraceTarget for EvalTrace {
    fn trace(&mut self, mut term: impl FnMut(&mut EvalTrace)) {
        term(self);
    }
}

impl TraceTarget for () {
    fn trace(&mut self, _: impl FnMut(&mut EvalTrace)) {
    }
}
