//! Declarative macros for defining metrics with less boilerplate.

/// Define counter metrics with optional labels.
///
/// # Example
/// ```ignore
/// define_counters! {
///     // Simple counter: generates fn name() that increments by 1
///     ("metric_name", name, "Description"),
///
///     // Counter with value: generates fn name(val: u64)
///     ("metric_name", name, "Description", value),
///
///     // Labeled counter: generates fn name(label: &str)
///     ("metric_name", name, "Description", label: "label_name"),
///
///     // Labeled counter with value: generates fn name(label: &str, val: u64)
///     ("metric_name", name, "Description", label: "label_name", value),
/// }
/// ```
#[macro_export]
macro_rules! define_counters {
    // Simple counter (increment by 1)
    ($(($metric:literal, $fn_name:ident, $desc:literal)),* $(,)?) => {
        /// Initialize counter descriptions
        pub fn init_descriptions() {
            $(
                ::metrics::describe_counter!($metric, $desc);
            )*
        }

        $(
            #[doc = $desc]
            pub fn $fn_name() {
                ::metrics::counter!($metric).increment(1);
            }
        )*
    };
}

/// Define gauge metrics with optional labels.
#[macro_export]
macro_rules! define_gauges {
    ($(($metric:literal, $fn_name:ident, $desc:literal)),* $(,)?) => {
        $(
            #[doc = $desc]
            pub fn $fn_name(value: u64) {
                ::metrics::gauge!($metric).set(value as f64);
            }
        )*

        pub fn init_gauge_descriptions() {
            $(
                ::metrics::describe_gauge!($metric, $desc);
            )*
        }
    };
}

/// Define histogram metrics.
#[macro_export]
macro_rules! define_histograms {
    ($(($metric:literal, $fn_name:ident, $desc:literal)),* $(,)?) => {
        $(
            #[doc = $desc]
            pub fn $fn_name(value: f64) {
                ::metrics::histogram!($metric).record(value);
            }
        )*

        pub fn init_histogram_descriptions() {
            $(
                ::metrics::describe_histogram!($metric, $desc);
            )*
        }
    };
}

/// Comprehensive macro to define all metric types for a module.
///
/// # Example
/// ```ignore
/// define_module_metrics! {
///     module: network,
///     counters: [
///         ("icn_network_connections_total", connections_total_inc, "Total connections"),
///         ("icn_network_messages_sent", messages_sent_add, "Messages sent", value),
///     ],
///     gauges: [
///         ("icn_network_connections_active", connections_active_set, "Active connections"),
///     ],
///     histograms: [
///         ("icn_network_latency_seconds", latency_record, "Network latency"),
///     ],
///     labeled_counters: [
///         ("icn_network_errors", errors_inc, "Errors by type", "error_type"),
///     ],
///     labeled_gauges: [
///         ("icn_network_peers", peers_by_class_set, "Peers by class", "class"),
///     ],
/// }
/// ```
#[macro_export]
macro_rules! define_module_metrics {
    (
        module: $module:ident,
        $(counters: [$($counter:tt),* $(,)?],)?
        $(counters_with_value: [$($counter_val:tt),* $(,)?],)?
        $(gauges: [$($gauge:tt),* $(,)?],)?
        $(histograms: [$($histogram:tt),* $(,)?],)?
        $(labeled_counters: [$($lcounter:tt),* $(,)?],)?
        $(labeled_counters_with_value: [$($lcounter_val:tt),* $(,)?],)?
        $(labeled_gauges: [$($lgauge:tt),* $(,)?],)?
        $(multi_labeled_counters: [$($mlcounter:tt),* $(,)?],)?
        $(multi_labeled_gauges: [$($mlgauge:tt),* $(,)?],)?
    ) => {
        pub mod $module {
            use ::metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram};

            /// Initialize all metric descriptions for this module
            pub fn init_descriptions() {
                // Counters
                $($(
                    $crate::_describe_counter!($counter);
                )*)?

                // Counters with value
                $($(
                    $crate::_describe_counter!($counter_val);
                )*)?

                // Gauges
                $($(
                    $crate::_describe_gauge!($gauge);
                )*)?

                // Histograms
                $($(
                    $crate::_describe_histogram!($histogram);
                )*)?

                // Labeled counters
                $($(
                    $crate::_describe_counter!($lcounter);
                )*)?

                // Labeled counters with value
                $($(
                    $crate::_describe_counter!($lcounter_val);
                )*)?

                // Labeled gauges
                $($(
                    $crate::_describe_gauge!($lgauge);
                )*)?

                // Multi-labeled counters
                $($(
                    $crate::_describe_counter!($mlcounter);
                )*)?

                // Multi-labeled gauges
                $($(
                    $crate::_describe_gauge!($mlgauge);
                )*)?
            }

            // Generate simple counter functions
            $($(
                $crate::_gen_counter!($counter);
            )*)?

            // Generate counters with value
            $($(
                $crate::_gen_counter_with_value!($counter_val);
            )*)?

            // Generate gauge functions
            $($(
                $crate::_gen_gauge!($gauge);
            )*)?

            // Generate histogram functions
            $($(
                $crate::_gen_histogram!($histogram);
            )*)?

            // Generate labeled counter functions
            $($(
                $crate::_gen_labeled_counter!($lcounter);
            )*)?

            // Generate labeled counters with value
            $($(
                $crate::_gen_labeled_counter_with_value!($lcounter_val);
            )*)?

            // Generate labeled gauge functions
            $($(
                $crate::_gen_labeled_gauge!($lgauge);
            )*)?

            // Generate multi-labeled counter functions
            $($(
                $crate::_gen_multi_labeled_counter!($mlcounter);
            )*)?

            // Generate multi-labeled gauge functions
            $($(
                $crate::_gen_multi_labeled_gauge!($mlgauge);
            )*)?
        }
    };
}

// Helper macros for description generation
#[macro_export]
#[doc(hidden)]
macro_rules! _describe_counter {
    (($metric:literal, $fn_name:ident, $desc:literal $(, $extra:tt)*)) => {
        describe_counter!($metric, $desc);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _describe_gauge {
    (($metric:literal, $fn_name:ident, $desc:literal $(, $extra:tt)*)) => {
        describe_gauge!($metric, $desc);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _describe_histogram {
    (($metric:literal, $fn_name:ident, $desc:literal $(, $extra:tt)*)) => {
        describe_histogram!($metric, $desc);
    };
}

// Helper macros for function generation

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_counter {
    (($metric:literal, $fn_name:ident, $desc:literal)) => {
        #[doc = $desc]
        pub fn $fn_name() {
            counter!($metric).increment(1);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_counter_with_value {
    (($metric:literal, $fn_name:ident, $desc:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(value: u64) {
            counter!($metric).increment(value);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_gauge {
    (($metric:literal, $fn_name:ident, $desc:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(value: u64) {
            gauge!($metric).set(value as f64);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_histogram {
    (($metric:literal, $fn_name:ident, $desc:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(value: f64) {
            histogram!($metric).record(value);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_labeled_counter {
    (($metric:literal, $fn_name:ident, $desc:literal, $label:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(label_value: &str) {
            counter!($metric, $label => label_value.to_string()).increment(1);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_labeled_counter_with_value {
    (($metric:literal, $fn_name:ident, $desc:literal, $label:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(label_value: &str, value: u64) {
            counter!($metric, $label => label_value.to_string()).increment(value);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_labeled_gauge {
    (($metric:literal, $fn_name:ident, $desc:literal, $label:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(label_value: &str, value: u64) {
            gauge!($metric, $label => label_value.to_string()).set(value as f64);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_multi_labeled_counter {
    (($metric:literal, $fn_name:ident, $desc:literal, $label1:literal, $label2:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(label1_value: &str, label2_value: &str) {
            counter!($metric, $label1 => label1_value.to_string(), $label2 => label2_value.to_string()).increment(1);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! _gen_multi_labeled_gauge {
    (($metric:literal, $fn_name:ident, $desc:literal, $label1:literal, $label2:literal)) => {
        #[doc = $desc]
        pub fn $fn_name(label1_value: &str, label2_value: &str, value: u64) {
            gauge!($metric, $label1 => label1_value.to_string(), $label2 => label2_value.to_string()).set(value as f64);
        }
    };
}
