//! Socket latency test tool
//!
//! Measures round-trip latency for GetWidgets requests over Unix socket.
//!
//! Usage:
//!   cargo run --example socket_latency_test /path/to/plugin.sock [iterations]

use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use waft_ipc::{OverviewMessage, PluginMessage};

const DEFAULT_ITERATIONS: usize = 100;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <socket_path> [iterations]", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    let iterations = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ITERATIONS);

    println!("Socket latency test");
    println!("Socket: {}", socket_path);
    println!("Iterations: {}", iterations);
    println!();

    // Warmup
    println!("Warmup (10 requests)...");
    for _ in 0..10 {
        if let Err(e) = measure_once(socket_path).await {
            eprintln!("Warmup failed: {}", e);
            std::process::exit(1);
        }
    }

    println!("Starting measurement...");
    let mut latencies = Vec::with_capacity(iterations);

    for i in 0..iterations {
        match measure_once(socket_path).await {
            Ok(latency_ms) => {
                latencies.push(latency_ms);
                if (i + 1) % 10 == 0 {
                    print!(".");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
            }
            Err(e) => {
                eprintln!("\nError on iteration {}: {}", i + 1, e);
                break;
            }
        }
    }
    println!();

    if latencies.is_empty() {
        eprintln!("No successful measurements");
        std::process::exit(1);
    }

    // Calculate statistics
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let sum: f64 = latencies.iter().sum();
    let avg = sum / latencies.len() as f64;
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

    println!();
    println!("Results:");
    println!("  Successful: {}/{}", latencies.len(), iterations);
    println!("  Average: {:.2} ms", avg);
    println!("  Minimum: {:.2} ms", min);
    println!("  Maximum: {:.2} ms", max);
    println!("  P50 (median): {:.2} ms", p50);
    println!("  P95: {:.2} ms", p95);
    println!("  P99: {:.2} ms", p99);

    // CSV output for scripting
    println!();
    println!("CSV format (iteration,latency_ms):");
    for (i, latency) in latencies.iter().enumerate() {
        println!("{},{:.2}", i + 1, latency);
    }

    Ok(())
}

/// Measure single round-trip latency
async fn measure_once(socket_path: &str) -> Result<f64, Box<dyn std::error::Error>> {
    // Connect
    let mut stream = UnixStream::connect(socket_path).await?;

    // Prepare GetWidgets message
    let msg = OverviewMessage::GetWidgets;
    let payload = serde_json::to_vec(&msg)?;
    let len_bytes = (payload.len() as u32).to_be_bytes();

    // Measure round-trip
    let start = Instant::now();

    // Send request
    stream.write_all(&len_bytes).await?;
    stream.write_all(&payload).await?;

    // Read response length
    let mut resp_len_bytes = [0u8; 4];
    stream.read_exact(&mut resp_len_bytes).await?;
    let resp_len = u32::from_be_bytes(resp_len_bytes) as usize;

    // Read response payload
    let mut resp_payload = vec![0u8; resp_len];
    stream.read_exact(&mut resp_payload).await?;

    let elapsed = start.elapsed();

    // Parse response to verify it's valid
    let _response: PluginMessage = serde_json::from_slice(&resp_payload)?;

    Ok(elapsed.as_secs_f64() * 1000.0)
}
