//! The only raw C API boundary: offsets, int64 information and interruption.
//! All pointers refer to live HiGHS handles owned by the safe `highs` crate.
use highs_sys::*;
use lp_ip_craft::{CancellationToken, Error, Result};
use std::ffi::{CStr, c_char, c_void};
use std::time::Instant;

pub fn version() -> String {
    // SAFETY: the API returns a static NUL-terminated version string.
    unsafe {
        CStr::from_ptr(Highs_version())
            .to_string_lossy()
            .into_owned()
    }
}
fn ok(code: HighsInt, op: &str) -> Result<()> {
    if code == kHighsStatusOk {
        Ok(())
    } else {
        Err(Error::Backend(format!("{op}: status {code}")))
    }
}
pub fn offset(model: &mut highs::Model, offset: f64) -> Result<()> {
    // SAFETY: live uniquely borrowed handle; no buffers retained.
    ok(
        unsafe { Highs_changeObjectiveOffset(model.as_mut_ptr(), offset) },
        "objective offset",
    )
}
pub fn int64(model: &highs::SolvedModel, name: &CStr) -> Result<i64> {
    let mut value = 0;
    // SAFETY: live handle, NUL-terminated name and writable out parameter.
    ok(
        unsafe { Highs_getInt64InfoValue(model.as_ptr(), name.as_ptr(), &mut value) },
        "int64 info",
    )?;
    Ok(value)
}
/// Stable boxed context. Must be declared before the handle and retained until
/// the model is destroyed. HiGHS may call it from a worker; only atomic reads
/// and immutable monotonic clock state are accessed. No Python or unwinding.
pub struct Interrupt {
    pub token: CancellationToken,
    pub began: Instant,
    pub seconds: Option<f64>,
}
impl Interrupt {
    pub fn timed_out(&self) -> bool {
        self.seconds
            .is_some_and(|s| self.began.elapsed().as_secs_f64() >= s)
    }
    pub fn install(&mut self, model: &mut highs::Model) -> Result<()> {
        // SAFETY: caller keeps boxed self alive beyond model destruction.
        ok(
            unsafe {
                Highs_setCallback(
                    model.as_mut_ptr(),
                    Some(callback),
                    self as *mut Self as *mut c_void,
                )
            },
            "set callback",
        )?;
        for kind in [
            kHighsCallbackSimplexInterrupt,
            kHighsCallbackIpmInterrupt,
            kHighsCallbackMipInterrupt,
        ] {
            // SAFETY: same live uniquely borrowed model; registered context alive.
            ok(
                unsafe { Highs_startCallback(model.as_mut_ptr(), kind) },
                "start callback",
            )?;
        }
        Ok(())
    }
}
unsafe extern "C" fn callback(
    _kind: i32,
    _msg: *const c_char,
    _out: *const HighsCallbackDataOut,
    input: *mut HighsCallbackDataIn,
    data: *mut c_void,
) {
    if data.is_null() || input.is_null() {
        return;
    }
    // SAFETY: registration guarantees stable context and callback-scoped input.
    let ctx = unsafe { &*(data as *const Interrupt) };
    if ctx.token.is_cancelled() || ctx.timed_out() {
        unsafe {
            (*input).user_interrupt = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_requests_cancellation_and_deadline_without_python() {
        let token = CancellationToken::default();
        token.cancel();
        let mut context = Box::new(Interrupt {
            token,
            began: Instant::now(),
            seconds: None,
        });
        // SAFETY: C callback input is a C struct of integers and nullable pointers;
        // all-zero is valid. It is live for the entire direct callback invocation.
        let mut input: HighsCallbackDataIn = unsafe { std::mem::zeroed() };
        unsafe {
            callback(
                1,
                std::ptr::null(),
                std::ptr::null(),
                &mut input,
                context.as_mut() as *mut Interrupt as *mut c_void,
            );
        }
        assert_eq!(input.user_interrupt, 1);
        context.token = CancellationToken::default();
        context.seconds = Some(0.0);
        input.user_interrupt = 0;
        unsafe {
            callback(
                1,
                std::ptr::null(),
                std::ptr::null(),
                &mut input,
                context.as_mut() as *mut Interrupt as *mut c_void,
            );
        }
        assert_eq!(input.user_interrupt, 1);
    }
}
