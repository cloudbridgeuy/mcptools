use std::time::{Duration, SystemTime};

pub struct Plan<'a> {
    pub id: &'a str,
    pub modified: SystemTime,
    pub active: bool,
}

pub fn newest_activity(times: &[SystemTime]) -> Option<SystemTime> {
    times.iter().copied().max()
}

pub fn expired_plan_ids<'a>(
    plans: &[Plan<'a>],
    now: SystemTime,
    ttl: Duration,
) -> Result<Vec<&'a str>, &'static str> {
    Ok(plans
        .iter()
        .filter(|plan| !plan.active && now.duration_since(plan.modified).is_ok_and(|age| age > ttl))
        .map(|plan| plan.id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_uses_newest_timestamp_regardless_of_source() {
        let old = SystemTime::UNIX_EPOCH;
        let recent = old + Duration::from_secs(100);
        for times in [
            [recent, old, old, old],
            [old, recent, old, old],
            [old, old, recent, old],
            [old, old, old, recent],
        ] {
            assert_eq!(newest_activity(&times), Some(recent));
        }
        assert_eq!(newest_activity(&[]), None);
    }

    #[test]
    fn selects_only_inactive_plans_older_than_ttl() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let plans = [
            Plan {
                id: "old",
                modified: now - Duration::from_secs(11),
                active: false,
            },
            Plan {
                id: "boundary",
                modified: now - Duration::from_secs(10),
                active: false,
            },
            Plan {
                id: "active",
                modified: now - Duration::from_secs(11),
                active: true,
            },
            Plan {
                id: "future",
                modified: now + Duration::from_secs(1),
                active: false,
            },
        ];
        assert_eq!(
            expired_plan_ids(&plans, now, Duration::from_secs(10)),
            Ok(vec!["old"])
        );
        assert_eq!(expired_plan_ids(&[], now, Duration::ZERO), Ok(vec![]));
    }
}
