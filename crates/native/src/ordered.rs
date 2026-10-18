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
#[cfg(test)]
mod tests {
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
