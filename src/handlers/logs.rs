use actix_web::{web, HttpResponse, Result};
use validator::Validate;
use chrono::Utc;

use crate::models::{
    LogAnalysisRequest, LogAnalysisResponse, ErrorResponse
};
use crate::AppState;

pub async fn analyze_logs(
    state: web::Data<AppState>,
    req: web::Json<LogAnalysisRequest>,
) -> Result<HttpResponse> {
    // Validate request
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse::with_details(
            "Invalid request",
            format!("Validation error: {}", e)
        )));
    }

    // Get mutable reference to AI model
    let mut ai_model = state.ai_model.write().await;

    // Process the log analysis request
    match ai_model.analyze_logs(&req.logs, req.context.clone()).await {
        Ok(analysis) => {
            // Extract structured information from the analysis
            let issues: Vec<String> = analysis
                .lines()
                .filter(|line| line.to_lowercase().contains("issue") ||
                               line.to_lowercase().contains("error"))
                .take(5)
                .map(|s| s.trim().to_string())
                .collect();

            let recommendations: Vec<String> = analysis
                .lines()
                .filter(|line| line.to_lowercase().contains("recommend") ||
                               line.to_lowercase().contains("suggest"))
                .take(5)
                .map(|s| s.trim().to_string())
                .collect();

            let severity = if analysis.to_lowercase().contains("critical") {
                "critical".to_string()
            } else if analysis.to_lowercase().contains("error") {
                "high".to_string()
            } else if analysis.to_lowercase().contains("warning") {
                "medium".to_string()
            } else {
                "low".to_string()
            };

            let confidence = if analysis.len() > 500 { 0.8 } else { 0.6 };

            let response = LogAnalysisResponse {
                analysis,
                issues,
                recommendations,
                severity,
                confidence,
                timestamp: Utc::now(),
            };

            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            tracing::error!("Log analysis error: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse::with_details(
                "Failed to analyze logs",
                e.to_string()
            )))
        }
    }
}
