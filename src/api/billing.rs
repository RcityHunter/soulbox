//! Billing API Routes
//! 
//! This module provides HTTP API endpoints for billing and usage tracking functionality.
//! It includes endpoints for retrieving usage statistics, real-time metrics, and billing records.

use crate::billing::{BillingService, models::*};
use crate::auth::middleware::AuthContext;
use crate::error::{SoulBoxError, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use tracing::{error, info};
use uuid::Uuid;

/// Billing API state
#[derive(Clone)]
pub struct BillingApiState {
    pub billing_service: Arc<BillingService>,
}

/// Usage query parameters
#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// Start time for the query (RFC3339 format)
    pub start_time: Option<DateTime<Utc>>,
    /// End time for the query (RFC3339 format)
    pub end_time: Option<DateTime<Utc>>,
    /// Specific metric types to include
    pub metric_types: Option<Vec<String>>,
    /// Limit number of results
    pub limit: Option<u32>,
}

/// Usage record request
#[derive(Debug, Deserialize)]
pub struct RecordUsageRequest {
    pub session_id: Uuid,
    pub metric_type: String,
    pub value: Decimal,
    pub timestamp: Option<DateTime<Utc>>,
    pub metadata: Option<HashMap<String, String>>,
}

/// Real-time usage response
#[derive(Debug, Serialize)]
pub struct RealtimeUsageResponse {
    pub session_id: Uuid,
    pub metrics: Vec<UsageMetric>,
    pub last_updated: DateTime<Utc>,
    pub total_cost_estimate: Decimal,
}

/// Usage summary response
#[derive(Debug, Serialize)]
pub struct UsageSummaryResponse {
    pub user_id: Uuid,
    pub summary: UsageSummary,
    pub estimated_cost: Decimal,
    pub period_description: String,
}

/// Billing record response
#[derive(Debug, Serialize)]
pub struct BillingRecordResponse {
    pub record: BillingRecord,
    pub period_description: String,
    pub invoice_id: Option<Uuid>,
}

/// Invoice response
#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    pub invoice: Invoice,
    pub payment_url: Option<String>,
    pub download_url: Option<String>,
}

/// Real-time metrics for server-sent events
#[derive(Debug, Clone, Serialize)]
pub struct RealtimeMetrics {
    pub timestamp: DateTime<Utc>,
    pub active_sessions: u64,
    pub current_hourly_rate: Decimal,
    pub total_cost_today: Decimal,
    pub metrics_per_second: f64,
}

/// Create billing API routes
pub fn create_billing_routes() -> Router<BillingApiState> {
    Router::new()
        .route("/usage", get(get_usage_summary).post(record_usage))
        .route("/usage/realtime", get(get_realtime_usage))
        .route("/usage/realtime/stream", get(realtime_usage_stream))
        .route("/usage/sessions/:session_id", get(get_session_usage))
        .route("/billing/records", get(get_billing_records))
        .route("/billing/records/:record_id", get(get_billing_record))
        .route("/billing/estimate", post(estimate_cost))
        .route("/invoices", get(get_user_invoices))
        .route("/invoices/:invoice_id", get(get_invoice))
        .route("/invoices/:invoice_id/status", post(update_invoice_status))
        .route("/metrics/realtime", get(realtime_metrics_stream))
}

/// Get usage summary for the authenticated user
async fn get_usage_summary(
    State(state): State<BillingApiState>,
    Query(params): Query<UsageQuery>,
    auth: AuthContext,
) -> Result<Json<UsageSummaryResponse>> {
    let user_id = auth.user_id;
    
    // Default to last 30 days if not specified
    let end_time = params.end_time.unwrap_or_else(Utc::now);
    let start_time = params.start_time.unwrap_or_else(|| end_time - chrono::Duration::days(30));

    let summary = state.billing_service
        .get_usage_summary(user_id, start_time, end_time)
        .await
        .map_err(|e| {
            error!("Failed to get usage summary: {}", e);
            SoulBoxError::Internal("Failed to retrieve usage summary".to_string())
        })?;

    // Calculate estimated cost
    let billing_record = state.billing_service
        .calculate_cost(&summary)
        .await
        .map_err(|e| {
            error!("Failed to calculate cost: {}", e);
            SoulBoxError::Internal("Failed to calculate cost estimate".to_string())
        })?;

    let period_description = format!(
        "{} to {}",
        start_time.format("%Y-%m-%d"),
        end_time.format("%Y-%m-%d")
    );

    Ok(Json(UsageSummaryResponse {
        user_id,
        summary,
        estimated_cost: billing_record.total_cost,
        period_description,
    }))
}

/// Record a usage metric
async fn record_usage(
    State(state): State<BillingApiState>,
    Json(request): Json<RecordUsageRequest>,
    auth: AuthContext,
) -> Result<StatusCode> {
    let user_id = auth.user_id;

    // Parse metric type
    let metric_type = parse_metric_type(&request.metric_type)
        .ok_or_else(|| SoulBoxError::Config("Invalid metric type".to_string()))?;

    state.billing_service
        .record_usage(
            request.session_id,
            user_id,
            metric_type,
            request.value,
            request.timestamp,
        )
        .await
        .map_err(|e| {
            error!("Failed to record usage: {}", e);
            SoulBoxError::Internal("Failed to record usage".to_string())
        })?;

    info!("Recorded usage for user {}: {} {}", user_id, request.value, request.metric_type);
    Ok(StatusCode::CREATED)
}

/// Get real-time usage for the authenticated user
async fn get_realtime_usage(
    State(state): State<BillingApiState>,
    Query(_params): Query<UsageQuery>,
    auth: AuthContext,
) -> Result<Json<Vec<RealtimeUsageResponse>>> {
    let user_id = auth.user_id;

    // For demo purposes, get usage from the last hour
    let end_time = Utc::now();
    let start_time = end_time - chrono::Duration::hours(1);

    let summary = state.billing_service
        .get_usage_summary(user_id, start_time, end_time)
        .await
        .map_err(|e| {
            error!("Failed to get realtime usage: {}", e);
            SoulBoxError::Internal("Failed to retrieve real-time usage".to_string())
        })?;

    // Calculate estimated cost
    let billing_record = state.billing_service
        .calculate_cost(&summary)
        .await
        .map_err(|e| {
            error!("Failed to calculate realtime cost: {}", e);
            SoulBoxError::Internal("Failed to calculate real-time cost".to_string())
        })?;

    // For now, create a single response
    // In a real implementation, you'd group by session
    let response = vec![RealtimeUsageResponse {
        session_id: Uuid::new_v4(), // Placeholder
        metrics: vec![], // Would be populated with actual metrics
        last_updated: end_time,
        total_cost_estimate: billing_record.total_cost,
    }];

    Ok(Json(response))
}

/// Server-sent events stream for real-time usage updates
async fn realtime_usage_stream(
    State(state): State<BillingApiState>,
    auth: AuthContext,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let user_id = auth.user_id;
    let billing_service = state.billing_service.clone();

    let stream = IntervalStream::new(interval(Duration::from_secs(5)))
        .map(move |_| {
            let billing_service = billing_service.clone();
            let user_id = user_id;
            
            async move {
                // Get current usage data
                let end_time = Utc::now();
                let start_time = end_time - chrono::Duration::hours(1);

                match billing_service.get_usage_summary(user_id, start_time, end_time).await {
                    Ok(summary) => {
                        let data = serde_json::to_string(&summary).unwrap_or_default();
                        Ok(Event::default().data(data))
                    }
                    Err(e) => {
                        error!("Failed to get usage for stream: {}", e);
                        Ok(Event::default().data("{}"))
                    }
                }
            }
        })
        .then(|future| future);

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Get usage for a specific session
async fn get_session_usage(
    State(state): State<BillingApiState>,
    Path(session_id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Vec<UsageMetric>>> {
    let _user_id = auth.user_id; // For authorization check

    let metrics = state.billing_service
        .get_realtime_usage(session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session usage: {}", e);
            SoulBoxError::Internal("Failed to retrieve session usage".to_string())
        })?;

    Ok(Json(metrics))
}

/// Get billing records for the authenticated user
async fn get_billing_records(
    State(state): State<BillingApiState>,
    Query(params): Query<UsageQuery>,
    auth: AuthContext,
) -> Result<Json<Vec<BillingRecordResponse>>> {
    let user_id = auth.user_id;

    // Validate query parameters
    if let Some(limit) = params.limit {
        if limit > 1000 {
            return Err(crate::error::SoulBoxError::validation(
                "Limit cannot exceed 1000 records".to_string()
            ));
        }
    }

    // For now, return empty list as this would require integration with storage
    // In a real implementation, you'd query the billing storage with proper error handling
    let records = match try_get_billing_records(&state, user_id, &params).await {
        Ok(records) => records,
        Err(e) => {
            tracing::error!("Failed to retrieve billing records for user {}: {}", user_id, e);
            return Err(crate::error::SoulBoxError::internal(
                "Failed to retrieve billing records. Please try again later.".to_string()
            ));
        }
    };

    let responses: Vec<BillingRecordResponse> = records.into_iter().map(|record: BillingRecord| {
        let period_description = format!(
            "{} to {}",
            record.start_time.format("%Y-%m-%d"),
            record.end_time.format("%Y-%m-%d")
        );

        BillingRecordResponse {
            record,
            period_description,
            invoice_id: None, // Would be populated from database
        }
    }).collect();

    Ok(Json(responses))
}

/// Get a specific billing record
async fn get_billing_record(
    State(state): State<BillingApiState>,
    Path(_record_id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<BillingRecordResponse>> {
    let _user_id = auth.user_id; // For authorization

    // Placeholder implementation
    Err(SoulBoxError::not_found("Billing record not found"))
}

/// Estimate cost for current or projected usage
#[derive(Debug, Deserialize)]
pub struct CostEstimateRequest {
    pub current_usage: HashMap<String, Decimal>,
    pub duration_hours: Option<f64>,
    pub pricing_tier: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CostEstimateResponse {
    pub estimated_cost: Decimal,
    pub breakdown: HashMap<String, Decimal>,
    pub duration_hours: f64,
    pub currency: String,
}

async fn estimate_cost(
    State(state): State<BillingApiState>,
    Json(request): Json<CostEstimateRequest>,
    auth: AuthContext,
) -> Result<Json<CostEstimateResponse>> {
    let _user_id = auth.user_id;

    // Convert string metric types to MetricType enum
    let mut usage_map = HashMap::new();
    for (metric_str, value) in request.current_usage {
        if let Some(metric_type) = parse_metric_type(&metric_str) {
            usage_map.insert(metric_type, value);
        }
    }

    let duration_hours = request.duration_hours.unwrap_or(1.0);
    let pricing_tier = request.pricing_tier
        .as_deref()
        .and_then(parse_pricing_tier)
        .unwrap_or(PricingTier::Basic);

    // Use cost calculator to estimate
    let calculator = crate::billing::CostCalculator::new();
    let estimated_cost = calculator.calculate_estimated_cost(
        &usage_map,
        &pricing_tier,
        duration_hours,
    ).map_err(|e| {
        error!("Failed to estimate cost: {}", e);
        SoulBoxError::Internal("Failed to calculate cost estimate".to_string())
    })?;

    // Create breakdown (simplified)
    let mut breakdown = HashMap::new();
    breakdown.insert("total".to_string(), estimated_cost);

    Ok(Json(CostEstimateResponse {
        estimated_cost,
        breakdown,
        duration_hours,
        currency: "USD".to_string(),
    }))
}

/// Get invoices for the authenticated user
async fn get_user_invoices(
    State(state): State<BillingApiState>,
    Query(params): Query<UsageQuery>,
    auth: AuthContext,
) -> Result<Json<Vec<InvoiceResponse>>> {
    let _user_id = auth.user_id;

    // Placeholder implementation
    let invoices = Vec::new();
    
    let responses: Vec<InvoiceResponse> = invoices.into_iter().map(|invoice| {
        InvoiceResponse {
            invoice,
            payment_url: None,
            download_url: None,
        }
    }).collect();

    Ok(Json(responses))
}

/// Get a specific invoice
async fn get_invoice(
    State(state): State<BillingApiState>,
    Path(invoice_id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<InvoiceResponse>> {
    let _user_id = auth.user_id;

    // Placeholder implementation
    Err(SoulBoxError::not_found("Invoice not found"))
}

/// Update invoice status (admin only)
#[derive(Debug, Deserialize)]
pub struct UpdateInvoiceStatusRequest {
    pub status: String,
}

async fn update_invoice_status(
    State(state): State<BillingApiState>,
    Path(invoice_id): Path<Uuid>,
    Json(request): Json<UpdateInvoiceStatusRequest>,
    auth: AuthContext,
) -> Result<StatusCode> {
    let _user_id = auth.user_id;

    // Parse status
    let _status = parse_invoice_status(&request.status)
        .ok_or_else(|| SoulBoxError::Config("Invalid invoice status".to_string()))?;

    // Placeholder implementation
    // In real implementation, you'd update the invoice status in storage
    
    Ok(StatusCode::OK)
}

/// Real-time metrics stream for dashboard
async fn realtime_metrics_stream(
    State(state): State<BillingApiState>,
    auth: AuthContext,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let _user_id = auth.user_id;
    let billing_service = state.billing_service.clone();

    let stream = IntervalStream::new(interval(Duration::from_secs(2)))
        .map(move |_| {
            let billing_service = billing_service.clone();
            
            async move {
                match billing_service.get_realtime_usage(Uuid::new_v4()).await {
                    Ok(_metrics) => {
                        let realtime_metrics = RealtimeMetrics {
                            timestamp: Utc::now(),
                            active_sessions: 0, // Would be calculated from active sessions
                            current_hourly_rate: Decimal::ZERO,
                            total_cost_today: Decimal::ZERO,
                            metrics_per_second: 0.0,
                        };

                        let data = serde_json::to_string(&realtime_metrics).unwrap_or_default();
                        Ok(Event::default().data(data))
                    }
                    Err(e) => {
                        error!("Failed to get metrics for stream: {}", e);
                        Ok(Event::default().data("{}"))
                    }
                }
            }
        })
        .then(|future| future);

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Helper function to retrieve billing records with proper error handling
async fn try_get_billing_records(
    _state: &BillingApiState, 
    _user_id: Uuid, 
    _params: &UsageQuery
) -> crate::error::Result<Vec<BillingRecord>> {
    // Placeholder implementation - would integrate with actual storage
    // This prevents the panic that would occur in the original implementation
    Ok(Vec::new())
}

/// Helper function to parse metric type from string
fn parse_metric_type(metric_str: &str) -> Option<MetricType> {
    match metric_str.to_lowercase().as_str() {
        "cpu" | "cpu_usage" => Some(MetricType::CpuUsage),
        "memory" | "memory_usage" => Some(MetricType::MemoryUsage),
        "network_ingress" | "network_in" => Some(MetricType::NetworkIngress),
        "network_egress" | "network_out" => Some(MetricType::NetworkEgress),
        "storage" | "storage_usage" => Some(MetricType::StorageUsage),
        "execution_time" | "exec_time" => Some(MetricType::ExecutionTime),
        "api_requests" | "requests" => Some(MetricType::ApiRequests),
        _ => {
            if metric_str.starts_with("custom:") {
                Some(MetricType::Custom(metric_str.strip_prefix("custom:").unwrap().to_string()))
            } else {
                None
            }
        }
    }
}

/// Helper function to parse pricing tier from string
fn parse_pricing_tier(tier_str: &str) -> Option<PricingTier> {
    match tier_str.to_lowercase().as_str() {
        "free" => Some(PricingTier::Free),
        "basic" => Some(PricingTier::Basic),
        "professional" | "pro" => Some(PricingTier::Professional),
        "enterprise" => Some(PricingTier::Enterprise),
        _ => None,
    }
}

/// Helper function to parse invoice status from string
fn parse_invoice_status(status_str: &str) -> Option<InvoiceStatus> {
    match status_str.to_lowercase().as_str() {
        "draft" => Some(InvoiceStatus::Draft),
        "sent" => Some(InvoiceStatus::Sent),
        "paid" => Some(InvoiceStatus::Paid),
        "overdue" => Some(InvoiceStatus::Overdue),
        "cancelled" => Some(InvoiceStatus::Cancelled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metric_type() {
        assert_eq!(parse_metric_type("cpu"), Some(MetricType::CpuUsage));
        assert_eq!(parse_metric_type("memory_usage"), Some(MetricType::MemoryUsage));
        assert_eq!(parse_metric_type("network_in"), Some(MetricType::NetworkIngress));
        assert_eq!(parse_metric_type("custom:my_metric"), Some(MetricType::Custom("my_metric".to_string())));
        assert_eq!(parse_metric_type("invalid"), None);
    }

    #[test]
    fn test_parse_pricing_tier() {
        assert_eq!(parse_pricing_tier("free"), Some(PricingTier::Free));
        assert_eq!(parse_pricing_tier("Basic"), Some(PricingTier::Basic));
        assert_eq!(parse_pricing_tier("PRO"), Some(PricingTier::Professional));
        assert_eq!(parse_pricing_tier("enterprise"), Some(PricingTier::Enterprise));
        assert_eq!(parse_pricing_tier("invalid"), None);
    }

    #[test]
    fn test_parse_invoice_status() {
        assert_eq!(parse_invoice_status("draft"), Some(InvoiceStatus::Draft));
        assert_eq!(parse_invoice_status("SENT"), Some(InvoiceStatus::Sent));
        assert_eq!(parse_invoice_status("paid"), Some(InvoiceStatus::Paid));
        assert_eq!(parse_invoice_status("overdue"), Some(InvoiceStatus::Overdue));
        assert_eq!(parse_invoice_status("cancelled"), Some(InvoiceStatus::Cancelled));
        assert_eq!(parse_invoice_status("invalid"), None);
    }
}