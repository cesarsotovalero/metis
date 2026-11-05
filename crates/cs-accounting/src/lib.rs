#![forbid(unsafe_code)]
use cs_core::{Usage, UsageMode};
fn add(a: Option<u64>, b: Option<u64>) -> Result<Option<u64>, &'static str> {
    match (a, b) {
        (Some(a), Some(b)) => a.checked_add(b).map(Some).ok_or("usage overflow"),
        (None, b) => Ok(b),
        (a, None) => Ok(a),
    }
}
pub fn normalize(mut u: Usage) -> Result<Usage, &'static str> {
    if let Some(total) = u.total_input {
        for part in [u.uncached_input, u.cache_read_input, u.cache_write_input]
            .into_iter()
            .flatten()
        {
            if part > total {
                return Err("inconsistent input counters");
            }
        }
    }
    if let (Some(total), Some(visible)) = (u.total_output, u.visible_output) {
        if visible > total {
            return Err("inconsistent output counters");
        }
    }

    if let (Some(total), Some(cache), Some(write)) =
        (u.total_input, u.cache_read_input, u.cache_write_input)
    {
        let uncached = total
            .checked_sub(cache)
            .and_then(|x| x.checked_sub(write))
            .ok_or("inconsistent input counters")?;
        if u.uncached_input.is_some_and(|x| x != uncached) {
            return Err("inconsistent input counters");
        }
        u.uncached_input = Some(uncached);
    }
    if u.total_input.is_none() {
        if let (Some(a), Some(b), Some(c)) =
            (u.uncached_input, u.cache_read_input, u.cache_write_input)
        {
            u.total_input = add(add(Some(a), Some(b))?, Some(c))?;
        }
    }
    if u.reasoning_included_in_output == Some(true) {
        if let (Some(total), Some(reasoning)) = (u.total_output, u.reasoning_output) {
            let visible = total
                .checked_sub(reasoning)
                .ok_or("inconsistent reasoning counters")?;
            if u.visible_output.is_some_and(|x| x != visible) {
                return Err("inconsistent output counters");
            }
            u.visible_output = Some(visible);
        }
    }
    if let (Some(input), Some(output)) = (u.total_input, u.total_output) {
        let total = input.checked_add(output).ok_or("usage overflow")?;
        if u.total_tokens.is_some_and(|reported| reported != total) {
            return Err("inconsistent total tokens");
        }
        u.total_tokens = Some(total);
    }
    Ok(u)
}
pub fn apply(current: &mut Usage, sample: Usage, mode: UsageMode) -> Result<(), &'static str> {
    let mut next = sample;
    if mode == UsageMode::Delta {
        macro_rules! field {
            ($f:ident) => {
                next.$f = add(current.$f, next.$f)?;
            };
        }
        field!(uncached_input);
        field!(cache_read_input);
        field!(cache_write_input);
        field!(total_input);
        field!(visible_output);
        field!(reasoning_output);
        field!(total_output);
        if next.total_tokens.is_some() {
            field!(total_tokens);
        }
        next.reasoning_included_in_output = next
            .reasoning_included_in_output
            .or(current.reasoning_included_in_output);
    }
    *current = normalize(next)?;
    Ok(())
}
/// Aggregates only fully covered categories; absence is never treated as zero.
pub fn aggregate(rows: &[Usage]) -> Result<Usage, &'static str> {
    let mut out = Usage::default();
    macro_rules! field {
        ($f:ident) => {
            if !rows.is_empty() && rows.iter().all(|u| u.$f.is_some()) {
                out.$f = Some(rows.iter().try_fold(0u64, |a, u| {
                    a.checked_add(u.$f.unwrap()).ok_or("usage overflow")
                })?);
            }
        };
    }
    field!(uncached_input);
    field!(cache_read_input);
    field!(cache_write_input);
    field!(total_input);
    field!(visible_output);
    field!(reasoning_output);
    field!(total_output);
    field!(total_tokens);
    if let Some(first) = rows.first() {
        if rows
            .iter()
            .all(|u| u.reasoning_included_in_output == first.reasoning_included_in_output)
        {
            out.reasoning_included_in_output = first.reasoning_included_in_output;
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_and_reasoning_subsets() {
        let u = normalize(Usage {
            total_input: Some(100),
            cache_read_input: Some(60),
            cache_write_input: Some(10),
            total_output: Some(20),
            reasoning_output: Some(7),
            reasoning_included_in_output: Some(true),
            ..Usage::default()
        })
        .unwrap();
        assert_eq!(u.uncached_input, Some(30));
        assert_eq!(u.visible_output, Some(13));
    }
    #[test]
    fn revisions_replace_and_deltas_add() {
        let mut u = Usage::default();
        for x in [10, 20, 18] {
            apply(
                &mut u,
                Usage {
                    total_input: Some(x),
                    ..Usage::default()
                },
                UsageMode::Cumulative,
            )
            .unwrap();
        }
        assert_eq!(u.total_input, Some(18));
        apply(
            &mut u,
            Usage {
                total_input: Some(2),
                ..Usage::default()
            },
            UsageMode::Delta,
        )
        .unwrap();
        assert_eq!(u.total_input, Some(20));
    }
    #[test]
    fn missing_is_not_zero() {
        assert_eq!(
            aggregate(&[
                Usage {
                    total_input: Some(9),
                    ..Usage::default()
                },
                Usage::default()
            ])
            .unwrap()
            .total_input,
            None
        );
    }
    #[test]
    fn inconsistent_and_overflow_rejected() {
        assert!(normalize(Usage {
            total_input: Some(1),
            cache_read_input: Some(2),
            cache_write_input: Some(0),
            ..Usage::default()
        })
        .is_err());
        assert!(aggregate(&[
            Usage {
                total_input: Some(u64::MAX),
                ..Usage::default()
            },
            Usage {
                total_input: Some(1),
                ..Usage::default()
            }
        ])
        .is_err());
    }
}
