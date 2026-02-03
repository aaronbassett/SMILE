# Bollard Crate - Advanced Patterns & Edge Cases

Deep dive into advanced usage patterns, edge cases, and production deployment strategies.

## Table of Contents

1. [Stream Processing Patterns](#stream-processing-patterns)
2. [Error Recovery Strategies](#error-recovery-strategies)
3. [Resource Management](#resource-management)
4. [Container Orchestration](#container-orchestration)
5. [Logging & Observability](#logging--observability)
6. [Performance Optimization](#performance-optimization)
7. [Edge Cases & Gotchas](#edge-cases--gotchas)
8. [Testing Strategies](#testing-strategies)

---

## Stream Processing Patterns

### Pattern 1: Buffered Stream Processing

When dealing with large outputs, process streams in chunks rather than accumulating everything in memory:

```rust
use futures::stream::StreamExt;
use std::io::Write;

/// Process container logs in chunks with bounded memory
pub async fn stream_logs_bounded(
    docker: &Docker,
    container_id: &str,
    max_buffer_size: usize,
) -> Result<()> {
    let mut file = std::fs::File::create("container.log")?;
    let mut buffer = Vec::with_capacity(max_buffer_size);

    let logs = docker.logs::<&str>(
        container_id,
        Some(bollard::container::LogsOptions::default()),
    ).await?;

    futures::pin_mut!(logs);

    while let Some(result) = logs.next().await {
        match result {
            Ok(msg) => {
                // Get the log output
                let log_bytes = match msg {
                    bollard::container::LogOutput::StdOut { message } |
                    bollard::container::LogOutput::StdErr { message } => {
                        message.to_vec()
                    }
                    _ => continue,
                };

                // Add to buffer
                buffer.extend_from_slice(&log_bytes);

                // Flush when buffer is full
                if buffer.len() >= max_buffer_size {
                    file.write_all(&buffer)?;
                    buffer.clear();
                }
            }
            Err(e) => {
                warn!("Error reading logs: {}", e);
                break;
            }
        }
    }

    // Final flush
    if !buffer.is_empty() {
        file.write_all(&buffer)?;
    }

    Ok(())
}
```

### Pattern 2: Stream with Backpressure

Handle streams that produce data faster than you can process:

```rust
use tokio::sync::mpsc;
use futures::stream::StreamExt;

/// Stream logs with backpressure handling
pub async fn stream_logs_with_backpressure(
    docker: &Docker,
    container_id: &str,
    channel_capacity: usize,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(channel_capacity);

    // Producer task
    let docker_clone = std::sync::Arc::new(docker.clone());
    let container_id_clone = container_id.to_string();

    tokio::spawn(async move {
        let logs = docker_clone.logs::<&str>(
            &container_id_clone,
            Some(bollard::container::LogsOptions::default()),
        ).await;

        if let Ok(stream) = logs {
            futures::pin_mut!(stream);
            while let Some(result) = stream.next().await {
                if let Ok(msg) = result {
                    if let Err(e) = tx.send(msg).await {
                        error!("Channel send failed: {}", e);
                        break;
                    }
                }
            }
        }
    });

    // Consumer task
    while let Some(msg) = rx.recv().await {
        process_log_message(msg).await?;
    }

    Ok(())
}

async fn process_log_message(msg: bollard::container::LogOutput) -> Result<()> {
    // Simulate slow processing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}
```

### Pattern 3: Stream Error Recovery

Gracefully handle errors within streams:

```rust
/// Resilient log streaming with error recovery
pub async fn resilient_log_stream(
    docker: &Docker,
    container_id: &str,
    max_errors: u32,
) -> Result<Vec<String>> {
    let mut logs = Vec::new();
    let mut error_count = 0;
    const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

    loop {
        match docker.logs::<&str>(
            container_id,
            Some(bollard::container::LogsOptions::default()),
        ).await {
            Ok(stream) => {
                error_count = 0;
                futures::pin_mut!(stream);

                while let Some(result) = stream.next().await {
                    match result {
                        Ok(msg) => {
                            let log_line = match msg {
                                bollard::container::LogOutput::StdOut { message } => {
                                    String::from_utf8_lossy(&message).to_string()
                                }
                                bollard::container::LogOutput::StdErr { message } => {
                                    String::from_utf8_lossy(&message).to_string()
                                }
                                _ => continue,
                            };
                            logs.push(log_line);
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            error_count += 1;
                            if error_count > max_errors {
                                return Err(anyhow::anyhow!(
                                    "Too many stream errors: {}",
                                    error_count
                                ));
                            }
                            // Retry after delay
                            tokio::time::sleep(RETRY_DELAY).await;
                            break;
                        }
                    }
                }

                if error_count == 0 {
                    break; // Successfully completed
                }
            }
            Err(e) => {
                error!("Failed to get logs: {}", e);
                error_count += 1;
                if error_count > max_errors {
                    return Err(anyhow::anyhow!("Too many connection errors"));
                }
                tokio::time::sleep(RETRY_DELAY).await;
            }
        }
    }

    Ok(logs)
}
```

---

## Error Recovery Strategies

### Pattern 1: Exponential Backoff with Jitter

```rust
use rand::Rng;
use std::time::Duration;

/// Execute operation with exponential backoff and jitter
pub async fn retry_with_backoff<F, T>(
    operation: &mut F,
    max_retries: u32,
) -> Result<T>
where
    F: FnMut() -> futures::future::BoxFuture<'static, Result<T>>,
{
    let mut rng = rand::thread_rng();
    let base_delay_ms = 100u64;
    let max_delay_ms = 30000u64; // 30 seconds

    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    info!("Operation succeeded on attempt {}", attempt + 1);
                }
                return Ok(result);
            }
            Err(e) if attempt < max_retries => {
                // Calculate exponential backoff: base * 2^attempt
                let base_wait = base_delay_ms * 2u64.pow(attempt);
                let max_wait = base_wait.min(max_delay_ms);

                // Add jitter: random between 0 and max_wait
                let jitter = rng.gen_range(0..=max_wait);

                warn!(
                    "Attempt {} failed: {}. Retrying in {}ms (base: {}ms)",
                    attempt + 1,
                    e,
                    jitter,
                    base_wait
                );

                tokio::time::sleep(Duration::from_millis(jitter)).await;
            }
            Err(e) => return Err(e),
        }
    }

    Err(anyhow::anyhow!(
        "Operation failed after {} attempts",
        max_retries + 1
    ))
}
```

### Pattern 2: Circuit Breaker

Prevent cascading failures by stopping retries when too many failures occur:

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CircuitBreaker {
    failure_count: Arc<AtomicU32>,
    failure_threshold: u32,
    last_failure_time: Arc<AtomicU64>,
    cooldown_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,    // Operating normally
    Open,      // Too many failures, rejecting requests
    HalfOpen,  // Testing if service recovered
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown_seconds: u64) -> Self {
        Self {
            failure_count: Arc::new(AtomicU32::new(0)),
            failure_threshold,
            last_failure_time: Arc::new(AtomicU64::new(0)),
            cooldown_seconds,
        }
    }

    pub fn state(&self) -> CircuitState {
        let failures = self.failure_count.load(Ordering::SeqCst);

        if failures >= self.failure_threshold {
            let last_failure = self.last_failure_time.load(Ordering::SeqCst);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now - last_failure > self.cooldown_seconds {
                CircuitState::HalfOpen // Try again after cooldown
            } else {
                CircuitState::Open // Still in cooldown
            }
        } else {
            CircuitState::Closed
        }
    }

    pub async fn execute<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce() -> futures::future::BoxFuture<'static, Result<T>>,
    {
        match self.state() {
            CircuitState::Open => {
                return Err(anyhow::anyhow!("Circuit breaker is open"));
            }
            CircuitState::HalfOpen => {
                info!("Circuit breaker is half-open, testing recovery");
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CircuitState::Closed => {}
        }

        match operation().await {
            Ok(result) => {
                // Success, reset failure count
                self.failure_count.store(0, Ordering::SeqCst);
                Ok(result)
            }
            Err(e) => {
                // Record failure
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                self.last_failure_time.store(now, Ordering::SeqCst);
                self.failure_count.fetch_add(1, Ordering::SeqCst);

                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let breaker = CircuitBreaker::new(3, 1);

        // Simulate failures
        for _ in 0..3 {
            let _ = breaker.execute(|| {
                Box::pin(async { Err::<(), _>(anyhow::anyhow!("failed")) })
            }).await;
        }

        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for cooldown
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
    }
}
```

### Pattern 3: Timeout Strategies

```rust
use tokio::time::{timeout, Duration, sleep};

pub enum TimeoutStrategy {
    Fixed(Duration),
    Exponential { base: Duration, max: Duration },
    Adaptive { min: Duration, max: Duration },
}

impl TimeoutStrategy {
    pub fn get_timeout(&self, attempt: u32) -> Duration {
        match self {
            TimeoutStrategy::Fixed(d) => *d,
            TimeoutStrategy::Exponential { base, max } => {
                let calculated = *base * 2_u32.pow(attempt).min(32); // Cap at 2^32
                calculated.min(*max)
            }
            TimeoutStrategy::Adaptive { min, max } => {
                // Could implement based on historical response times
                *min
            }
        }
    }
}

pub async fn execute_with_timeout_strategy<F, T>(
    operation: F,
    strategy: TimeoutStrategy,
    max_attempts: u32,
) -> Result<T>
where
    F: Fn(Duration) -> futures::future::BoxFuture<'static, Result<T>>,
{
    for attempt in 0..max_attempts {
        let timeout_duration = strategy.get_timeout(attempt);

        match timeout(timeout_duration, operation(timeout_duration)).await {
            Ok(Ok(result)) => return Ok(result),
            Ok(Err(e)) if attempt < max_attempts - 1 => {
                warn!("Attempt {} failed: {}. Retrying", attempt + 1, e);
                continue;
            }
            Ok(Err(e)) => return Err(e),
            Err(_) if attempt < max_attempts - 1 => {
                warn!(
                    "Attempt {} timed out after {:?}. Retrying",
                    attempt + 1,
                    timeout_duration
                );
                continue;
            }
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "Operation timed out after {:?}",
                    timeout_duration
                ))
            }
        }
    }

    Err(anyhow::anyhow!("All attempts exhausted"))
}
```

---

## Resource Management

### Pattern 1: Resource Pool

Manage containers as a pool for reuse:

```rust
use std::collections::VecDeque;
use tokio::sync::Mutex;

pub struct ContainerPool {
    docker: Arc<Docker>,
    available: Arc<Mutex<VecDeque<String>>>,
    in_use: Arc<Mutex<std::collections::HashSet<String>>>,
    max_size: usize,
    image: String,
}

impl ContainerPool {
    pub async fn new(
        docker: Arc<Docker>,
        image: String,
        pool_size: usize,
    ) -> Result<Self> {
        let pool = Self {
            docker,
            available: Arc::new(Mutex::new(VecDeque::new())),
            in_use: Arc::new(Mutex::new(std::collections::HashSet::new())),
            max_size: pool_size,
            image,
        };

        pool.initialize().await?;
        Ok(pool)
    }

    async fn initialize(&self) -> Result<()> {
        let mut available = self.available.lock().await;

        for i in 0..self.max_size {
            let container_name = format!("pool-container-{}", i);
            let config = Config {
                image: Some(self.image.clone()),
                ..Default::default()
            };

            let container = self.docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: &container_name,
                    }),
                    config,
                )
                .await?;

            self.docker
                .start_container::<String>(&container.id, None)
                .await?;

            available.push_back(container.id);
        }

        Ok(())
    }

    pub async fn acquire(&self) -> Result<PooledContainer> {
        let mut available = self.available.lock().await;

        if let Some(id) = available.pop_front() {
            let mut in_use = self.in_use.lock().await;
            in_use.insert(id.clone());

            Ok(PooledContainer {
                pool: self.clone(),
                id,
            })
        } else {
            Err(anyhow::anyhow!("No available containers in pool"))
        }
    }

    async fn release(&self, id: String) {
        let mut in_use = self.in_use.lock().await;
        in_use.remove(&id);

        let mut available = self.available.lock().await;
        available.push_back(id);
    }

    pub async fn cleanup(&self) -> Result<()> {
        let available = self.available.lock().await;
        let in_use = self.in_use.lock().await;

        for id in available.iter().chain(in_use.iter()) {
            let _ = self.docker
                .remove_container(id, Some(RemoveContainerOptions { force: true, ..Default::default() }))
                .await;
        }

        Ok(())
    }
}

impl Clone for ContainerPool {
    fn clone(&self) -> Self {
        Self {
            docker: Arc::clone(&self.docker),
            available: Arc::clone(&self.available),
            in_use: Arc::clone(&self.in_use),
            max_size: self.max_size,
            image: self.image.clone(),
        }
    }
}

pub struct PooledContainer {
    pool: ContainerPool,
    id: String,
}

impl PooledContainer {
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for PooledContainer {
    fn drop(&mut self) {
        let pool = self.pool.clone();
        let id = self.id.clone();

        tokio::spawn(async move {
            pool.release(id).await;
        });
    }
}
```

### Pattern 2: Resource Limits

Set memory and CPU limits for containers:

```rust
pub struct ResourceLimits {
    pub memory_bytes: i64,
    pub cpus: f64,
    pub cpu_period: i64,
}

pub async fn start_container_limited(
    docker: &Docker,
    name: &str,
    image: &str,
    limits: ResourceLimits,
) -> Result<String> {
    let host_config = HostConfig {
        memory: Some(limits.memory_bytes),
        cpu_period: Some(limits.cpu_period),
        cpus_str: Some(limits.cpus.to_string()),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let container = docker
        .create_container(Some(CreateContainerOptions { name }), config)
        .await?;

    docker
        .start_container::<String>(&container.id, None)
        .await?;

    Ok(container.id)
}
```

---

## Container Orchestration

### Pattern 1: Coordinated Multi-Container Setup

```rust
pub struct ContainerOrchestrator {
    docker: Arc<Docker>,
    containers: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl ContainerOrchestrator {
    pub async fn new() -> Result<Self> {
        let docker = Arc::new(Docker::connect_with_local_defaults()?);
        docker.version().await?;

        Ok(Self {
            docker,
            containers: Arc::new(Mutex::new(std::collections::HashMap::new())),
        })
    }

    pub async fn start_services(
        &self,
        services: Vec<ServiceSpec>,
    ) -> Result<()> {
        // Start services in dependency order
        for service in services {
            info!("Starting service: {}", service.name);

            let config = Config {
                image: Some(service.image),
                env: service.env,
                host_config: Some(HostConfig {
                    extra_hosts: Some(vec![
                        "host.docker.internal:host-gateway".to_string(),
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let container = self.docker
                .create_container(
                    Some(CreateContainerOptions { name: &service.name }),
                    config,
                )
                .await?;

            self.docker
                .start_container::<String>(&container.id, None)
                .await?;

            let mut containers = self.containers.lock().await;
            containers.insert(service.name.clone(), container.id);

            // Wait for readiness probe
            if let Some(probe) = &service.readiness_probe {
                self.wait_for_readiness(
                    &container.id,
                    probe,
                ).await?;
            }
        }

        Ok(())
    }

    async fn wait_for_readiness(
        &self,
        container_id: &str,
        probe: &ReadinessProbe,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        loop {
            let output = execute_command_complete(
                &self.docker,
                container_id,
                probe.command.clone(),
                5,
            )
            .await;

            if output.is_ok() && output.unwrap().exit_code == 0 {
                info!("Readiness probe passed");
                return Ok(());
            }

            if start.elapsed() > probe.timeout {
                return Err(anyhow::anyhow!("Readiness probe timed out"));
            }

            tokio::time::sleep(probe.period).await;
        }
    }

    pub async fn cleanup_all(&self) -> Result<()> {
        let containers = self.containers.lock().await;

        for (name, id) in containers.iter() {
            info!("Stopping service: {}", name);
            let _ = self.docker.stop_container(id, Some(10)).await;
            let _ = self.docker.remove_container(
                id,
                Some(RemoveContainerOptions { force: true, ..Default::default() }),
            ).await;
        }

        Ok(())
    }
}

pub struct ServiceSpec {
    pub name: String,
    pub image: String,
    pub env: Option<Vec<String>>,
    pub readiness_probe: Option<ReadinessProbe>,
}

pub struct ReadinessProbe {
    pub command: Vec<String>,
    pub timeout: Duration,
    pub period: Duration,
}
```

---

## Logging & Observability

### Pattern 1: Structured Logging for Container Operations

```rust
use serde::Serialize;

#[derive(Serialize)]
struct ContainerEvent {
    timestamp: String,
    event_type: String,
    container_id: String,
    container_name: String,
    status: String,
    details: serde_json::Value,
}

pub async fn log_container_event(
    container_id: &str,
    event_type: &str,
    status: &str,
) {
    let event = ContainerEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event_type: event_type.to_string(),
        container_id: container_id.to_string(),
        container_name: String::new(),
        status: status.to_string(),
        details: serde_json::json!({}),
    };

    info!(
        event = ?event,
        "Container event"
    );
}
```

### Pattern 2: Metrics Collection

```rust
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

pub struct ContainerMetrics {
    operations_total: Arc<AtomicU64>,
    operations_failed: Arc<AtomicU64>,
    exec_duration_ms: Arc<Mutex<Vec<u64>>>,
}

impl ContainerMetrics {
    pub fn new() -> Self {
        Self {
            operations_total: Arc::new(AtomicU64::new(0)),
            operations_failed: Arc::new(AtomicU64::new(0)),
            exec_duration_ms: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn record_operation(&self, success: bool, duration_ms: u64) {
        self.operations_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if !success {
            self.operations_failed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        let mut durations = self.exec_duration_ms.lock().await;
        durations.push(duration_ms);
    }

    pub async fn summary(&self) -> String {
        let total = self.operations_total.load(std::sync::atomic::Ordering::Relaxed);
        let failed = self.operations_failed.load(std::sync::atomic::Ordering::Relaxed);
        let durations = self.exec_duration_ms.lock().await;

        let avg_duration = if durations.is_empty() {
            0
        } else {
            durations.iter().sum::<u64>() / durations.len() as u64
        };

        format!(
            "Total: {}, Failed: {}, Avg Duration: {}ms",
            total, failed, avg_duration
        )
    }
}
```

---

## Performance Optimization

### Pattern 1: Parallel Container Operations

```rust
pub async fn execute_parallel_commands(
    docker: &Docker,
    container_ids: Vec<&str>,
    cmd: Vec<&str>,
    max_concurrent: usize,
) -> Result<Vec<(String, Result<String>)>> {
    use futures::stream::{self, StreamExt};

    let stream = stream::iter(container_ids.into_iter().map(|id| {
        let docker = docker.clone();
        let cmd = cmd.clone();
        async move {
            let result = execute_command_complete(&docker, id, cmd, 30).await
                .map(|output| output.stdout);
            (id.to_string(), result)
        }
    }))
    .buffered(max_concurrent);

    Ok(stream.collect().await)
}
```

### Pattern 2: Lazy Container Creation

```rust
pub struct LazyContainerPool {
    docker: Arc<Docker>,
    containers: Arc<Mutex<std::collections::HashMap<String, String>>>,
    config: Arc<Config>,
}

impl LazyContainerPool {
    pub async fn get_or_create(&self, name: &str) -> Result<String> {
        let mut containers = self.containers.lock().await;

        if let Some(id) = containers.get(name) {
            return Ok(id.clone());
        }

        // Create on demand
        let container = self.docker
            .create_container(
                Some(CreateContainerOptions { name }),
                (*self.config).clone(),
            )
            .await?;

        self.docker
            .start_container::<String>(&container.id, None)
            .await?;

        containers.insert(name.to_string(), container.id.clone());
        Ok(container.id)
    }
}
```

---

## Edge Cases & Gotchas

### Gotcha 1: Docker API Versions

Different Docker versions support different API features:

```rust
pub async fn check_docker_compatibility() -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    let version = docker.version().await?;

    if let Some(api_version) = version.api_version {
        let parts: Vec<&str> = api_version.split('.').collect();
        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            if major < 1 || (major == 1 && minor < 40) {
                warn!(
                    "Docker API version {} is older than recommended",
                    api_version
                );
            }
        }
    }

    Ok(())
}
```

### Gotcha 2: Container Name Conflicts

Container names must be unique across the entire daemon:

```rust
pub async fn safe_container_name(
    docker: &Docker,
    preferred_name: &str,
) -> Result<String> {
    let mut name = preferred_name.to_string();
    let mut counter = 0;

    loop {
        if docker.inspect_container(&name).await.is_err() {
            return Ok(name);
        }

        counter += 1;
        name = format!("{}-{}", preferred_name, counter);

        if counter > 100 {
            return Err(anyhow::anyhow!("Too many containers with name: {}", preferred_name));
        }
    }
}
```

### Gotcha 3: Signal Handling in Containers

Containers may not respond to signals properly:

```rust
pub async fn graceful_container_stop(
    docker: &Docker,
    container_id: &str,
    graceful_timeout: Duration,
) -> Result<()> {
    // First try SIGTERM
    docker.kill_container::<String>(container_id, None).await.ok();

    // Wait for graceful shutdown
    tokio::time::sleep(graceful_timeout).await;

    // Check if it stopped
    match docker.inspect_container(container_id).await {
        Ok(c) => {
            let state = c.state
                .as_ref()
                .and_then(|s| s.status.as_deref())
                .unwrap_or("unknown");

            if state != "exited" {
                // Force kill
                docker.kill_container::<String>(container_id, None).await?;
            }
        }
        Err(_) => {} // Already gone
    }

    Ok(())
}
```

---

## Testing Strategies

### Pattern 1: Test Container Fixtures

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct TestFixture {
        docker: Docker,
        container_id: String,
    }

    impl TestFixture {
        async fn new() -> Result<Self> {
            let docker = Docker::connect_with_local_defaults()?;

            let config = Config {
                image: Some("alpine:latest".to_string()),
                cmd: Some(vec!["tail".to_string(), "-f".to_string(), "/dev/null".to_string()]),
                ..Default::default()
            };

            let container = docker
                .create_container(
                    Some(CreateContainerOptions { name: "test-fixture" }),
                    config,
                )
                .await?;

            docker
                .start_container::<String>(&container.id, None)
                .await?;

            Ok(Self {
                docker,
                container_id: container.id,
            })
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            let docker = self.docker.clone();
            let id = self.container_id.clone();

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let _ = docker
                        .remove_container(
                            &id,
                            Some(RemoveContainerOptions { force: true, ..Default::default() }),
                        )
                        .await;
                });
            });
        }
    }

    #[tokio::test]
    async fn test_execute_command() -> Result<()> {
        let fixture = TestFixture::new().await?;

        let output = execute_command_complete(
            &fixture.docker,
            &fixture.container_id,
            vec!["echo", "test"],
            5,
        )
        .await?;

        assert_eq!(output.exit_code, 0);
        Ok(())
    }
}
```

