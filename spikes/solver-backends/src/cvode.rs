//! The same Robertson ODE through SUNDIALS CVODE (BDF, dense LU, analytic Jacobian).

use std::ffi::c_void;
use std::ptr::null_mut;

use sundials_sys::*;

const P: [f64; 3] = [0.04, 1.0e4, 3.0e7];

unsafe extern "C" fn rhs(_t: f64, x: N_Vector, ydot: N_Vector, _: *mut c_void) -> i32 {
    let (x, y) = unsafe { (std::slice::from_raw_parts(N_VGetArrayPointer(x), 3), std::slice::from_raw_parts_mut(N_VGetArrayPointer(ydot), 3)) };
    y[0] = -P[0] * x[0] + P[1] * x[1] * x[2];
    y[1] = P[0] * x[0] - P[1] * x[1] * x[2] - P[2] * x[1] * x[1];
    y[2] = P[2] * x[1] * x[1];
    0
}

unsafe extern "C" fn jac(_t: f64, x: N_Vector, _fy: N_Vector, j: SUNMatrix, _: *mut c_void, _: N_Vector, _: N_Vector, _: N_Vector) -> i32 {
    unsafe {
        let x = std::slice::from_raw_parts(N_VGetArrayPointer(x), 3);
        let col = |c: i64| std::slice::from_raw_parts_mut(SUNDenseMatrix_Column(j, c), 3);
        let (c0, c1, c2) = (col(0), col(1), col(2));
        c0.copy_from_slice(&[-P[0], P[0], 0.0]);
        c1.copy_from_slice(&[P[1] * x[2], -P[1] * x[2] - 2.0 * P[2] * x[1], 2.0 * P[2] * x[1]]);
        c2.copy_from_slice(&[P[1] * x[1], -P[1] * x[1], 0.0]);
    }
    0
}

pub fn robertson_ode() -> Vec<f64> {
    unsafe {
        let mut ctx: SUNContext = null_mut();
        SUNContext_Create(SUN_COMM_NULL, &mut ctx);
        let y = N_VNew_Serial(3, ctx);
        std::slice::from_raw_parts_mut(N_VGetArrayPointer(y), 3).copy_from_slice(&[1.0, 0.0, 0.0]);
        let atol = N_VNew_Serial(3, ctx);
        std::slice::from_raw_parts_mut(N_VGetArrayPointer(atol), 3).copy_from_slice(&[1e-8, 1e-6, 1e-6]);
        let mut mem = CVodeCreate(CV_BDF, ctx);
        assert_eq!(CVodeInit(mem, Some(rhs), 0.0, y), 0);
        assert_eq!(CVodeSVtolerances(mem, 1e-4, atol), 0);
        CVodeSetMaxNumSteps(mem, 100_000);
        let a = SUNDenseMatrix(3, 3, ctx);
        let ls = SUNLinSol_Dense(y, a, ctx);
        assert_eq!(CVodeSetLinearSolver(mem, ls, a), 0);
        assert_eq!(CVodeSetJacFn(mem, Some(jac)), 0);
        let mut t = 0.0;
        assert!(CVode(mem, 4e10, y, &mut t, CV_NORMAL) >= 0);
        let out = std::slice::from_raw_parts(N_VGetArrayPointer(y), 3).to_vec();
        CVodeFree(&mut mem);
        SUNLinSolFree(ls);
        SUNMatDestroy(a);
        N_VDestroy(y);
        N_VDestroy(atol);
        SUNContext_Free(&mut ctx);
        out
    }
}
