use actix_web::{web, HttpResponse, Result};
use validator::Validate;
use chrono::Utc;

use crate::models::{
    ScriptGenerationRequest, ScriptResponse, ErrorResponse, Environment, ScriptLanguage
};
use crate::AppState;

pub async fn generate_script(
    state: web::Data<AppState>,
    req: web::Json<ScriptGenerationRequest>,
) -> Result<HttpResponse> {
    // Validate request
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse::with_details(
            "Invalid request",
            format!("Validation error: {}", e)
        )));
    }

    // Convert environment and language to strings
    let environment_str = match req.environment {
        Environment::Linux => "linux",
        Environment::Windows => "windows",
        Environment::MacOS => "macos",
    };

    let language_str = match req.language {
        ScriptLanguage::Bash => "bash",
        ScriptLanguage::Python => "python",
        ScriptLanguage::Powershell => "powershell",
    };

    // Get mutable reference to AI model
    let mut ai_model = state.ai_model.write().await;

    // Process the script generation request
    match ai_model.generate_script(&req.requirement, environment_str, language_str).await {
        Ok(script_content) => {
            // Parse the response to extract script, explanation, and warnings
            let parts: Vec<&str> = script_content.split("\n\n").collect();

            let script = parts.get(0)
                .unwrap_or(&"# No script generated")
                .trim_start_matches("Script:\n")
                .to_string();

            let explanation = parts.get(1)
                .and_then(|p| p.strip_prefix("Explanation:"))
                .unwrap_or("Script generated based on requirements")
                .trim()
                .to_string();

            let safety_warnings: Vec<String> = vec![
                "Test scripts in a non-production environment first".to_string(),
                "Review script contents before execution".to_string(),
                "Ensure proper backups are in place".to_string(),
            ];

            let response = ScriptResponse {
                script,
                language: language_str.to_string(),
                environment: environment_str.to_string(),
                explanation,
                safety_warnings,
                timestamp: Utc::now(),
            };

            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            tracing::error!("Script generation error: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse::with_details(
                "Failed to generate script",
                e.to_string()
            )))
        }
    }
}
