//! ADR 0001 spike: is diffsol good enough to be the default solver backend?
//! Run: cargo run --release

#[cfg(feature = "sundials")]
mod cvode;

use std::time::Instant;

use diffsol::{NalgebraLU, NalgebraMat, OdeBuilder, OdeSolverMethod, OdeSolverStopReason, Vector};

type M = NalgebraMat<f64>;
type LS = NalgebraLU<f64>;

/// Reference values at t = 4e10 (Hairer & Wanner; also diffsol's test table).
const ROBER_REF: [f64; 3] = [4.8641e-08, 1.9456e-13, 1.0];

fn rel_err(got: &[f64], want: &[f64], atol: &[f64]) -> f64 {
    got.iter().zip(want).zip(atol).map(|((g, w), a)| (g - w).abs() / (a + 1e-4 * w.abs())).fold(0.0, f64::max)
}

fn time<T>(reps: u32, mut f: impl FnMut() -> T) -> (T, f64) {
    let mut out = f();
    let start = Instant::now();
    for _ in 0..reps {
        out = f();
    }
    (out, start.elapsed().as_secs_f64() * 1e6 / reps as f64)
}

fn robertson_ode() -> Vec<f64> {
    let p = OdeBuilder::<M>::new()
        .p([0.04, 1.0e4, 3.0e7])
        .rtol(1e-4)
        .atol([1.0e-8, 1.0e-6, 1.0e-6])
        .rhs_implicit(
            |x, p, _t, y| {
                y[0] = -p[0] * x[0] + p[1] * x[1] * x[2];
                y[1] = p[0] * x[0] - p[1] * x[1] * x[2] - p[2] * x[1] * x[1];
                y[2] = p[2] * x[1] * x[1];
            },
            |x, p, _t, v, y| {
                y[0] = -p[0] * v[0] + p[1] * (v[1] * x[2] + x[1] * v[2]);
                y[1] = p[0] * v[0] - p[1] * (v[1] * x[2] + x[1] * v[2]) - 2.0 * p[2] * x[1] * v[1];
                y[2] = 2.0 * p[2] * x[1] * v[1];
            },
        )
        .init(|_p, _t, y| y.copy_from_slice(&[1.0, 0.0, 0.0]), 3)
        .build()
        .unwrap();
    let mut s = p.bdf::<LS>().unwrap();
    s.set_stop_time(4e10).unwrap();
    while !matches!(s.step().unwrap(), OdeSolverStopReason::TstopReached) {}
    s.state().y.clone_as_vec()
}

/// Same chemistry, written as a DAE: the third equation is algebraic (M singular).
fn robertson_dae() -> Vec<f64> {
    let p = OdeBuilder::<M>::new()
        .p([0.04, 1.0e4, 3.0e7])
        .rtol(1e-4)
        .atol([1.0e-8, 1.0e-6, 1.0e-6])
        .rhs_implicit(
            |x, p, _t, y| {
                y[0] = -p[0] * x[0] + p[1] * x[1] * x[2];
                y[1] = p[0] * x[0] - p[1] * x[1] * x[2] - p[2] * x[1] * x[1];
                y[2] = x[0] + x[1] + x[2] - 1.0;
            },
            |x, p, _t, v, y| {
                y[0] = -p[0] * v[0] + p[1] * (v[1] * x[2] + x[1] * v[2]);
                y[1] = p[0] * v[0] - p[1] * (v[1] * x[2] + x[1] * v[2]) - 2.0 * p[2] * x[1] * v[1];
                y[2] = v[0] + v[1] + v[2];
            },
        )
        .mass(|v, _p, _t, beta, y| {
            y[0] = v[0] + beta * y[0];
            y[1] = v[1] + beta * y[1];
            y[2] *= beta;
        })
        .init(|_p, _t, y| y.copy_from_slice(&[1.0, 0.0, 0.0]), 3)
        .build()
        .unwrap();
    let mut s = p.bdf::<LS>().unwrap();
    s.set_stop_time(4e10).unwrap();
    while !matches!(s.step().unwrap(), OdeSolverStopReason::TstopReached) {}
    s.state().y.clone_as_vec()
}

/// Ball dropped from 10 m: the root (height = 0) must be located at sqrt(2h/g).
fn bouncing_ball_event() -> f64 {
    let p = OdeBuilder::<M>::new()
        .rtol(1e-8)
        .atol([1e-10, 1e-10])
        .rhs_implicit(
            |x, _p, _t, y| {
                y[0] = x[1];
                y[1] = -9.81;
            },
            |_x, _p, _t, v, y| {
                y[0] = v[1];
                y[1] = 0.0;
            },
        )
        .root(|x, _p, _t, y| y[0] = x[0], 1)
        .init(|_p, _t, y| y.copy_from_slice(&[10.0, 0.0]), 2)
        .build()
        .unwrap();
    let mut s = p.tsit45().unwrap();
    loop {
        if let OdeSolverStopReason::RootFound(t, _) = s.step().unwrap() {
            return t;
        }
    }
}

fn main() {
    let atol = [1e-8, 1e-6, 1e-6];
    let (y, us) = time(20, robertson_ode);
    println!("robertson ODE  BDF   {us:9.1} us   err/tol {:.2}   y = {}", rel_err(&y, &ROBER_REF, &atol), fmt(&y));
    #[cfg(feature = "sundials")]
    {
        let (y, us) = time(20, cvode::robertson_ode);
        println!("robertson ODE  CVODE {us:9.1} us   err/tol {:.2}   y = {}", rel_err(&y, &ROBER_REF, &atol), fmt(&y));
    }
    let (y, us) = time(20, robertson_dae);
    println!("robertson DAE  BDF   {us:9.1} us   err/tol {:.2}   y = {}", rel_err(&y, &ROBER_REF, &atol), fmt(&y));
    let (t, us) = time(100, bouncing_ball_event);
    let exact = (2.0f64 * 10.0 / 9.81).sqrt();
    println!("ball event     TSIT45 {us:8.1} us   t = {t:.10}  exact {exact:.10}  |dt| {:.1e}", (t - exact).abs());
}

fn fmt(y: &[f64]) -> String {
    y.iter().map(|v| format!("{v:.4e}")).collect::<Vec<_>>().join(", ")
}
