use vstd::prelude::*;

verus! {

/// Compute start + width only when the end is within limit.
/// This is a verifier smoke test, not yet a proof of the runtime parser.
pub fn checked_end(start: usize, width: usize, limit: usize) -> (result: Option<usize>)
    ensures
        match result {
            Some(end) => end <= limit,
            None => true,
        }
{
    if start <= limit && width <= limit - start {
        Some(start + width)
    } else {
        None
    }
}

fn main() {
    assert(checked_end(3, 4, 10) == Some(7));
    assert(checked_end(9, 2, 10) == None);
    assert(checked_end(10, 0, 10) == Some(10));
}

} // verus!
