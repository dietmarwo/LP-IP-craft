//! The only raw C API boundary: offsets, int64 information and interruption.
//! All pointers refer to live HiGHS handles owned by the safe `highs` crate.
use highs_sys::*;
use lp_ip_craft::{CancellationToken, Error, Result};
use std::ffi::{CStr, c_char, c_void};
use std::time::Instant;

/// Version of the linked HiGHS library.
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
/// Sets the constant objective term, which the safe wrapper does not expose.
pub fn offset(model: &mut highs::Model, offset: f64) -> Result<()> {
    // SAFETY: live uniquely borrowed handle; no buffers retained.
    ok(
        unsafe { Highs_changeObjectiveOffset(model.as_mut_ptr(), offset) },
        "objective offset",
    )
}
/// Reads a 64-bit integer info value, such as `mip_node_count`.
pub fn int64(model: &highs::SolvedModel, name: &CStr) -> Result<i64> {
    let mut value = 0;
    // SAFETY: live handle, NUL-terminated name and writable out parameter.
    ok(
        unsafe { Highs_getInt64InfoValue(model.as_ptr(), name.as_ptr(), &mut value) },
        "int64 info",
    )?;
    Ok(value)
}
/// Context of the interrupt callback.
///
/// It must stay at a fixed address (boxed) and outlive the HiGHS handle it is
/// installed on: declare the box before the handle, so the handle is dropped
/// first. The callback only reads an atomic flag and a monotonic clock; it
/// never calls Python, allocates or unwinds.
pub struct Interrupt {
    /// Cancellation requested by the caller.
    pub token: CancellationToken,
    /// Start of the time budget (after input validation).
    pub began: Instant,
    /// Whole-call time budget in seconds, if any.
    pub seconds: Option<f64>,
}
impl Interrupt {
    /// Whether the time budget is used up.
    pub fn timed_out(&self) -> bool {
        self.seconds
            .is_some_and(|s| self.began.elapsed().as_secs_f64() >= s)
    }

    /// Registers the callback for the simplex, IPM and MIP interrupt points.
    ///
    /// `self` must live in a `Box` that outlives `model`'s native handle.
    pub fn install(&self, model: &mut highs::Model) -> Result<()> {
        let data = self as *const Self as *mut c_void;
        // SAFETY: live, uniquely borrowed handle. `data` is the heap address
        // of the boxed context, which the caller keeps alive until the handle
        // is destroyed; the callback only reads through it.
        ok(
            unsafe { Highs_setCallback(model.as_mut_ptr(), Some(callback), data) },
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
    // SAFETY: `data` is the `Interrupt` registered by `install`, alive for the
    // whole run; `input` is valid for the duration of this call.
    let ctx = unsafe { &*(data as *const Interrupt) };
    if ctx.token.is_cancelled() || ctx.timed_out() {
        // SAFETY: see above.
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
