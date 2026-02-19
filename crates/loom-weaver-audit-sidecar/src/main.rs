// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

mod buffer;
mod client;
mod config;
#[allow(dead_code)] // Planned for Phase 3: Socket lifecycle tracking
mod connection_tracker;
mod dns_cache;
mod event_processor;
mod events;
mod filter;
mod health;
#[cfg(all(feature = "ebpf", target_os = "linux"))]
mod loader;
mod metrics;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{routing::get, Router};
use tokio::sync::mpsc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::buffer::EventBuffer;
use crate::client::{AuditClient, BatchSender};
use crate::config::Config;
use crate::event_processor::{EventProcessor, EventProcessorConfig};
use crate::events::WeaverAuditEvent;
use crate::health::{health_router, HealthState};
#[cfg(all(feature = "ebpf", target_os = "linux"))]
use crate::loader::EbpfAuditLoader;
use crate::metrics::Metrics;

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::registry()
		.with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.with(tracing_subscriber::fmt::layer().json())
		.init();

	info!("Starting loom-audit-sidecar");

	let config = Config::from_env()?;
	info!(
		weaver_id = %config.weaver_id,
		org_id = %config.org_id,
		server_url = %config.server_url,
		"Loaded configuration"
	);

	let metrics = Arc::new(Metrics::new());
	let health_state = HealthState::new(config.buffer_max_bytes);

	let (buffer_tx, mut buffer_rx) = mpsc::channel::<WeaverAuditEvent>(10000);

	let audit_client = AuditClient::new(&config)?;
	let batch_sender = BatchSender::new(
		audit_client,
		config.batch_interval,
		buffer_tx.clone(),
		health_state.clone(),
		metrics.clone(),
	);

	let mut event_buffer =
		EventBuffer::new(PathBuf::from(&config.buffer_path), config.buffer_max_bytes)?;

	let buffer_health_state = health_state.clone();
	tokio::spawn(async move {
		while let Some(event) = buffer_rx.recv().await {
			if let Err(e) = event_buffer.append(&event) {
				warn!("Failed to buffer event: {}", e);
			}
			let _ = buffer_health_state
				.set_buffer_status(1, event_buffer.len())
				.await;
		}
	});

	let metrics_clone = metrics.clone();
	let metrics_port = config.metrics_port;
	tokio::spawn(async move {
		let app = Router::new().route(
			"/metrics",
			get(move || {
				let metrics = metrics_clone.clone();
				async move { metrics.encode() }
			}),
		);

		let addr = SocketAddr::from(([0, 0, 0, 0], metrics_port));
		info!("Metrics server listening on {}", addr);

		if let Err(e) = axum::serve(
			tokio::net::TcpListener::bind(addr).await.unwrap(),
			app.into_make_service(),
		)
		.await
		{
			warn!("Metrics server error: {}", e);
		}
	});

	let health_port = config.health_port;
	let health_router = health_router(health_state.clone());
	tokio::spawn(async move {
		let addr = SocketAddr::from(([0, 0, 0, 0], health_port));
		info!("Health server listening on {}", addr);

		if let Err(e) = axum::serve(
			tokio::net::TcpListener::bind(addr).await.unwrap(),
			health_router.into_make_service(),
		)
		.await
		{
			warn!("Health server error: {}", e);
		}
	});

	let flush_client = AuditClient::new(&config)?;
	let flush_buffer_path = PathBuf::from(&config.buffer_path);
	let flush_buffer_max_bytes = config.buffer_max_bytes;
	let flush_health = health_state.clone();
	let flush_metrics = metrics.clone();
	tokio::spawn(async move {
		let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
		loop {
			interval.tick().await;

			let mut buffer = match EventBuffer::new(flush_buffer_path.clone(), flush_buffer_max_bytes) {
				Ok(b) => b,
				Err(e) => {
					warn!("Failed to open buffer for flush: {}", e);
					continue;
				}
			};

			if buffer.is_empty() {
				continue;
			}

			if !flush_client.is_server_reachable().await {
				continue;
			}

			match buffer.read_all() {
				Ok(events) if !events.is_empty() => {
					let event_count = events.len();
					info!(count = event_count, "Flushing buffered events");
					match flush_client.send_events(&events).await {
						Ok(()) => {
							if let Err(e) = buffer.clear() {
								warn!("Failed to clear buffer after flush: {}", e);
							}
							flush_health.record_successful_send().await;
							flush_health.set_buffer_status(0, 0).await;
							flush_metrics.record_batch_sent(true, event_count, 0);
						}
						Err(e) => {
							warn!("Failed to flush buffered events: {}", e);
							flush_metrics.record_batch_sent(false, event_count, 0);
						}
					}
				}
				Ok(_) => {}
				Err(e) => {
					warn!("Failed to read buffered events: {}", e);
				}
			}
		}
	});

	let processor_config = EventProcessorConfig::from(&config);
	let event_processor = Arc::new(EventProcessor::new(
		processor_config,
		batch_sender.sender().clone(),
		metrics.clone(),
	));

	// Bounded channel for raw eBPF events to prevent OOM from unbounded task spawning
	#[allow(unused_variables)]
	let (raw_event_tx, mut raw_event_rx) = mpsc::channel::<Vec<u8>>(1000);

	// Spawn consumer task to process raw events
	let processor = event_processor.clone();
	tokio::spawn(async move {
		while let Some(data) = raw_event_rx.recv().await {
			processor.process_raw_event(&data).await;
		}
	});

	#[cfg(all(feature = "ebpf", target_os = "linux"))]
	let ebpf_loaded = match EbpfAuditLoader::new() {
		Ok(loader) => {
			let attached = loader.attached_count();
			let total = loader.total_programs();
			info!(attached, total, "eBPF programs loaded successfully");
			health_state
				.set_ebpf_status(attached as u32, total as u32)
				.await;

			let raw_tx = raw_event_tx.clone();
			tokio::task::spawn_blocking(move || {
				let mut loader = loader;
				match loader.ring_buffer() {
					Ok(mut ring_buf) => {
						info!("Ring buffer polling started");
						loop {
							while let Some(item) = ring_buf.next() {
								let data = item.as_ref().to_vec();
								// Use blocking_send - blocks if channel is full (backpressure)
								if raw_tx.blocking_send(data).is_err() {
									warn!("Event channel closed, stopping ring buffer polling");
									return;
								}
							}
							std::thread::sleep(std::time::Duration::from_millis(10));
						}
					}
					Err(e) => {
						warn!(error = %e, "Failed to get ring buffer, events will not be processed");
					}
				}
			});

			true
		}
		Err(e) => {
			warn!(error = %e, "Failed to load eBPF programs");
			health_state.set_ebpf_status(0, 5).await;
			false
		}
	};

	#[cfg(not(all(feature = "ebpf", target_os = "linux")))]
	let ebpf_loaded = {
		info!("eBPF not available (feature disabled or not on Linux), running in stub mode");
		health_state.set_ebpf_status(0, 0).await;
		false
	};

	if ebpf_loaded {
		info!("Audit sidecar running with eBPF monitoring");
	} else {
		info!("Audit sidecar running without eBPF (stub mode)");
	}

	info!("Waiting for shutdown signal...");

	tokio::signal::ctrl_c().await?;
	info!("Shutting down");

	Ok(())
}
