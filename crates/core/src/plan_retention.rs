use std::time::{Duration, SystemTime};

pub struct Plan<'a> {
    pub id: &'a str,
    pub modified: SystemTime,
    pub active: bool,
}

pub fn expired_plan_ids<'a>(
    _plans: &[Plan<'a>],
    _now: SystemTime,
    _ttl: Duration,
) -> Result<Vec<&'a str>, &'static str> {
    Err("retention selection is not implemented")
}
