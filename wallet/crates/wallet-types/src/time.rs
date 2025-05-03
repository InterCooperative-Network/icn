use std::time::{SystemTime, Duration, UNIX_EPOCH};
use chrono::{DateTime, Utc, TimeZone};

/// Wrapper type for SystemTime to allow conversion implementations
#[derive(Debug, Clone)]
pub struct TimeWrapper(pub SystemTime);

/// Wrapper type for DateTime to allow conversion implementations
#[derive(Debug, Clone)]
pub struct DateTimeWrapper(pub DateTime<Utc>);

/// Convert between different time representations
pub fn convert_time<T, U>(time: T) -> U 
where 
    T: Into<SystemTime>,
    SystemTime: Into<U>,
{
    let system_time = time.into();
    system_time.into()
}

/// Convert from SystemTime to DateTime<Utc>
pub fn system_time_to_datetime(time: SystemTime) -> DateTime<Utc> {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos();
    match Utc.timestamp_opt(seconds, nanos) {
        chrono::LocalResult::Single(dt) => dt,
        _ => Utc::now() // Fallback if conversion fails
    }
}

/// Convert from DateTime<Utc> to SystemTime
pub fn datetime_to_system_time(dt: DateTime<Utc>) -> SystemTime {
    let duration = Duration::from_secs(dt.timestamp() as u64)
        + Duration::from_nanos(dt.timestamp_subsec_nanos() as u64);
    UNIX_EPOCH + duration
}

// Implementation for the wrapper types
impl From<TimeWrapper> for DateTime<Utc> {
    fn from(wrapper: TimeWrapper) -> Self {
        system_time_to_datetime(wrapper.0)
    }
}

impl From<DateTimeWrapper> for SystemTime {
    fn from(wrapper: DateTimeWrapper) -> Self {
        datetime_to_system_time(wrapper.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_time_to_datetime_conversion() {
        let now_system = SystemTime::now();
        let now_dt = system_time_to_datetime(now_system);
        let back_to_system = datetime_to_system_time(now_dt);

        // Allow for minor precision loss in conversion
        let diff = now_system.duration_since(back_to_system)
            .or_else(|_| back_to_system.duration_since(now_system))
            .unwrap_or_default();
        
        assert!(diff.as_millis() < 2, "Time conversion should be nearly lossless");
    }

    #[test]
    fn test_wrapper_conversions() {
        let now_system = SystemTime::now();
        let wrapper = TimeWrapper(now_system);
        let dt: DateTime<Utc> = wrapper.into();
        
        let dt_wrapper = DateTimeWrapper(dt);
        let back_to_system: SystemTime = dt_wrapper.into();

        // Allow for minor precision loss
        let diff = now_system.duration_since(back_to_system)
            .or_else(|_| back_to_system.duration_since(now_system))
            .unwrap_or_default();
        
        assert!(diff.as_millis() < 2, "Wrapper conversion should be nearly lossless");
    }

    #[test]
    fn test_convert_time_function() {
        // For future extension when we have more conversion implementations
        // Currently just a placeholder test
        let now = SystemTime::now();
        let same_now: SystemTime = convert_time(now);
        assert_eq!(format!("{:?}", now), format!("{:?}", same_now));
    }
} 