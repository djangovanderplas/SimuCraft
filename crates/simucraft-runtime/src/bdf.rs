//! diffsol behind the [`Solver`] trait (ADR 0001). All diffsol calls stay in
//! this module, because diffsol is pre-1.0 and its API can change.

use std::cell::RefCell;

use diffsol::{NalgebraLU, NalgebraMat, OdeBuilder, OdeSolverMethod, OdeSolverStopReason, Vector};
use simucraft_contracts::Diagnostic;

use crate::codes;
use crate::solver::{Rhs, Solver};

/// diffsol's variable-order BDF method, for stiff models. The Jacobian comes
/// from finite differences of the model (ADR 0001, third source).
pub struct Bdf {
    /// Relative tolerance.
    pub rtol: f64,
    /// Absolute tolerance.
    pub atol: f64,
}

impl Solver for Bdf {
    fn advance(&mut self, f: &mut Rhs, t: f64, x: &mut [f64], t_end: f64) -> Result<(), Diagnostic> {
        // diffsol takes `Fn` closures; the model's right-hand side is `FnMut`.
        let f = RefCell::new(f);
        let fail = |e: diffsol::error::DiffsolError| Diagnostic::error(codes::STEP, format!("BDF at t = {t}: {e}"));
        let x0 = x.to_vec();
        // shortcut: a new problem for every major step restarts BDF at order 1; keep one problem
        // across steps when profiles show the cost.
        let problem = OdeBuilder::<NalgebraMat<f64>>::new()
            .t0(t)
            .rtol(self.rtol)
            .atol(vec![self.atol; x.len()])
            .rhs_implicit(
                |x, _p, t, y| (f.borrow_mut())(t, x, y),
                |x, _p, t, v, y| {
                    // Central directional difference: J v = (f(x + h v) - f(x - h v)) / 2h.
                    // A one-sided difference fails on Robertson's quadratic stiff term.
                    let norm = v.iter().map(|v| v * v).sum::<f64>().sqrt();
                    if norm == 0.0 {
                        y.fill(0.0);
                        return;
                    }
                    let scale = x.iter().map(|x| x * x).sum::<f64>().sqrt().max(1.0);
                    let h = f64::EPSILON.cbrt() * scale / norm;
                    let at = |sign: f64| x.iter().zip(v).map(|(x, v)| x + sign * h * v).collect::<Vec<f64>>();
                    let mut back = vec![0.0; x.len()];
                    let mut f = f.borrow_mut();
                    f(t, &at(-1.0), &mut back);
                    f(t, &at(1.0), y);
                    y.iter_mut().zip(&back).for_each(|(y, b)| *y = (*y - b) / (2.0 * h));
                },
            )
            .init(move |_p, _t, y| y.copy_from_slice(&x0), x.len())
            .build()
            .map_err(fail)?;
        let mut solver = problem.bdf::<NalgebraLU<f64>>().map_err(fail)?;
        solver.set_stop_time(t_end).map_err(fail)?;
        while !matches!(solver.step().map_err(fail)?, OdeSolverStopReason::TstopReached) {}
        x.copy_from_slice(&solver.state().y.clone_as_vec());
        Ok(())
    }
}
