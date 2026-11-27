//! Coalesce eligible waits on monotonic private timelines within one batch.
//! Callers must filter completed, self and label-blocked dependencies first.

pub(super) fn include<F: Clone + PartialEq>(waits: &mut Vec<(F, u64)>, fence: &F, tick: u64) {
    if let Some((_, previous)) = waits.iter_mut().find(|(f, _)| f == fence) {
        *previous = (*previous).max(tick);
    } else {
        waits.push((fence.clone(), tick));
    }
}

#[cfg(test)]
#[path = "../../tests/device/placement/waits.rs"]
mod tests;
