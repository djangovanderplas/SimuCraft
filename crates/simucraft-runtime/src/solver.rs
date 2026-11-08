//! Solvers behind one trait. The explicit Runge-Kutta methods share one
//! Butcher-tableau stepper.

use simucraft_contracts::Diagnostic;

/// The right-hand side `dx = f(t, x)`.
pub type Rhs<'a> = dyn FnMut(f64, &[f64], &mut [f64]) + 'a;

/// Integrates `x' = f(t, x)`. The runtime calls `advance` once per major
/// step, so a solver never steps over a sample hit or an output time.
pub trait Solver {
    /// Advance `x` from `t` to exactly `t_end`.
    fn advance(&mut self, f: &mut Rhs, t: f64, x: &mut [f64], t_end: f64) -> Result<(), Diagnostic>;
}

struct Tableau {
    c: &'static [f64],
    a: &'static [&'static [f64]],
    /// Weights of the solution.
    b: &'static [f64],
    /// Weights of the error estimate (`b` minus the embedded weights); empty
    /// for fixed-step methods.
    e: &'static [f64],
}

const EULER: Tableau = Tableau {
    c: &[0.0],
    a: &[&[]],
    b: &[1.0],
    e: &[],
};

const RK4: Tableau = Tableau {
    c: &[0.0, 0.5, 0.5, 1.0],
    a: &[&[], &[0.5], &[0.0, 0.5], &[0.0, 0.0, 1.0]],
    b: &[1.0 / 6.0, 1.0 / 3.0, 1.0 / 3.0, 1.0 / 6.0],
    e: &[],
};

/// Dormand-Prince 5(4) (Hairer, Norsett and Wanner, Table 5.2).
const DOPRI5: Tableau = Tableau {
    c: &[0.0, 0.2, 0.3, 0.8, 8.0 / 9.0, 1.0, 1.0],
    a: &[
        &[],
        &[0.2],
        &[3.0 / 40.0, 9.0 / 40.0],
        &[44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0],
        &[19372.0 / 6561.0, -25360.0 / 2187.0, 64448.0 / 6561.0, -212.0 / 729.0],
        &[
            9017.0 / 3168.0,
            -355.0 / 33.0,
            46732.0 / 5247.0,
            49.0 / 176.0,
            -5103.0 / 18656.0,
        ],
        &[
            35.0 / 384.0,
            0.0,
            500.0 / 1113.0,
            125.0 / 192.0,
            -2187.0 / 6784.0,
            11.0 / 84.0,
        ],
    ],
    b: &[
        35.0 / 384.0,
        0.0,
        500.0 / 1113.0,
        125.0 / 192.0,
        -2187.0 / 6784.0,
        11.0 / 84.0,
        0.0,
    ],
    e: &[
        71.0 / 57600.0,
        0.0,
        -71.0 / 16695.0,
        71.0 / 1920.0,
        -17253.0 / 339200.0,
        22.0 / 525.0,
        -1.0 / 40.0,
    ],
};

/// Stage derivatives for one step of size `h`. `k` has one vector per stage.
fn stages(f: &mut Rhs, tab: &Tableau, t: f64, x: &[f64], h: f64, k: &mut [Vec<f64>], tmp: &mut [f64]) {
    for i in 0..tab.c.len() {
        let (done, rest) = k.split_at_mut(i);
        for (n, v) in tmp.iter_mut().enumerate() {
            *v = x[n] + h * tab.a[i].iter().zip(done.iter()).map(|(a, k)| a * k[n]).sum::<f64>();
        }
        f(t + tab.c[i] * h, tmp, &mut rest[0]);
    }
}

/// `x += h * sum(w_i k_i)`.
fn combine(x: &mut [f64], h: f64, w: &[f64], k: &[Vec<f64>]) {
    for (n, v) in x.iter_mut().enumerate() {
        *v += h * w.iter().zip(k).map(|(w, k)| w * k[n]).sum::<f64>();
    }
}

fn buffers(tab: &Tableau, n: usize) -> (Vec<Vec<f64>>, Vec<f64>) {
    (vec![vec![0.0; n]; tab.c.len()], vec![0.0; n])
}

/// Fixed step: `n` equal steps that land on `t_end`, each at most `h` long.
fn fixed(tab: &Tableau, h: f64, f: &mut Rhs, t: f64, x: &mut [f64], t_end: f64) {
    let n = ((t_end - t) / h - 1e-9).ceil().max(1.0);
    let h = (t_end - t) / n;
    let (mut k, mut tmp) = buffers(tab, x.len());
    for i in 0..n as usize {
        stages(f, tab, t + i as f64 * h, x, h, &mut k, &mut tmp);
        combine(x, h, tab.b, &k);
    }
}

/// Forward Euler with a fixed step `h`.
pub struct Euler {
    /// Largest step size.
    pub h: f64,
}

impl Solver for Euler {
    fn advance(&mut self, f: &mut Rhs, t: f64, x: &mut [f64], t_end: f64) -> Result<(), Diagnostic> {
        fixed(&EULER, self.h, f, t, x, t_end);
        Ok(())
    }
}

/// Classic fourth-order Runge-Kutta with a fixed step `h`.
pub struct Rk4 {
    /// Largest step size.
    pub h: f64,
}

impl Solver for Rk4 {
    fn advance(&mut self, f: &mut Rhs, t: f64, x: &mut [f64], t_end: f64) -> Result<(), Diagnostic> {
        fixed(&RK4, self.h, f, t, x, t_end);
        Ok(())
    }
}

/// Dormand-Prince 5(4) with error control: a step is accepted when the RMS
/// of `err_i / (atol + rtol * |x_i|)` is at most 1.
pub struct DormandPrince {
    /// Relative tolerance.
    pub rtol: f64,
    /// Absolute tolerance.
    pub atol: f64,
    /// Step size for the next step; `None` until the first step.
    h: Option<f64>,
}

impl DormandPrince {
    /// A solver with the given tolerances.
    pub fn new(rtol: f64, atol: f64) -> Self {
        Self { rtol, atol, h: None }
    }
}

impl Solver for DormandPrince {
    fn advance(&mut self, f: &mut Rhs, mut t: f64, x: &mut [f64], t_end: f64) -> Result<(), Diagnostic> {
        let (mut k, mut tmp) = buffers(&DOPRI5, x.len());
        let mut h = self.h.unwrap_or(0.01 * (t_end - t));
        while t < t_end {
            let last = t + h >= t_end;
            let step = if last { t_end - t } else { h };
            if step <= 1e-14 * t.abs().max(1.0) {
                return Err(Diagnostic::error(
                    "R003",
                    format!("Dormand-Prince step size underflow at t = {t}; the model may be stiff or singular"),
                ));
            }
            // shortcut: no first-same-as-last reuse of the last stage; costs one RHS call per step.
            stages(f, &DOPRI5, t, x, step, &mut k, &mut tmp);
            let mut new = x.to_vec();
            combine(&mut new, step, DOPRI5.b, &k);
            let mut est = vec![0.0; x.len()];
            combine(&mut est, step, DOPRI5.e, &k);
            let err = if x.is_empty() {
                0.0
            } else {
                let sum: f64 = (0..x.len())
                    .map(|i| (est[i] / (self.atol + self.rtol * x[i].abs().max(new[i].abs()))).powi(2))
                    .sum();
                (sum / x.len() as f64).sqrt()
            };
            let factor = match err {
                0.0 => 5.0,
                e if e.is_finite() => (0.9 * e.powf(-0.2)).clamp(0.2, 5.0),
                _ => 0.2, // NaN or infinite: shrink until the step underflows
            };
            if err <= 1.0 {
                t = if last { t_end } else { t + step };
                x.copy_from_slice(&new);
                // A step cut short to land on t_end says nothing about the next step size.
                h = if last { h.max(step * factor) } else { step * factor };
            } else {
                h = step * factor;
            }
        }
        self.h = Some(h);
        Ok(())
    }
}
