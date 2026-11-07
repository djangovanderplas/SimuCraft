//! ADR 0001 measurement: cost of interpreting a compiled model, per block call.
//! A chain Constant -> N x Gain runs on the fake runtime (two passes per step).
//! Run: cargo run --release -p simucraft-contracts --example dispatch

use std::time::Instant;

use simucraft_contracts::block::SampleTime;
use simucraft_contracts::fake;
use simucraft_contracts::ir::{BlockInstance, CompiledModel, ParamValue, Slots};

fn chain(n: usize) -> CompiledModel {
    let block = |i: usize, type_name: &str, param: (&str, f64), inputs: Vec<Slots>| BlockInstance {
        id: format!("b_{i:012}"),
        type_name: type_name.into(),
        params: [(param.0.to_string(), ParamValue::Scalar(param.1))].into(),
        inputs,
        outputs: vec![Slots::new(i, 1)],
        x: Slots::default(),
        xd: Slots::default(),
        z: Slots::default(),
        zc: Slots::default(),
        rate: 0,
    };
    let mut blocks = vec![block(0, "core.Constant", ("value", 1.0), vec![])];
    blocks.extend((1..=n).map(|i| block(i, "core.Gain", ("k", 1.0), vec![Slots::new(i - 1, 1)])));
    CompiledModel {
        format: simucraft_contracts::ir::FORMAT.into(),
        signals: n + 1,
        continuous_states: 0,
        discrete_states: 0,
        algebraic: 0,
        zero_crossings: 0,
        rates: vec![SampleTime::Continuous],
        blocks,
        inports: Default::default(),
        outports: Default::default(),
        logged: [("out".to_string(), Slots::new(n, 1))].into(),
    }
}

fn main() {
    let library = fake::library();
    for n in [100, 1_000, 10_000] {
        let model = chain(n);
        let steps = 1_000_000 / n;
        let start = Instant::now();
        let traces = fake::run_euler(&model, &library, 1.0, steps as f64).unwrap();
        let secs = start.elapsed().as_secs_f64();
        assert_eq!(traces["out"].last(), Some(&1.0));
        let calls = 2.0 * (n + 1) as f64 * (steps + 1) as f64;
        println!(
            "{n:>6} blocks  {:6.1} ns per block call  {:8.1} us per step",
            secs * 1e9 / calls,
            secs * 1e6 / (steps + 1) as f64
        );
    }
}
