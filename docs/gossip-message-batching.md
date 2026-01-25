# Gossip Message Batching

## Overview

Message batching is a performance optimization that reduces network overhead by combining multiple small messages into a single batch before transmission. This feature is particularly beneficial for high-throughput scenarios where many small messages are sent in quick succession.

## Benefits

1. **Reduced Network Overhead**: Fewer TCP/QUIC frame headers
2. **Better Serialization Efficiency**: Single encode operation for multiple messages
3. **Lower Latency**: Fewer network round-trips
4. **Configurable Trade-offs**: Balance between latency and throughput

Expected performance improvements:
- 30-50% reduction in network overhead for small messages
- Higher throughput under heavy message load
- Lower CPU usage for serialization

## Configuration

### Default Configuration

```rust
use icn_gossip::BatchingConfig;

// Balanced configuration (default)
let config = BatchingConfig::default();
// max_batch_size: 10 messages
// max_delay: 10ms
// max_batch_bytes: 256KB
// compression_threshold: 1KB
// enabled: true
```

### Predefined Configurations

```rust
// Low latency - prioritize quick sends
let config = BatchingConfig::low_latency();
// max_batch_size: 5
// max_delay: 5ms

// High throughput - larger batches
let config = BatchingConfig::high_throughput();
// max_batch_size: 50
// max_delay: 50ms
// max_batch_bytes: 1MB

// Disabled - send immediately
let config = BatchingConfig::disabled();
```

### Custom Configuration

```rust
let config = BatchingConfig {
    max_batch_size: 20,                    // Maximum messages per batch
    max_delay: Duration::from_millis(15),  // Maximum batching delay
    compression_threshold: 2048,            // Compress batches > 2KB
    enabled: true,
    max_batch_bytes: 512 * 1024,           // Maximum 512KB per batch
};

gossip_actor.set_batching_config(config);
```

## Configuration Parameters

### `max_batch_size`
Maximum number of messages to accumulate before sending a batch.
- **Default**: 10
- **Range**: 1-100 (practical limits)
- **Trade-off**: Higher = better efficiency, higher latency

### `max_delay`
Maximum time to wait before sending a partial batch.
- **Default**: 10ms
- **Range**: 1ms-100ms (recommended)
- **Trade-off**: Higher = better batching, higher latency

### `max_batch_bytes`
Maximum total size of messages in a batch (uncompressed).
- **Default**: 256KB
- **Range**: 10KB-10MB (practical limits)
- **Purpose**: Prevents oversized batches that could block the network

### `compression_threshold`
Minimum batch size before compression is applied.
- **Default**: 1KB
- **Purpose**: Avoid compression overhead for small batches

### `enabled`
Whether batching is enabled.
- **Default**: true
- **Use Case**: Disable for testing or low-latency requirements

## Batch Triggering

A batch is sent when ANY of these conditions is met:

1. **Size Limit**: `max_batch_size` messages accumulated
2. **Byte Limit**: Total message size exceeds `max_batch_bytes`
3. **Time Check**: When a new message arrives, if `max_delay` has elapsed since the last batch send, the current batch is sent
4. **Manual Flush**: `flush_all_batches()` called

**Note**: There is no background timer that automatically flushes batches after `max_delay`. Time-based flushing is evaluated only when a new message arrives or when you call `flush_all_batches()`. A batch containing a single message may remain pending until one of these events occurs.

## Manual Batch Flushing

```rust
// Flush all pending batches immediately
gossip_actor.flush_all_batches();

// Useful for:
// - Shutdown sequences
// - End of critical operations
// - Before blocking operations
```

## Metrics

Monitor batching performance via Prometheus metrics:

### Metrics Reference

| Metric | Type | Description |
|--------|------|-------------|
| `icn_gossip_batches_sent_total` | Counter | Total number of message batches sent |
| `icn_gossip_batches_received_total` | Counter | Total number of message batches received |
| `icn_gossip_batch_size` | Histogram | Number of messages per batch |
| `icn_gossip_batch_compression_ratio` | Histogram | Compression ratio (original/compressed size) |
| `icn_gossip_batches_rejected_oversized_total` | Counter | DoS protection: batches exceeding size limits |
| `icn_gossip_batch_mutex_poisoned_total` | Counter | Critical: thread panics during batch processing |
| `icn_gossip_trust_check_lock_skipped_total` | Counter | Trust graph lock contention events |

### Alerting Recommendations

- **`icn_gossip_batch_mutex_poisoned_total > 0`**: Investigate thread panics immediately. This indicates a critical bug in batch processing.
- **`icn_gossip_trust_check_lock_skipped_total` increasing**: Trust graph lock contention. Consider increasing trust graph capacity or reducing message rate.
- **`icn_gossip_batches_rejected_oversized_total` increasing**: Potential DoS attempts or misconfigured peers. Review peer trust levels and rate limits.

## Best Practices

### When to Use Default Configuration
- General purpose gossip traffic
- Mixed message sizes
- Balanced latency/throughput requirements

### When to Use Low Latency
- Time-sensitive operations
- Interactive applications
- Small message volumes

### When to Use High Throughput
- Bulk data synchronization
- High message volumes
- Batch processing scenarios

### When to Disable Batching
- Testing individual message handling
- Extremely low-latency requirements
- Debugging message flows

## Implementation Details

### Thread Safety
Batching state uses `std::sync::Mutex` for thread-safe interior mutability. This allows batching from `&self` methods while maintaining API compatibility.

### Per-Recipient Batching
Messages are batched separately per recipient. This ensures:
- Optimal batch utilization per peer
- No cross-contamination between recipients
- Fair resource distribution

### Nested Batch Prevention
The protocol automatically rejects nested Batch messages to prevent:
- Recursive processing issues
- Unbounded memory growth
- Protocol confusion

### Batch Processing
Received batches are unpacked and processed as individual messages with:
- Proper error isolation (one failure doesn't affect others)
- Sequential ordering preservation
- Full message validation

## Examples

### Basic Usage
```rust
use icn_gossip::{GossipActor, BatchingConfig};

let mut gossip = GossipActor::new(did, trust_lookup);

// Set high throughput configuration
gossip.set_batching_config(BatchingConfig::high_throughput());

// Messages are automatically batched
for i in 0..100 {
    gossip.send_message(recipient, message);
}

// Ensure all messages are sent before shutdown
gossip.flush_all_batches();
```

### Custom Configuration for Specific Use Case
```rust
// Configure for real-time monitoring with frequent small updates
let config = BatchingConfig {
    max_batch_size: 3,                    // Small batches
    max_delay: Duration::from_millis(1),  // Very low delay
    compression_threshold: 512,            // Compress tiny batches
    enabled: true,
    max_batch_bytes: 64 * 1024,           // 64KB limit
};

gossip.set_batching_config(config);
```

## Future Enhancements

Potential future improvements:
1. **Dynamic Batch Sizing**: Adjust batch size based on network conditions
2. **Priority Batching**: Separate queues for high-priority messages
3. **Compression**: Implement zstd compression at network layer
4. **Adaptive Delays**: Adjust `max_delay` based on message frequency

## Related Documentation

- [Gossip Protocol](gossip-protocol.md)
- [Performance Tuning](performance-tuning.md)
- [Metrics Guide](metrics.md)
