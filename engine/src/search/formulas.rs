use crate::eval::Eval;

use super::window::Window;

pub const LMR_MIN_DEPTH: u8 = 3;

pub fn nmp_calculate_reduction(static_eval: Eval, window: Window) -> u8 {
    let mut reduction = 3;
    if let (Some(eval), Some(beta)) = (static_eval.as_cp(), window.beta.as_cp()) {
        if eval >= beta {
            // CITE: This kind of reduction increase when eval >= beta was first observed in MadChess.
            // https://www.madchess.net/2021/02/09/madchess-3-0-beta-f231dac-pvs-and-null-move-improvements/
            reduction += ((eval as i32 - beta as i32) / 100).min(2) as u8;
        }
    }
    reduction
}

pub fn lmr_calculate_reduction(i: usize, depth: u8, history: i32) -> u8 {
    let mut reduction: i8 = if i < 3 {
        0
    } else if depth < 7 {
        1
    } else {
        2
    };
    reduction -= (history / 200) as i8;
    reduction.max(0) as u8
}

pub fn lmp_quiets_to_check(depth: u8) -> usize {
    match depth {
        1 => 5,
        2 => 10,
        3 => 15,
        _ => usize::MAX
    }
}

pub fn futility_margin(depth: u8) -> Option<Eval> {
    Some(Eval::cp(match depth {
        1 => 300,
        2 => 600,
        _ => return None
    }))
}

pub fn reverse_futility_margin(depth: u8) -> Option<Eval> {
    if depth < 5 {
        Some(Eval::cp(100 * depth as i16))
    } else {
        None
    }
}
