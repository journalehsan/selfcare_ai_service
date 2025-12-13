pub fn generate_chat_prompt(message: &str, conversation_id: Option<String>) -> String {
    let context = if let Some(id) = conversation_id {
        format!("\n[Conversation ID: {}]", id)
    } else {
        String::new()
    };

    format!(
        r#"You are a helpful AI assistant specializing in troubleshooting and technical support.{}

User: {}
Assistant: "#,
        context, message
    )
}

pub fn generate_log_analysis_prompt(logs: &str, context: Option<String>) -> String {
    let context_info = context.unwrap_or_else(|| "No additional context provided".to_string());

    format!(
        r#"You are an expert system administrator and DevOps engineer analyzing system logs.

Context: {}

Logs to analyze:
{}

Please analyze these logs and provide:
1. A summary of what's happening
2. Any errors, warnings, or issues identified
3. Potential root causes
4. Specific recommendations to resolve issues
5. Severity assessment (low/medium/high/critical)

Focus on actionable insights and be specific about file names, timestamps, and error codes when available."#,
        context_info, logs
    )
}

pub fn generate_script_prompt(requirement: &str, environment: &str, language: &str) -> String {
    format!(
        r#"You are an expert DevOps engineer and system administrator. Generate a script based on the following requirements:

Requirement: {}
Target Environment: {}
Script Language: {}

Please provide:
1. A complete, working script that addresses the requirement
2. Inline comments explaining key parts of the script
3. Error handling where appropriate
4. Safety considerations
5. Brief explanation of how the script works

Ensure the script:
- Is production-ready and follows best practices
- Includes proper error handling
- Has clear documentation
- Is safe to run (include warnings if destructive operations are needed)
- Is optimized for the specified environment

Script:"#,
        requirement, environment, language
    )
}
