//! Linear ordered collection of independently computed task outputs.
use lp_ip_craft::{Error, Result};
pub(crate) fn gather<T>(count: usize, values: Vec<(usize, T)>) -> Result<Vec<(usize, T)>> {
    let mut slots: Vec<_> = (0..count).map(|_| None).collect();
    for (i, value) in values {
        let slot = slots
            .get_mut(i)
            .ok_or_else(|| Error::Backend("task output index outside queue".into()))?;
        if slot.is_some() {
            return Err(Error::Backend("duplicate task output".into()));
        }
        *slot = Some(value);
    }
    Ok(slots
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|v| (i, v)))
        .collect())
}
/// Runs a worker's share on the calling thread. A panic is reported as
/// `Error::Backend("<what> worker panicked")`, the error a spawned worker's
/// panic gives when it is joined.
pub(crate) fn inline<T>(what: &str, work: impl FnOnce() -> T) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .map_err(|_| Error::Backend(format!("{what} worker panicked")))
}
#[cfg(test)]
mod tests {
    #[test]
    fn an_inline_share_reports_a_panic_like_a_spawned_worker() {
        assert_eq!(super::inline("residual", || 3).unwrap(), 3);
        let r = super::inline("residual", || -> usize { panic!("injected") });
        assert!(
            matches!(r, Err(lp_ip_craft::Error::Backend(m)) if m == "residual worker panicked")
        );
    }
    #[test]
    fn incomplete_and_invalid_task_outputs_are_explicit() {
        assert_eq!(
            super::gather(5, vec![(4, "last"), (0, "first")]).unwrap(),
            vec![(0, "first"), (4, "last")]
        );
        assert!(super::gather(2, vec![(0, 1), (0, 2)]).is_err());
        assert!(super::gather(2, vec![(2, 1)]).is_err());
    }
}
