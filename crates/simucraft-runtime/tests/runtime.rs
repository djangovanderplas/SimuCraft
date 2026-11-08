//! WP1.2 done-when: the hand-written mass-spring-damper matches the analytic
//! solution with Dormand-Prince at rtol 1e-6.

use simucraft_contracts::block::{Block, BlockSpec, BlockType, Built, Ctx, Layout, Params, SampleTime};
use simucraft_contracts::fake;
use simucraft_contracts::ir::CompiledModel;
use simucraft_runtime::{Bdf, DormandPrince, Euler, Rk4, Simulation, Solver};

const MSD: &str = include_str!("../../simucraft-contracts/tests/fixtures/mass_spring_damper.json");

/// x'' = -0.5 x' - 4 x, x(0) = 1, x'(0) = 0.
fn msd_exact(t: f64) -> f64 {
    let (decay, wd) = (0.25, (4.0f64 - 0.0625).sqrt());
    (-decay * t).exp() * ((wd * t).cos() + decay / wd * (wd * t).sin())
}

/// Run the fixture to t = 5 and return the largest error of `x` and the sample count.
fn msd_error(solver: &mut dyn Solver, output_step: f64) -> (f64, usize) {
    let model: CompiledModel = serde_json::from_str(MSD).unwrap();
    let mut sim = Simulation::new(&model, &fake::library()).unwrap();
    let traces = sim.run(solver, 5.0, output_step).unwrap();
    let err = traces
        .t
        .iter()
        .zip(&traces.signals["x"])
        .map(|(t, x)| (x - msd_exact(*t)).abs())
        .fold(0.0, f64::max);
    (err, traces.t.len())
}

#[test]
fn dormand_prince_matches_the_analytic_solution() {
    let (err, samples) = msd_error(&mut DormandPrince::new(1e-6, 1e-8), 0.01);
    assert_eq!(samples, 501);
    assert!(err < 1e-5, "error {err}");
}

#[test]
fn fixed_step_solvers_converge_with_their_order() {
    let (euler, _) = msd_error(&mut Euler { h: 1e-4 }, 1e-4);
    assert!(euler < 2e-3, "euler {euler}");
    let (rk4_coarse, _) = msd_error(&mut Rk4 { h: 1e-2 }, 1e-2);
    let (rk4_fine, _) = msd_error(&mut Rk4 { h: 5e-3 }, 1e-2);
    assert!(rk4_coarse < 1e-7, "rk4 {rk4_coarse}");
    // Fourth order: half the step gives about 1/16 of the error.
    let ratio = rk4_coarse / rk4_fine;
    assert!((12.0..20.0).contains(&ratio), "ratio {ratio}");
}

#[test]
fn layout_mismatch_is_a_diagnostic() {
    let mut model: CompiledModel = serde_json::from_str(MSD).unwrap();
    model.blocks[0].x.len = 2;
    let err = Simulation::new(&model, &fake::library()).err().unwrap();
    assert_eq!((err.code.as_str(), err.id.as_deref()), ("R002", Some("b_00000000000v")));
    model.blocks[0].type_name = "core.Nothing".into();
    let err = Simulation::new(&model, &fake::library()).err().unwrap();
    assert_eq!(err.code, "R001");
}

/// A discrete counter: `xd += 1` on every sample hit; `y = xd`.
struct Counter;
impl Block for Counter {
    fn outputs(&mut self, cx: &mut Ctx) {
        cx.y[0] = cx.xd[0];
    }
    fn update(&mut self, cx: &mut Ctx) {
        cx.xd[0] += 1.0;
    }
}

struct CounterType(BlockSpec);
impl BlockType for CounterType {
    fn spec(&self) -> &BlockSpec {
        &self.0
    }
    fn build(&self, _: &Params, _: &[usize], _: SampleTime) -> Result<Built, simucraft_contracts::Diagnostic> {
        let layout = Layout {
            inputs: vec![],
            outputs: vec![1],
            feedthrough: vec![],
            continuous_states: 0,
            discrete_states: 1,
            algebraic: 0,
            zero_crossings: 0,
            sample_time: SampleTime::Discrete {
                period: 0.1,
                offset: 0.0,
            },
        };
        Ok((Box::new(Counter), layout))
    }
}

#[test]
fn discrete_blocks_update_on_their_sample_hits_only() {
    let model: CompiledModel = serde_json::from_value(serde_json::json!({
        "format": "simucraft.ir/0.1",
        "signals": 1,
        "discrete_states": 1,
        "rates": [{ "kind": "discrete", "period": 0.1, "offset": 0.0 }],
        "blocks": [{ "id": "b_000000000001", "type_name": "test.Counter", "outputs": [[0, 1]], "xd": [0, 1], "rate": 0 }],
        "logged": { "n": [0, 1] }
    }))
    .unwrap();
    let library: Vec<Box<dyn BlockType>> = vec![Box::new(CounterType(BlockSpec {
        type_name: "test.Counter".into(),
        doc: String::new(),
        params: vec![],
        inputs: vec![],
        variadic_inputs: None,
        outputs: vec![],
        connectors: vec![],
    }))];
    // Output every 0.025 s. The output changes only on a hit (0.0, 0.1, ...) and holds between.
    let traces = Simulation::new(&model, &library)
        .unwrap()
        .run(&mut DormandPrince::new(1e-6, 1e-8), 0.3, 0.025)
        .unwrap();
    let n = &traces.signals["n"];
    assert_eq!(traces.t.len(), 13);
    assert_eq!(n[..5], [0.0, 0.0, 0.0, 0.0, 1.0][..]);
    assert_eq!(n[12], 3.0);
}

#[test]
fn bdf_solves_the_fixture_and_a_stiff_problem() {
    let (err, _) = msd_error(&mut Bdf { rtol: 1e-6, atol: 1e-8 }, 0.01);
    assert!(err < 1e-4, "msd error {err}");

    // Robertson chemistry to t = 4e10 (the ADR 0001 benchmark). Reference
    // values from Hairer and Wanner.
    let mut x = [1.0, 0.0, 0.0];
    let mut rhs = |_t: f64, x: &[f64], dx: &mut [f64]| {
        dx[0] = -0.04 * x[0] + 1e4 * x[1] * x[2];
        dx[1] = 0.04 * x[0] - 1e4 * x[1] * x[2] - 3e7 * x[1] * x[1];
        dx[2] = 3e7 * x[1] * x[1];
    };
    Bdf { rtol: 1e-4, atol: 1e-8 }
        .advance(&mut rhs, 0.0, &mut x, 4e10)
        .unwrap();
    let want = [4.8641e-08, 1.9456e-13, 1.0];
    for i in 0..3 {
        assert!(
            (x[i] - want[i]).abs() < 1e-8 + 1e-2 * want[i],
            "x[{i}] = {} vs {}",
            x[i],
            want[i]
        );
    }
}
