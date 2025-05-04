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

/// Convert a SystemTime to DateTime<Utc>
pub fn system_time_to_datetime(time: SystemTime) -> DateTime<Utc> {
    // Get duration since epoch, default to 0 if time is before epoch
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos();
    
    // Use timestamp_opt which returns a chrono::LocalResult
    match Utc.timestamp_opt(seconds, nanos) {
        chrono::LocalResult::Single(dt) => dt,
        // Default to current time if something went wrong
        _ => Utc::now() 
    }
}

/// Convert a DateTime<Utc> to SystemTime
pub fn datetime_to_system_time(dt: DateTime<Utc>) -> SystemTime {
    // Create duration using timestamp components
    let duration = Duration::from_secs(dt.timestamp().max(0) as u64) // Ensure non-negative
        + Duration::from_nanos(dt.timestamp_subsec_nanos() as u64);
    
    // Add to UNIX_EPOCH
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
    use std::time::SystemTime;

    #[test]
    fn test_time_roundtrip_conversion() {
        // Test current time
        let now_system = SystemTime::now();
        let now_dt = system_time_to_datetime(now_system);
        let back_to_system = datetime_to_system_time(now_dt);

        // Calculate difference, accounting for either direction
        let diff = now_system.duration_since(back_to_system)
            .or_else(|_| back_to_system.duration_since(now_system))
            .unwrap_or_default();
        
        // Allow small precision loss in conversion 
        assert!(diff.as_millis() < 2, "Time conversion should be nearly lossless");
    }

    #[test]
    fn test_epoch_time_conversion() {
        // Test with UNIX_EPOCH
        let epoch_dt = system_time_to_datetime(UNIX_EPOCH);
        assert_eq!(epoch_dt.timestamp(), 0);
        assert_eq!(epoch_dt.timestamp_subsec_nanos(), 0);

        let epoch_time = datetime_to_system_time(Utc.timestamp_opt(0, 0).unwrap());
        assert_eq!(
            epoch_time.duration_since(UNIX_EPOCH).unwrap().as_nanos(),
            0
        );
    }

    #[test]
    fn test_negative_timestamp_handling() {
        // Test with pre-epoch time (1960)
        // Should handle gracefully without panics
        if let Some(pre_epoch) = Utc.timestamp_opt(-315619200, 0).single() { // ~ 1960-01-01
            let system_time = datetime_to_system_time(pre_epoch);
            // Should convert to epoch or later
            assert!(system_time >= UNIX_EPOCH);
        }
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